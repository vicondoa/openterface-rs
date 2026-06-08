//! W3.3 end-to-end vertical slice (no hardware).
//!
//! Drives a full [`Session`] against the simulated devices from
//! `openterface-test-support`: input events flow through the pacing scheduler to
//! a recording serial transport (verified as exact CH9329 bytes), and captured
//! frames flow out of the session and decode to RGBA. This proves the runtime
//! architecture (threads + channels + shutdown) before the GUI is built.

use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

use openterface_core::decode::decode_frame;
use openterface_core::event::{AbsPosition, HidUsage, InputEvent, Modifiers, MouseButton};
use openterface_core::pacing::PacingConfig;
use openterface_core::protocol::ch9329;
use openterface_core::session::Session;
use openterface_core::video::{CaptureConfig, Frame};

use openterface_test_support::{SharedSerial, SimulatedVideoSource};

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
fn input_flows_to_serial_and_frames_decode() {
    let (serial, written) = SharedSerial::new();
    let video = SimulatedVideoSource::new(); // healthy MJPEG frames
    let (frame_tx, frame_rx) = sync_channel::<Frame>(4);

    let mut session = Session::start(
        serial,
        video,
        CaptureConfig::default(),
        PacingConfig::default(),
        frame_tx,
    )
    .unwrap();

    // 1) A captured frame flows out of the session and decodes to RGBA.
    let frame = frame_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("a frame should arrive");
    let img = decode_frame(&frame).expect("frame decodes");
    assert!(img.width > 0 && img.height > 0);

    // 2) Input events become the exact CH9329 frames on the serial line.
    session.send_input(InputEvent::MouseMoveAbsolute {
        pos: AbsPosition { x: 100, y: 200 },
    });
    session.click(MouseButton::Left);
    session.send_input(InputEvent::Key {
        usage: HidUsage(0x04), // 'a'
        modifiers: Modifiers::NONE,
        pressed: true,
    });

    // Expected absolute-move bytes for (100,200), no buttons (golden vector).
    let abs = ch9329::mouse_absolute(AbsPosition { x: 100, y: 200 }, Default::default(), 0);
    let key = ch9329::keyboard(Modifiers::NONE, &[HidUsage(0x04)]);

    let landed = wait_until(
        || {
            let buf = written.lock().unwrap();
            contains_subslice(&buf, &abs) && contains_subslice(&buf, &key)
        },
        Duration::from_secs(2),
    );
    assert!(
        landed,
        "expected CH9329 move + key frames on the serial line"
    );

    let t = Instant::now();
    session.shutdown();
    assert!(t.elapsed() < Duration::from_secs(2), "shutdown hung");
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
