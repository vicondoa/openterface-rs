//! USB identity constants for the two Openterface endpoints.
//!
//! The Openterface presents two independent USB endpoints that share nothing
//! but the cable: an **MS2109** HDMI capture device (UVC / MJPEG video) and a
//! **CH9329** USB-serial HID bridge (mouse + keyboard). Discovery should prefer
//! matching on the *driver + advertised format* (uvcvideo + MJPG for video),
//! using these constants as a secondary signal — in a VM there can be several
//! `/dev/video*` nodes and only the uvcvideo one is the real capture.

/// USB vendor id of the CH340/CH9329 serial bridge (WCH / `1a86`).
pub const SERIAL_VENDOR_ID: u16 = 0x1A86;

/// Known serial-bridge product ids. Firmware revisions enumerate differently:
/// `7523` (classic CH340) or `fe0c` (newer Openterface firmware).
pub const SERIAL_PRODUCT_IDS: [u16; 2] = [0x7523, 0xFE0C];

/// USB vendor id seen for the MS2109 UVC capture on the validated hardware
/// (`345f`).
pub const VIDEO_VENDOR_ID: u16 = 0x345F;

/// Alternate/upstream MS2109 vendor id (MacroSilicon, `534d`).
pub const VIDEO_VENDOR_ID_ALT: u16 = 0x534D;

/// USB product id of the MS2109 capture (`2109`).
pub const VIDEO_PRODUCT_ID: u16 = 0x2109;

/// Returns `true` if the given USB VID/PID identifies an Openterface serial
/// bridge.
#[must_use]
pub fn is_serial_device(vendor_id: u16, product_id: u16) -> bool {
    vendor_id == SERIAL_VENDOR_ID && SERIAL_PRODUCT_IDS.contains(&product_id)
}

/// Returns `true` if the given USB VID/PID identifies an Openterface video
/// capture endpoint (either the validated or upstream vendor id).
#[must_use]
pub fn is_video_device(vendor_id: u16, product_id: u16) -> bool {
    (vendor_id == VIDEO_VENDOR_ID || vendor_id == VIDEO_VENDOR_ID_ALT)
        && product_id == VIDEO_PRODUCT_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_known_serial_ids() {
        assert!(is_serial_device(0x1A86, 0x7523));
        assert!(is_serial_device(0x1A86, 0xFE0C));
        assert!(!is_serial_device(0x1A86, 0x0001));
        assert!(!is_serial_device(0x0000, 0x7523));
    }

    #[test]
    fn matches_known_video_ids() {
        assert!(is_video_device(0x345F, 0x2109));
        assert!(is_video_device(0x534D, 0x2109));
        assert!(!is_video_device(0x345F, 0x0000));
    }
}
