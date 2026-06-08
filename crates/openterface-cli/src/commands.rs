//! Command handlers for the `openterface-rs` CLI.
//!
//! `scan` and `status` use the pure-sysfs [`SysfsScanner`] and work without the
//! `hardware` feature. `reset` and `connect` need real device I/O and are gated
//! behind `hardware`; without it they print a clear message and fail.

use std::path::Path;

use openterface_core::discovery::{DeviceInfo, DeviceScanner, SysfsScanner};

use crate::cli::{ConnectArgs, ExitCode, ResetArgs};

/// `scan` — enumerate Openterface devices.
pub(crate) fn scan(verbose: bool) -> ExitCode {
    println!("Scanning for Openterface USB KVM devices...");
    let devices = match SysfsScanner::new().scan() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("scan failed: {e}");
            return ExitCode::Failure;
        }
    };
    if devices.is_empty() {
        println!("No Openterface devices detected.");
        println!("Ensure the device is plugged in and recognized by the system.");
        println!("Or use: openterface-rs connect --dummy");
        return ExitCode::Success;
    }
    for dev in &devices {
        print_device(dev, verbose);
    }
    if let Some(dev) = devices.iter().find(|d| d.is_complete()) {
        println!(
            "\nRecommended: openterface-rs connect --video={} --serial={}",
            display_path(dev.video_path.as_deref()),
            display_path(dev.serial_path.as_deref()),
        );
    }
    ExitCode::Success
}

/// `status` — show detected device status (presence-based, no device open).
pub(crate) fn status(_verbose: bool) -> ExitCode {
    let devices = match SysfsScanner::new().scan() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("status failed: {e}");
            return ExitCode::Failure;
        }
    };
    println!("=== Openterface KVM Status ===");
    let dev = devices.first();
    let video = dev.and_then(|d| d.video_path.as_deref());
    let serial = dev.and_then(|d| d.serial_path.as_deref());
    println!(
        "Video:  {}",
        match video {
            Some(p) => format!("DETECTED ({})", p.display()),
            None => "NOT DETECTED".to_string(),
        }
    );
    println!(
        "Serial: {}",
        match serial {
            Some(p) => format!("DETECTED ({})", p.display()),
            None => "NOT DETECTED".to_string(),
        }
    );
    ExitCode::Success
}

fn print_device(dev: &DeviceInfo, verbose: bool) {
    println!("Found: {}", dev.description);
    if let Some(v) = &dev.video_path {
        println!("  video:  {}", v.display());
    }
    if let Some(s) = &dev.serial_path {
        let ids = match (dev.serial_vendor_id, dev.serial_product_id) {
            (Some(v), Some(p)) => format!(" (VID:PID {v:04x}:{p:04x})"),
            _ => String::new(),
        };
        println!("  serial: {}{}", s.display(), ids);
    }
    if verbose && !dev.is_complete() {
        println!("  (incomplete: only one endpoint detected)");
    }
}

fn display_path(p: Option<&Path>) -> String {
    p.map(|p| p.display().to_string())
        .unwrap_or_else(|| "<none>".to_string())
}

/// `reset` — factory-reset the CH9329. Requires `--serial`; needs `hardware`.
pub(crate) fn reset(args: &ResetArgs, _verbose: bool) -> ExitCode {
    let Some(serial) = args.serial.as_deref() else {
        eprintln!("Error: --serial is required for reset");
        eprintln!("Usage: openterface-rs reset --serial /dev/ttyACM0");
        return ExitCode::Failure;
    };
    reset_impl(serial)
}

/// `connect` — start a KVM session and open the display.
pub(crate) fn connect(args: &ConnectArgs, _verbose: bool) -> ExitCode {
    connect_impl(args)
}

#[cfg(not(feature = "hardware"))]
fn reset_impl(_serial: &Path) -> ExitCode {
    eprintln!(
        "openterface-rs was built without hardware support; `reset` is unavailable.\n\
         Rebuild with `--features hardware` (the release/Nix build enables it)."
    );
    ExitCode::Failure
}

#[cfg(not(feature = "hardware"))]
fn connect_impl(args: &ConnectArgs) -> ExitCode {
    if args.dummy {
        // Dummy mode needs no hardware; the live window lands with `hardware`.
        println!("Starting in dummy mode (no device connection).");
        println!("(dummy) the display window requires `--features hardware`.");
        return ExitCode::Success;
    }
    eprintln!(
        "openterface-rs was built without hardware support; `connect` is unavailable.\n\
         Rebuild with `--features hardware` (the release/Nix build enables it)."
    );
    ExitCode::Failure
}

#[cfg(feature = "hardware")]
fn reset_impl(serial: &Path) -> ExitCode {
    use openterface_core::serial::backend::SerialPortTransport;
    use openterface_core::serial::{
        connect_with_fallback, factory_reset, FACTORY_RESET_RTS_HOLD, FACTORY_RESET_SETTLE,
    };
    use std::time::Duration;

    let path = serial.to_string_lossy();
    println!("=== CH9329 Factory Reset ===");
    println!("Connecting to serial port: {path}");
    let mut transport = match SerialPortTransport::open(&path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to open serial port {path}: {e}");
            eprintln!(
                "Check that the device is plugged in and accessible. Try: openterface-rs scan"
            );
            return ExitCode::Failure;
        }
    };
    if let Err(e) = connect_with_fallback(&mut transport, Duration::from_millis(500)) {
        eprintln!("Failed to negotiate with CH9329: {e}");
        return ExitCode::Failure;
    }
    // Hardware factory reset: pulse RTS high ~4s, release, settle, then software
    // reset (C++ parity). This blocks for ~4.5s.
    println!("Pulsing RTS for factory reset (~4s)...");
    if let Err(e) = factory_reset(
        &mut transport,
        FACTORY_RESET_RTS_HOLD,
        FACTORY_RESET_SETTLE,
        std::thread::sleep,
    ) {
        eprintln!("Factory reset failed: {e}");
        return ExitCode::Failure;
    }
    println!("Factory reset complete.");
    ExitCode::Success
}

