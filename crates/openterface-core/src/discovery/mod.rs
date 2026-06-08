//! Openterface device enumeration contract and a pure-sysfs implementation.
//!
//! Discovery prefers matching on the *driver/card name + USB identity* over a
//! bare device-node guess: in a VM several `/dev/video*` nodes exist and only
//! the `uvcvideo` MS2109 node is the KVM (the virtio-media decoder adapter must
//! be skipped). [`SysfsScanner`] reads `/sys` and matches by card name and the
//! USB VID/PID parsed from `modalias`, so it needs no `libudev` and is fully
//! testable against a fixture `/sys` tree (no hardware).

use std::path::{Path, PathBuf};

use crate::device::{is_serial_device, is_video_device};
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

/// Parses `(vendor, product)` from a USB `modalias` string such as
/// `usb:v1A86p7523d...`. Returns `None` if the pattern is absent.
#[must_use]
pub fn parse_usb_modalias(modalias: &str) -> Option<(u16, u16)> {
    let rest = modalias.strip_prefix("usb:v")?;
    if rest.len() < 9 || rest.as_bytes()[4] != b'p' {
        return None;
    }
    let vid = u16::from_str_radix(&rest[0..4], 16).ok()?;
    let pid = u16::from_str_radix(&rest[5..9], 16).ok()?;
    Some((vid, pid))
}

/// A `DeviceScanner` backed by the Linux `/sys` filesystem.
///
/// Defaults to the real root `/`; tests point `root` at a fixture tree.
pub struct SysfsScanner {
    root: PathBuf,
}

impl Default for SysfsScanner {
    fn default() -> Self {
        Self::with_root("/")
    }
}

impl SysfsScanner {
    /// Creates a scanner rooted at `/` (the real system).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a scanner rooted at `root` (a fixture `/sys` parent for tests).
    pub fn with_root(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Reads the USB `(vid, pid)` for a `/sys/class/<class>/<name>` entry by
    /// trying `device/modalias` then `device/../modalias`.
    fn usb_ids(&self, class_dir: &Path) -> Option<(u16, u16)> {
        for rel in ["device/modalias", "device/../modalias"] {
            let path = class_dir.join(rel);
            if let Ok(s) = std::fs::read_to_string(&path) {
                if let Some(ids) = parse_usb_modalias(s.trim()) {
                    return Some(ids);
                }
            }
        }
        None
    }

    fn scan_video(&self) -> Vec<(PathBuf, Option<(u16, u16)>)> {
        let base = self.root.join("sys/class/video4linux");
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(&base) else {
            return found;
        };
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with("video") {
                continue;
            }
            let card = std::fs::read_to_string(dir.join("name"))
                .unwrap_or_default()
                .trim()
                .to_string();
            let ids = self.usb_ids(&dir);
            let is_openterface =
                card.contains("Openterface") || ids.is_some_and(|(v, p)| is_video_device(v, p));
            if is_openterface {
                found.push((PathBuf::from(format!("/dev/{name}")), ids));
            }
        }
        found
    }

    fn scan_serial(&self) -> Vec<(PathBuf, (u16, u16))> {
        let base = self.root.join("sys/class/tty");
        let mut found = Vec::new();
        let Ok(entries) = std::fs::read_dir(&base) else {
            return found;
        };
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Prefer ACM (current firmware) ordering before USB.
            if !(name.starts_with("ttyACM") || name.starts_with("ttyUSB")) {
                continue;
            }
            if let Some((v, p)) = self.usb_ids(&dir) {
                if is_serial_device(v, p) {
                    found.push((PathBuf::from(format!("/dev/{name}")), (v, p)));
                }
            }
        }
        // ttyACM* sorts before ttyUSB* lexically, matching the kvm-debug
        // preference for ACM nodes.
        found
    }
}

impl DeviceScanner for SysfsScanner {
    fn scan(&self) -> Result<Vec<DeviceInfo>> {
        let videos = self.scan_video();
        let serials = self.scan_serial();
        if videos.is_empty() && serials.is_empty() {
            return Ok(Vec::new());
        }
        let (serial_path, serial_ids) = match serials.into_iter().next() {
            Some((path, ids)) => (Some(path), Some(ids)),
            None => (None, None),
        };
        let video_path = videos.into_iter().next().map(|(path, _)| path);
        let description = match (&video_path, &serial_path) {
            (Some(_), Some(_)) => "Openterface Mini-KVM (video + serial)".to_string(),
            (Some(_), None) => "Openterface video only (no serial control found)".to_string(),
            (None, Some(_)) => "Openterface serial only (no capture found)".to_string(),
            (None, None) => unreachable!("guarded above"),
        };
        Ok(vec![DeviceInfo {
            serial_path,
            video_path,
            serial_vendor_id: serial_ids.map(|(v, _)| v),
            serial_product_id: serial_ids.map(|(_, p)| p),
            description,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modalias() {
        assert_eq!(
            parse_usb_modalias("usb:v1A86p7523d0263dc02"),
            Some((0x1A86, 0x7523))
        );
        assert_eq!(
            parse_usb_modalias("usb:v345Fp2109d0100"),
            Some((0x345F, 0x2109))
        );
        assert_eq!(parse_usb_modalias("pci:v00008086d"), None);
        assert_eq!(parse_usb_modalias("usb:v1A86"), None);
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// Builds a fixture /sys tree under a unique temp dir and returns its root.
    fn fixture_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("otrs-disco-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Real Openterface video node (uvcvideo + MJPG capture).
        write(
            &root.join("sys/class/video4linux/video2/name"),
            "Openterface\n",
        );
        write(
            &root.join("sys/class/video4linux/video2/device/modalias"),
            "usb:v345Fp2109d0100dc00",
        );
        // Decoy: a virtio-media decoder node that must be skipped.
        write(
            &root.join("sys/class/video4linux/video0/name"),
            "virtio-media\n",
        );
        write(
            &root.join("sys/class/video4linux/video0/device/modalias"),
            "virtio:d00000021v00001AF4",
        );
        // CH9329 serial node.
        write(
            &root.join("sys/class/tty/ttyACM0/device/modalias"),
            "usb:v1A86p7523d0263dc02",
        );
        // Decoy serial: some other USB serial.
        write(
            &root.join("sys/class/tty/ttyUSB0/device/modalias"),
            "usb:v0403p6001d0600",
        );
        root
    }

    #[test]
    fn finds_openterface_and_skips_decoys() {
        let root = fixture_root("find");
        let scanner = SysfsScanner::with_root(&root);
        let devices = scanner.scan().unwrap();
        assert_eq!(devices.len(), 1);
        let d = &devices[0];
        assert_eq!(d.video_path.as_deref(), Some(Path::new("/dev/video2")));
        assert_eq!(d.serial_path.as_deref(), Some(Path::new("/dev/ttyACM0")));
        assert_eq!(d.serial_vendor_id, Some(0x1A86));
        assert_eq!(d.serial_product_id, Some(0x7523));
        assert!(d.is_complete());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_tree_yields_nothing() {
        let root = std::env::temp_dir().join(format!("otrs-disco-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sys/class/tty")).unwrap();
        let scanner = SysfsScanner::with_root(&root);
        assert!(scanner.scan().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
