//! Openterface device enumeration contract.
//!
//! Discovery prefers matching on the *driver + advertised format* (uvcvideo +
//! MJPG for video) over a bare VID/PID, because in a VM there can be several
//! `/dev/video*` nodes and only the uvcvideo one is the real capture (the
//! virtio-media decoder adapter must be skipped). The real implementation
//! (udev/sysfs) and a `FixtureScanner` over sample sysfs trees both satisfy
//! this trait — see W2.6.

use std::path::PathBuf;

use crate::Result;

/// A discovered Openterface device (its two endpoints, paired when possible).
#[derive(Clone, PartialEq, Eq, Default, Debug)]
pub struct DeviceInfo {
    /// Path to the CH9329 serial control node (e.g. `/dev/ttyACM0`).
    pub serial_path: Option<PathBuf>,
    /// Path to the MS2109 capture node (e.g. `/dev/video2`).
    pub video_path: Option<PathBuf>,
    /// Serial-bridge USB vendor id, if known.
    pub serial_vendor_id: Option<u16>,
    /// Serial-bridge USB product id, if known.
    pub serial_product_id: Option<u16>,
    /// Human-readable description for `scan`/`status` output.
    pub description: String,
}

impl DeviceInfo {
    /// Returns `true` if both endpoints were resolved.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.serial_path.is_some() && self.video_path.is_some()
    }
}

/// Enumerates Openterface devices present on the system.
pub trait DeviceScanner: Send {
    /// Returns all detected Openterface devices.
    fn scan(&self) -> Result<Vec<DeviceInfo>>;
}
