//! Openterface device enumeration contract and a pure-sysfs implementation.
//!
//! Discovery prefers matching on the `uvcvideo` driver, advertised MJPG formats
//! when sysfs exposes them, and USB identity over a bare device-node guess. In a
//! VM several `/dev/video*` nodes exist and only the uvcvideo MS2109 node is the
//! KVM (the virtio-media decoder adapter must be skipped). [`SysfsScanner`]
//! reads `/sys`, so it needs no `libudev` and is fully testable against a
//! fixture `/sys` tree (no hardware).

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
///
/// Parses on bytes (not `&str` indices) so arbitrary sysfs contents with
/// multibyte UTF-8 cannot cause a non-char-boundary slice panic.
#[must_use]
pub fn parse_usb_modalias(modalias: &str) -> Option<(u16, u16)> {
    let bytes = modalias.as_bytes();
    let prefix = b"usb:v";
    if !bytes.starts_with(prefix) {
        return None;
    }
    let rest = &bytes[prefix.len()..];
    if rest.len() < 9 || rest[4] != b'p' {
        return None;
    }
    let vid = hex4(&rest[0..4])?;
    let pid = hex4(&rest[5..9])?;
    Some((vid, pid))
}

/// Parses exactly four ASCII hex bytes into a `u16`.
fn hex4(bytes: &[u8]) -> Option<u16> {
    let s = std::str::from_utf8(bytes).ok()?;
    u16::from_str_radix(s, 16).ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[must_use]
fn video_node_index(name: &str) -> Option<u32> {
    name.strip_prefix("video")?.parse().ok()
}

#[must_use]
fn is_uvcvideo_driver(driver: Option<&str>) -> bool {
    driver.is_some_and(|driver| driver.eq_ignore_ascii_case("uvcvideo"))
}

#[must_use]
fn is_virtio_video(driver: Option<&str>, modaliases: &[String]) -> bool {
    driver.is_some_and(|driver| driver.to_ascii_lowercase().contains("virtio"))
        || modaliases
            .iter()
            .any(|modalias| modalias.to_ascii_lowercase().contains("virtio"))
}

#[must_use]
fn advertises_mjpg_text(formats: &str) -> bool {
    let formats = formats.to_ascii_uppercase();
    formats.contains("MJPG") || formats.contains("MJPEG")
}

#[derive(Debug)]
struct VideoCandidate {
    index: u32,
    path: PathBuf,
    ids: Option<(u16, u16)>,
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

    fn modaliases(&self, class_dir: &Path) -> Vec<String> {
        ["device/modalias", "device/../modalias"]
            .into_iter()
            .filter_map(|rel| read_trimmed(&class_dir.join(rel)))
            .collect()
    }

    /// Reads the USB `(vid, pid)` for a `/sys/class/<class>/<name>` entry by
    /// trying `device/modalias` then `device/../modalias`.
    fn usb_ids(&self, class_dir: &Path) -> Option<(u16, u16)> {
        self.modaliases(class_dir)
            .iter()
            .find_map(|modalias| parse_usb_modalias(modalias))
    }

    fn driver_name(&self, class_dir: &Path) -> Option<String> {
        let path = class_dir.join("device/driver");
        if let Ok(target) = std::fs::read_link(&path) {
            if let Some(driver) = target.file_name().and_then(|name| name.to_str()) {
                return Some(driver.to_string());
            }
        }
        read_trimmed(&path)
    }

    fn advertises_mjpg(&self, class_dir: &Path) -> bool {
        ["formats", "format", "device/formats", "device/format"]
            .into_iter()
            .filter_map(|rel| read_trimmed(&class_dir.join(rel)))
            .next()
            .is_none_or(|formats| advertises_mjpg_text(&formats))
    }

    fn scan_video(&self) -> Vec<(PathBuf, Option<(u16, u16)>)> {
        let base = self.root.join("sys/class/video4linux");
        let mut found: Vec<VideoCandidate> = Vec::new();
        let Ok(entries) = std::fs::read_dir(&base) else {
            return Vec::new();
        };
        let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let Some(index) = video_node_index(name) else {
                continue;
            };

            let modaliases = self.modaliases(&dir);
            let driver = self.driver_name(&dir);
            if is_virtio_video(driver.as_deref(), &modaliases)
                || !is_uvcvideo_driver(driver.as_deref())
            {
                continue;
            }

            let card = read_trimmed(&dir.join("name")).unwrap_or_default();
            let ids = modaliases
                .iter()
                .find_map(|modalias| parse_usb_modalias(modalias));
            let is_openterface =
                card.contains("Openterface") || ids.is_some_and(|(v, p)| is_video_device(v, p));
            if is_openterface && self.advertises_mjpg(&dir) {
                found.push(VideoCandidate {
                    index,
                    path: PathBuf::from(format!("/dev/{name}")),
                    ids,
                });
            }
        }
        found.sort_by_key(|candidate| candidate.index);
        found
            .into_iter()
            .map(|candidate| (candidate.path, candidate.ids))
            .collect()
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

    /// All detected Openterface capture node paths (for `scan` enumeration).
    #[must_use]
    pub fn video_nodes(&self) -> Vec<PathBuf> {
        self.scan_video().into_iter().map(|(p, _)| p).collect()
    }

    /// All detected Openterface serial nodes with their USB (vendor, product)
    /// IDs (for `scan` enumeration).
    #[must_use]
    pub fn serial_nodes(&self) -> Vec<(PathBuf, (u16, u16))> {
        self.scan_serial()
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
        // Must not panic on multibyte UTF-8 in the id positions.
        assert_eq!(parse_usb_modalias("usb:v1A86p75€3"), None);
        assert_eq!(parse_usb_modalias("usb:v€€€€p7523d"), None);
    }

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn fixture_root(tag: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/discovery-fixtures")
            .join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn write_video_node(
        root: &Path,
        name: &str,
        card: &str,
        driver: &str,
        modalias: &str,
        formats: Option<&str>,
    ) {
        let base = root.join("sys/class/video4linux").join(name);
        write(&base.join("name"), card);
        write(&base.join("device/driver"), driver);
        write(&base.join("device/modalias"), modalias);
        if let Some(formats) = formats {
            write(&base.join("formats"), formats);
        }
    }

    fn openterface_fixture_root(tag: &str) -> PathBuf {
        let root = fixture_root(tag);
        // Decoy: a virtio-media node that matches the card name but must be skipped.
        write_video_node(
            &root,
            "video0",
            "Openterface virtio decoy\n",
            "virtio-video\n",
            "virtio:d00000021v00001AF4",
            Some("MJPG\n"),
        );
        // Real Openterface video node (uvcvideo + MJPG capture).
        write_video_node(
            &root,
            "video2",
            "Openterface\n",
            "uvcvideo\n",
            "usb:v345Fp2109d0100dc00",
            Some("MJPG\nYUYV\n"),
        );
        // Sibling metadata node for the same USB device; it does not advertise MJPG.
        write_video_node(
            &root,
            "video3",
            "Openterface metadata\n",
            "uvcvideo\n",
            "usb:v345Fp2109d0100dc00",
            Some("META\n"),
        );
        // Higher capture node for the same device; the lower capture index wins.
        write_video_node(
            &root,
            "video4",
            "Openterface alternate capture\n",
            "uvcvideo\n",
            "usb:v345Fp2109d0100dc00",
            Some("MJPG\n"),
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
    fn finds_openterface_and_skips_video_decoys() {
        let root = openterface_fixture_root("find");
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
    fn prefers_lowest_numeric_video_index() {
        let root = fixture_root("lowest");
        write_video_node(
            &root,
            "video10",
            "Openterface high index\n",
            "uvcvideo\n",
            "usb:v345Fp2109d0100dc00",
            Some("MJPG\n"),
        );
        write_video_node(
            &root,
            "video2",
            "Openterface low index\n",
            "uvcvideo\n",
            "usb:v345Fp2109d0100dc00",
            Some("MJPG\n"),
        );

        let scanner = SysfsScanner::with_root(&root);
        let devices = scanner.scan().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(
            devices[0].video_path.as_deref(),
            Some(Path::new("/dev/video2"))
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_tree_yields_nothing() {
        let root = fixture_root("empty");
        std::fs::create_dir_all(root.join("sys/class/tty")).unwrap();
        std::fs::create_dir_all(root.join("sys/class/video4linux")).unwrap();
        let scanner = SysfsScanner::with_root(&root);
        assert!(scanner.scan().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
