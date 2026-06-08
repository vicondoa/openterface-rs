//! Session orchestration: video + input + serial.
//!
//! A [`Session`] owns the worker threads and an explicit **shutdown model** so
//! it never deadlocks or hangs on stop, even on device disconnect:
//!
//! - a **serial-writer thread** drains an input-event channel into the
//!   [`crate::pacing::PacingScheduler`] and writes paced CH9329 frames to the
//!   [`SerialTransport`];
//! - a **capture thread** pulls frames from the [`VideoSource`] and forwards
//!   decodable frames to a bounded output channel for the GUI;
//! - a shared `running` flag plus channel disconnection signals shutdown; both
//!   threads observe it within one blocking-read timeout and exit, and
//!   [`Session::shutdown`] joins them.
//!
//! The session is generic over the transport and source traits, so it runs the
//! full pipeline against the simulated devices in `openterface-test-support`
//! with **no hardware**.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::event::{HidUsage, InputEvent, Modifiers, MouseButton};
use crate::pacing::{PacingConfig, PacingScheduler};
use crate::serial::SerialTransport;
use crate::video::{CaptureConfig, Frame, VideoSource};
use crate::Result;

/// How long the worker loops block before re-checking the shutdown flag.
const TICK: Duration = Duration::from_millis(10);

/// Bound on the frame channel; capture drops frames when the consumer lags
/// rather than growing unbounded memory.
const FRAME_CHANNEL_BOUND: usize = 4;

/// A running KVM session. Dropping it (or calling [`Session::shutdown`]) stops
/// and joins the worker threads.
pub struct Session {
    input_tx: Option<Sender<InputEvent>>,
    running: Arc<AtomicBool>,
    writer: Option<JoinHandle<()>>,
    capture: Option<JoinHandle<()>>,
}

impl Session {
    /// Starts a session: spawns the serial-writer and capture threads.
    ///
    /// `frame_tx` receives decoded-able capture frames (bounded; lagging
    /// consumers drop frames). `config` is the capture configuration; the
    /// video source is configured and started here.
    pub fn start<T, V>(
        mut serial: T,
        mut video: V,
        config: CaptureConfig,
        pacing: PacingConfig,
        frame_tx: SyncSender<Frame>,
    ) -> Result<Session>
    where
        T: SerialTransport + 'static,
        V: VideoSource + 'static,
    {
        video.configure(config)?;
        video.start()?;

        let running = Arc::new(AtomicBool::new(true));
        let (input_tx, input_rx) = std::sync::mpsc::channel::<InputEvent>();

        let writer = std::thread::Builder::new()
            .name("openterface-serial".into())
            .spawn(move || writer_loop(&mut serial, &input_rx, pacing))
            .map_err(crate::Error::Io)?;
        let capture = {
            let running = Arc::clone(&running);
            match std::thread::Builder::new()
                .name("openterface-capture".into())
                .spawn(move || capture_loop(&mut video, &frame_tx, &running))
            {
                Ok(h) => h,
                Err(e) => {
                    // The writer is already running; stop it cleanly (drop the
                    // input sender → writer drains and exits) before returning.
                    drop(input_tx);
                    let _ = writer.join();
                    return Err(crate::Error::Io(e));
                }
            }
        };

        Ok(Session {
            input_tx: Some(input_tx),
            running,
            writer: Some(writer),
            capture: Some(capture),
        })
    }

    /// Forwards an input event to the target (non-blocking).
    pub fn send_input(&self, event: InputEvent) {
        if let Some(tx) = &self.input_tx {
            let _ = tx.send(event);
        }
    }

    /// Sends the Ctrl+Alt+Del combination (press then release).
    pub fn send_ctrl_alt_del(&self) {
        let mods = Modifiers::LEFT_CTRL.union(Modifiers::LEFT_ALT);
        self.send_input(InputEvent::Key {
            usage: HidUsage(0x4C), // Delete
            modifiers: mods,
            pressed: true,
        });
        self.send_input(InputEvent::Key {
            usage: HidUsage(0x4C),
            modifiers: Modifiers::NONE,
            pressed: false,
        });
    }

