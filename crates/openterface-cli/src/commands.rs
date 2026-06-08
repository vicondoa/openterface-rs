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

/// `connect` — start a KVM session. The live display lands in W4.2; this resolves
/// and reports the target configuration (and handles `--dummy`).
pub(crate) fn connect(args: &ConnectArgs, _verbose: bool) -> ExitCode {
    if args.dummy {
        println!("Starting in dummy mode (no device connection).");
        return connect_impl(args);
    }
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
        // Dummy mode needs no hardware; the live window lands in W4.2.
        println!("(dummy) display frontend is implemented in the GUI crate (W4.2).");
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
    use openterface_core::serial::{connect_with_fallback, SerialTransport};
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
    // Software reset/reconfigure. (The hardware RTS-toggle sequence is validated
    // on the work-ssd VM in W6.)
    if let Err(e) = transport.write_all(&openterface_core::protocol::ch9329::software_reset()) {
        eprintln!("Reset command failed: {e}");
        return ExitCode::Failure;
    }
    println!("Factory reset command sent.");
    ExitCode::Success
}

#[cfg(feature = "hardware")]
fn connect_impl(args: &ConnectArgs) -> ExitCode {
    // Device resolution + headless capability check. The live winit/wgpu window
    // and input capture are wired in W4.2 (openterface-gui).
    let scanner = SysfsScanner::new();
    let devices = scanner.scan().unwrap_or_default();
    let dev = devices.first();
    let video = args
        .video
        .clone()
        .or_else(|| dev.and_then(|d| d.video_path.clone()));
    let serial = args
        .serial
        .clone()
        .or_else(|| dev.and_then(|d| d.serial_path.clone()));

    if !args.no_video {
        match &video {
            Some(v) => println!("Video device: {}", v.display()),
            None => {
                eprintln!("No Openterface capture device found. Try: openterface-rs scan");
                return ExitCode::Failure;
            }
        }
    }
    if !args.no_serial {
        match &serial {
            Some(s) => println!("Serial device: {}", s.display()),
            None => eprintln!("No serial control device found (input forwarding disabled)."),
        }
    }
    println!("Connected. (Live display + input capture: openterface-gui, W4.2.)");
    ExitCode::Success
}