#[cfg(feature = "hardware")]
fn connect_impl(args: &ConnectArgs) -> ExitCode {
    use openterface_core::pacing::PacingConfig;
    use openterface_core::serial::backend::SerialPortTransport;
    use openterface_core::session::Session;
    use openterface_core::video::{backend::V4l2Source, CaptureConfig, VideoSource};
    use openterface_gui::{run, RunConfig};

    let fullscreen = std::env::var("OPENTERFACE_FULLSCREEN")
        .ok()
        .map(|v| !matches!(v.trim(), "" | "0" | "false" | "no" | "off"))
        .unwrap_or(false);

    // Dummy mode: open the window with a test pattern, no devices.
    if args.dummy {
        println!("Starting Openterface KVM in dummy mode...");
        println!("No device connections will be made.");
        println!("- Running in dummy mode (no device connections)");
        println!("- Video will show test pattern");
        println!("- Input will be simulated (not forwarded)");
        if args.debug {
            println!("Debug mode enabled - input events will be logged");
        }
        let cfg = RunConfig {
            session: None,
            frames: None,
            fullscreen,
            title: "Openterface KVM (dummy)".to_string(),
            debug: args.debug,
        };
        return match run(cfg) {
            Ok(()) => ExitCode::Success,
            Err(e) => {
                eprintln!("display error: {e}");
                ExitCode::Failure
            }
        };
    }

    // Resolve devices (explicit flags or auto-detect).
    let devices = SysfsScanner::new().scan().unwrap_or_default();
    let dev = devices.first();
    let video_path = args
        .video
        .clone()
        .or_else(|| dev.and_then(|d| d.video_path.clone()));
    let serial_path = args
        .serial
        .clone()
        .or_else(|| dev.and_then(|d| d.serial_path.clone()));

    if args.no_video {
        eprintln!("--no-video: a display session requires video; nothing to show.");
        return ExitCode::Failure;
    }
    let Some(video_path) = video_path else {
        eprintln!("No Openterface capture device found. Try: openterface-rs scan");
        return ExitCode::Failure;
    };

    let mut video = match V4l2Source::open(&video_path.to_string_lossy()) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to open video device {}: {e}", video_path.display());
            return ExitCode::Failure;
        }
    };
    if let Err(e) = video.configure(CaptureConfig::default()) {
        eprintln!("Failed to configure capture: {e}");
        return ExitCode::Failure;
    }

    // Serial is optional (input forwarding); without it we still show video.
    let serial: Box<dyn openterface_core::serial::SerialTransport> = if args.no_serial {
        Box::new(NullSerial)
    } else if let Some(sp) = &serial_path {
        match SerialPortTransport::open(&sp.to_string_lossy()) {
            Ok(mut t) => {
                // Negotiate baud (115200 → 9600 fallback) before forwarding, so
                // devices that only respond at the fallback rate still work.
                use openterface_core::serial::connect_with_fallback;
                if let Err(e) = connect_with_fallback(&mut t, std::time::Duration::from_millis(500))
                {
                    eprintln!("CH9329 negotiation failed ({e}); input forwarding disabled.");
                    Box::new(NullSerial)
                } else {
                    Box::new(t)
                }
            }
            Err(e) => {
                eprintln!("Serial open failed ({e}); continuing without input forwarding.");
                Box::new(NullSerial)
            }
        }
    } else {
        eprintln!("No serial control device found; input forwarding disabled.");
        Box::new(NullSerial)
    };

    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel(4);
    let session = match Session::start(
        serial,
        video,
        CaptureConfig::default(),
        PacingConfig::from_env(),
        frame_tx,
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to start session: {e}");
            return ExitCode::Failure;
        }
    };

    let cfg = RunConfig {
        session: Some(session),
        frames: Some(frame_rx),
        fullscreen,
        title: "Openterface KVM".to_string(),
        debug: args.debug,
    };
    if args.debug {
        println!("Debug mode enabled - input events will be logged");
    }
    match run(cfg) {
        Ok(()) => ExitCode::Success,
        Err(e) => {
            eprintln!("display error: {e}");
            ExitCode::Failure
        }
    }
}

/// A no-op serial transport used when input forwarding is disabled/unavailable.
#[cfg(feature = "hardware")]
struct NullSerial;

#[cfg(feature = "hardware")]
impl openterface_core::serial::SerialTransport for NullSerial {
    fn write_all(&mut self, _bytes: &[u8]) -> openterface_core::Result<()> {
        Ok(())
    }
    fn read(
        &mut self,
        _buf: &mut [u8],
        _timeout: std::time::Duration,
    ) -> openterface_core::Result<usize> {
        Ok(0)
    }
    fn set_baud_rate(&mut self, _baud: u32) -> openterface_core::Result<()> {
        Ok(())
    }
}
