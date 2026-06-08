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
use crate::pacing::{PacingConfig, PacingScheduler, DEFAULT_COMMAND_GAP};
use crate::serial::SerialTransport;
use crate::video::{CaptureConfig, Frame, VideoSource};
use crate::Result;

/// How long the worker loops block before re-checking the shutdown flag.
const TICK: Duration = Duration::from_millis(10);

/// How long the capture loop waits for a frame before treating the stream as
/// stalled. This must comfortably exceed one frame interval (and the first-frame
/// warm-up after STREAMON) so a normal dequeue is not mistaken for a stall — a
/// too-short value perpetually resets the stream and starves capture. The loop
/// returns at frame cadence (~30 fps) during normal operation, so this only
/// bounds shutdown latency when the device has genuinely stopped delivering.
///
/// 1 s is comfortably above the interval of any KVM frame rate this tool
/// negotiates (the slowest realistic capture is a few fps → a few hundred ms);
/// if much lower frame rates are ever supported, derive this from the negotiated
/// interval instead of assuming the constant still clears it.
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(1000);

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

    /// Starts an **input-only** session: the paced serial writer thread with no
    /// video capture. Used by `connect --no-video` for serial/input-only
    /// forwarding (the GUI still provides input; no frames are shown).
    pub fn start_input_only<T>(mut serial: T, pacing: PacingConfig) -> Result<Session>
    where
        T: SerialTransport + 'static,
    {
        let running = Arc::new(AtomicBool::new(true));
        let (input_tx, input_rx) = std::sync::mpsc::channel::<InputEvent>();
        let writer = std::thread::Builder::new()
            .name("openterface-serial".into())
            .spawn(move || writer_loop(&mut serial, &input_rx, pacing))
            .map_err(crate::Error::Io)?;
        Ok(Session {
            input_tx: Some(input_tx),
            running,
            writer: Some(writer),
            capture: None,
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

    /// Types `text` on the target by submitting key press/release events through
    /// the paced input path (manual text injection; C++ `Serial::sendText`
    /// parity). Unmappable characters are skipped; each character is a press
    /// (with any needed modifier) followed by an all-keys-released report.
    pub fn send_text(&self, text: &str) {
        for ch in text.chars() {
            if let Some((mods, usage)) = crate::protocol::hid::ascii_to_hid(ch) {
                self.send_input(InputEvent::Key {
                    usage,
                    modifiers: mods,
                    pressed: true,
                });
                self.send_input(InputEvent::Key {
                    usage,
                    modifiers: Modifiers::NONE,
                    pressed: false,
                });
            }
        }
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

    /// Releases all held keys, modifiers, and mouse buttons on the target.
    /// Call this on window focus loss / pointer leave to avoid stuck input.
    pub fn release_all(&self) {
        self.send_input(InputEvent::ReleaseAll);
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
    // Physical inter-command spacing (CH9329 buffer safety; C++ sendDataRaw).
    let mut last_write: Option<Instant> = None;
    loop {
        let now = Instant::now();
        // Time until the scheduler next has work, lower-bounded by the physical
        // command gap so two writes are never < gap apart. Waiting *here* (before
        // popping a command) means a key/button release arriving during the gap
        // is submitted and re-prioritized before the next poll — so it still
        // jumps ahead of a pending paced move.
        let wait = match scheduler.time_until_ready(now) {
            None => TICK,
            Some(ready_in) => ready_in.max(gap_remaining(last_write, now)).min(TICK),
        };
        match input_rx.recv_timeout(wait) {
            Ok(event) => {
                scheduler.submit(event);
                // Drain the rest of the currently-queued batch so coalescing and
                // release-priority apply across the whole available batch.
                while let Ok(ev) = input_rx.try_recv() {
                    scheduler.submit(ev);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        // Emit at most ONE command, and only once the gap has elapsed, then loop
        // so the next iteration re-drains the channel before the next poll.
        let now = Instant::now();
        if gap_remaining(last_write, now).is_zero() {
            if let Some(cmd) = scheduler.poll(now) {
                if serial.write_all(&cmd).is_err() {
                    return; // a serial write error is a fatal session condition
                }
                last_write = Some(Instant::now());
            }
        }
    }
    // Final drain on shutdown: flush every ready (priority) command — crucially
    // every release — honoring the gap, so trailing input is never lost.
    loop {
        if let Some(last) = last_write {
            let elapsed = last.elapsed();
            if elapsed < DEFAULT_COMMAND_GAP {
                std::thread::sleep(DEFAULT_COMMAND_GAP - elapsed);
            }
        }
        match scheduler.poll(Instant::now()) {
            Some(cmd) => {
                if serial.write_all(&cmd).is_err() {
                    return;
                }
                last_write = Some(Instant::now());
            }
            None => break,
        }
    }
}

/// Time remaining before another CH9329 write is allowed (ZERO if ready now).
fn gap_remaining(last_write: Option<Instant>, now: Instant) -> Duration {
    match last_write {
        None => Duration::ZERO,
        Some(last) => DEFAULT_COMMAND_GAP.saturating_sub(now.saturating_duration_since(last)),
    }
}

/// Capture loop: pull frames and forward them (bounded, lossy) until shutdown.
fn capture_loop<V: VideoSource>(video: &mut V, frame_tx: &SyncSender<Frame>, running: &AtomicBool) {
    let mut captured: u64 = 0;
    let mut errors: u64 = 0;
    let mut timeouts: u64 = 0;
    while running.load(Ordering::SeqCst) {
        match video.next_frame(CAPTURE_TIMEOUT) {
            Ok(frame) => {
                captured += 1;
                if captured == 1 || captured.is_multiple_of(120) {
                    tracing::debug!(captured, bytes = frame.data.len(), "captured frame");
                }
                match frame_tx.try_send(frame) {
                    Ok(()) | Err(TrySendError::Full(_)) => {} // drop on a lagging consumer
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
            Err(crate::Error::Timeout) => {
                // A genuine stall: the device delivered no frame within
                // CAPTURE_TIMEOUT and the backend rebuilt the stream. This should
                // not happen during normal capture (frames arrive at ~30 fps), so
                // a climbing count here is the signature of a starved/black feed —
                // exactly the regression this timeout was widened to prevent.
                timeouts += 1;
                if timeouts == 1 || timeouts.is_multiple_of(10) {
                    tracing::debug!(
                        timeouts,
                        "capture stalled (stream rebuilt, awaiting frames)"
                    );
                }
                continue;
            }
            Err(e) => {
                // Recoverable stream errors (e.g. a corrupt frame) are skipped;
                // a disconnect surfaces repeatedly and the consumer can react.
                errors += 1;
                if errors == 1 || errors.is_multiple_of(120) {
                    tracing::warn!(errors, error = %e, "capture error (skipping)");
                }
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

    /// Models a real capture device whose first frame only arrives `warmup`
    /// after `STREAMON`. The v4l backend rebuilds the stream on every timed-out
    /// dequeue, discarding that warm-up, so a `next_frame` timeout shorter than
    /// `warmup` never yields a frame — it just thrashes the stream. Encoding that
    /// pins the contract that `capture_loop` must wait long enough — a guard
    /// against the capture-starvation bug where a 10 ms (`TICK`) timeout
    /// perpetually reset the stream and showed a black window at 1080p.
    ///
    /// `resets` counts how many times a too-short timeout discarded warm-up
    /// progress, so a test can assert the loop obtained a frame *without*
    /// starving the stream, not merely that some frame eventually arrived.
    struct WarmupVideo {
        warmup: Duration,
        resets: Arc<std::sync::atomic::AtomicU64>,
    }
    impl VideoSource for WarmupVideo {
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
        fn next_frame(&mut self, timeout: Duration) -> Result<Frame> {
            if timeout < self.warmup {
                // A real DQBUF blocks for the timeout; the backend then rebuilds
                // the stream and discards warm-up progress, so no frame arrives
                // and the next call starts over from zero.
                std::thread::sleep(timeout);
                self.resets.fetch_add(1, Ordering::SeqCst);
                return Err(crate::Error::Timeout);
            }
            // A caller patient enough to wait out the warm-up gets the frame.
            std::thread::sleep(self.warmup);
            Ok(Frame {
                format: crate::video::PixelFormat::Mjpeg,
                width: 1920,
                height: 1080,
                bytes_per_line: 0,
                color_range: crate::video::ColorRange::Limited,
                color_space: crate::video::ColorSpace::Bt709,
                timestamp: Duration::ZERO,
                data: vec![0xFF, 0xD8, 0xFF, 0xD9],
            })
        }
    }

    #[test]
    fn capture_loop_survives_first_frame_warmup() {
        // Regression: the capture loop must pass a timeout that comfortably
        // exceeds a device's first-frame warm-up. A too-short timeout makes the
        // v4l backend rebuild the stream on every dequeue and starve capture
        // (the 1080p black-window bug). 100 ms is above `TICK` yet below
        // `CAPTURE_TIMEOUT`, so this fails fast if the loop reverts to `TICK`.
        let running = Arc::new(AtomicBool::new(true));
        let resets = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (tx, rx) = std::sync::mpsc::sync_channel::<Frame>(FRAME_CHANNEL_BOUND);
        let r2 = Arc::clone(&running);
        let resets_in = Arc::clone(&resets);
        let handle = std::thread::spawn(move || {
            let mut video = WarmupVideo {
                warmup: Duration::from_millis(100),
                resets: resets_in,
            };
            capture_loop(&mut video, &tx, &r2);
        });
        let got = rx.recv_timeout(Duration::from_secs(2));
        running.store(false, Ordering::SeqCst);
        let _ = handle.join();
        assert!(
            got.is_ok(),
            "capture_loop starved on a 100 ms first-frame warm-up: it must pass \
             a timeout longer than one frame interval, not TICK ({:?})",
            got.err()
        );
        // The frame must arrive without any starving stream rebuild — a single
        // patient dequeue, not luck after thrashing the stream.
        assert_eq!(
            resets.load(Ordering::SeqCst),
            0,
            "capture_loop rebuilt/starved the stream before getting a frame"
        );
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
    fn input_only_session_forwards_without_video() {
        // `--no-video`: a serial-only session forwards input with no capture.
        let sink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let serial = VecSerial(std::sync::Arc::clone(&sink));
        let mut session = Session::start_input_only(serial, PacingConfig::default()).unwrap();
        session.send_text("a");
        let press = ch9329::keyboard(Modifiers::NONE, &[HidUsage(0x04)]);
        assert!(
            wait_until(
                || ordered_subslices(&sink.lock().unwrap(), &[&press]),
                Duration::from_secs(1)
            ),
            "the 'a' key report should reach the serial sink"
        );
        session.shutdown();
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