    /// Taps a single key (press then release) with no modifiers.
    pub fn tap_key(&self, usage: HidUsage) {
        self.send_input(InputEvent::Key {
            usage,
            modifiers: Modifiers::NONE,
            pressed: true,
        });
        self.send_input(InputEvent::Key {
            usage,
            modifiers: Modifiers::NONE,
            pressed: false,
        });
    }

    /// Clicks a mouse button (press then release) at the current position.
    pub fn click(&self, button: MouseButton) {
        self.send_input(InputEvent::MouseButton {
            button,
            pressed: true,
        });
        self.send_input(InputEvent::MouseButton {
            button,
            pressed: false,
        });
    }

    /// Stops the session and joins the worker threads. Idempotent.
    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        // Dropping the sender unblocks the writer's recv with a disconnect.
        self.input_tx.take();
        if let Some(h) = self.writer.take() {
            let _ = h.join();
        }
        if let Some(h) = self.capture.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Serial-writer loop: drain input events into the scheduler and write paced
/// CH9329 frames. Shutdown is signalled by dropping the input sender: once the
/// channel is empty **and** disconnected, the loop drains any final ready frames
/// and exits. Gating on the channel (not a flag) guarantees every queued event
/// — crucially every release — is processed before exit.
fn writer_loop<T: SerialTransport>(
    serial: &mut T,
    input_rx: &Receiver<InputEvent>,
    pacing: PacingConfig,
) {
    let mut scheduler = PacingScheduler::new(pacing);
    loop {
        // Wait until the scheduler next has work, or a new event arrives.
        let wait = scheduler
            .time_until_ready(Instant::now())
            .unwrap_or(TICK)
            .min(TICK);
        match input_rx.recv_timeout(wait) {
            Ok(event) => {
                scheduler.submit(event);
                // Drain the rest of the currently-queued batch so coalescing and
                // release-priority apply across the whole available batch (a
                // release sitting behind a move in the channel still jumps ahead).
                while let Ok(ev) = input_rx.try_recv() {
                    scheduler.submit(ev);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Emit everything that is ready now.
        while let Some(cmd) = scheduler.poll(Instant::now()) {
            if serial.write_all(&cmd).is_err() {
                // A serial write error is treated as a fatal session condition.
                return;
            }
        }
    }
    // Final drain so a trailing release (always a priority command, hence ready)
    // is never lost on a clean shutdown.
    while let Some(cmd) = scheduler.poll(Instant::now()) {
        if serial.write_all(&cmd).is_err() {
            return;
        }
    }
}

/// Capture loop: pull frames and forward them (bounded, lossy) until shutdown.
fn capture_loop<V: VideoSource>(video: &mut V, frame_tx: &SyncSender<Frame>, running: &AtomicBool) {
    while running.load(Ordering::SeqCst) {
        match video.next_frame(TICK) {
            Ok(frame) => match frame_tx.try_send(frame) {
                Ok(()) | Err(TrySendError::Full(_)) => {} // drop on a lagging consumer
                Err(TrySendError::Disconnected(_)) => break,
            },
            Err(crate::Error::Timeout) => continue, // no frame this tick; recheck shutdown
            Err(_) => {
                // Recoverable stream errors (e.g. a corrupt frame) are skipped;
                // a disconnect surfaces repeatedly and the consumer can react.
                std::thread::sleep(TICK);
            }
        }
    }
    let _ = video.stop();
    let _ = FRAME_CHANNEL_BOUND;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ch9329;

    /// Returns true if each needle appears in `haystack`, in the given order
    /// (non-overlapping), i.e. needle[i] starts after needle[i-1] ends.
    fn ordered_subslices(haystack: &[u8], needles: &[&[u8]]) -> bool {
        let mut from = 0usize;
        for needle in needles {
            match haystack[from..]
                .windows(needle.len())
                .position(|w| w == *needle)
            {
                Some(p) => from += p + needle.len(),
                None => return false,
            }
        }
        true
    }

    // A minimal in-thread recording serial for unit tests in this module.
    struct VecSerial(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl SerialTransport for VecSerial {
        fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(())
        }
        fn read(&mut self, _b: &mut [u8], _t: Duration) -> Result<usize> {
            Ok(0)
        }
        fn set_baud_rate(&mut self, _b: u32) -> Result<()> {
            Ok(())
        }
    }

    // A trivial video source that yields timeouts (no frames) so capture just
    // spins and observes shutdown.
    struct IdleVideo;
    impl VideoSource for IdleVideo {
        fn supported_formats(&self) -> Result<Vec<crate::video::FormatDesc>> {
            Ok(Vec::new())
        }
        fn configure(&mut self, _c: CaptureConfig) -> Result<()> {
            Ok(())
        }
        fn active_config(&self) -> Option<CaptureConfig> {
            None
        }
        fn start(&mut self) -> Result<()> {
            Ok(())
        }
        fn stop(&mut self) -> Result<()> {
            Ok(())
        }
        fn next_frame(&mut self, _t: Duration) -> Result<Frame> {
            Err(crate::Error::Timeout)
        }
    }

    fn wait_until<F: Fn() -> bool>(pred: F, max: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < max {
            if pred() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        pred()
    }

    #[test]
    fn ctrl_alt_del_reaches_serial_then_shuts_down_cleanly() {
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let serial = VecSerial(std::sync::Arc::clone(&sink));
        let (frame_tx, _frame_rx) = std::sync::mpsc::sync_channel::<Frame>(FRAME_CHANNEL_BOUND);
        let mut session = Session::start(
            serial,
            IdleVideo,
            CaptureConfig::default(),
            PacingConfig::default(),
            frame_tx,
        )
        .unwrap();

        session.send_ctrl_alt_del();

        // The exact press frame (mods Ctrl|Alt = 0x05, key 0x4C) followed by the
        // all-zero release frame must reach the serial line, in order.
        let press = ch9329::keyboard(
            Modifiers::LEFT_CTRL.union(Modifiers::LEFT_ALT),
            &[HidUsage(0x4C)],
        );
        let release = ch9329::keyboard_release();
        let got = wait_until(
            || {
                let buf = sink.lock().unwrap();
                ordered_subslices(&buf, &[&press, &release])
            },
            Duration::from_secs(2),
        );
        assert!(
            got,
            "exact Ctrl+Alt+Del press then release frames did not reach serial"
        );

        // Shutdown must join promptly (no deadlock/hang).
        let t = Instant::now();
        session.shutdown();
        assert!(t.elapsed() < Duration::from_secs(2), "shutdown hung");
    }

    #[test]
    fn drop_shuts_down_without_hanging() {
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (frame_tx, _rx) = std::sync::mpsc::sync_channel::<Frame>(FRAME_CHANNEL_BOUND);
        let t = Instant::now();
        {
            let _session = Session::start(
                VecSerial(sink),
                IdleVideo,
                CaptureConfig::default(),
                PacingConfig::default(),
                frame_tx,
            )
            .unwrap();
            // Dropped here.
        }
        assert!(t.elapsed() < Duration::from_secs(2), "drop-shutdown hung");
    }

    #[test]
    fn queued_releases_are_not_lost_on_shutdown() {
        // Submit a press+release then immediately shut down. The writer must
        // drain the channel (not exit on a flag mid-queue), so the all-zero
        // release frame is still written.
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let (frame_tx, _rx) = std::sync::mpsc::sync_channel::<Frame>(FRAME_CHANNEL_BOUND);
        let mut session = Session::start(
            VecSerial(std::sync::Arc::clone(&sink)),
            IdleVideo,
            CaptureConfig::default(),
            PacingConfig::default(),
            frame_tx,
        )
        .unwrap();
        session.tap_key(HidUsage(0x04));
        session.shutdown(); // drains the channel before joining

        let buf = sink.lock().unwrap();
        let release = ch9329::keyboard_release();
        assert!(
            buf.windows(release.len()).any(|w| w == release.as_slice()),
            "release frame must survive shutdown drain"
        );
    }
}
