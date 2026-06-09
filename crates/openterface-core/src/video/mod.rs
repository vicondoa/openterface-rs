//! Video capture contract and the frame model.
//!
//! The [`VideoSource`] trait abstracts a V4L2 capture device so the pipeline
//! can run against a [`crate::video`]-shaped simulator with no hardware. A
//! [`Frame`] carries *encoded* (MJPEG) or *packed* (YUYV) bytes; decoding to
//! RGBA happens in [`crate::decode`].

use std::time::Duration;

use crate::Result;

#[cfg(feature = "video-backend")]
pub mod backend;

/// Pixel/encoding format of a captured frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// Motion-JPEG (hardware-compressed; lower bus bandwidth, needs decode).
    Mjpeg,
    /// Packed YUYV 4:2:2 (uncompressed).
    Yuyv,
}

/// Color quantization range — required for correct YUYV→RGB conversion.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ColorRange {
    /// Studio/limited range (luma 16..=235) — the usual capture default.
    #[default]
    Limited,
    /// Full range (0..=255).
    Full,
}

/// Color space / conversion matrix — required for correct YUYV→RGB conversion.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ColorSpace {
    /// Unknown / unspecified (decoder picks a sane default by resolution).
    #[default]
    Unknown,
    /// ITU-R BT.601 (standard definition).
    Bt601,
    /// ITU-R BT.709 (high definition).
    Bt709,
}

/// A requested capture configuration.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CaptureConfig {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Frames per second.
    pub fps: u32,
    /// Desired pixel format.
    pub format: PixelFormat,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 30,
            format: PixelFormat::Mjpeg,
        }
    }
}

/// A V4L2 format/resolution candidate without frame-rate policy.
#[cfg(any(test, feature = "video-backend"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CaptureFormatCandidate {
    /// Frame width in pixels.
    pub(crate) width: u32,
    /// Frame height in pixels.
    pub(crate) height: u32,
    /// Desired pixel format.
    pub(crate) format: PixelFormat,
}

#[cfg(any(test, feature = "video-backend"))]
impl CaptureFormatCandidate {
    /// Creates a candidate from its V4L2 format fields.
    pub(crate) const fn new(width: u32, height: u32, format: PixelFormat) -> Self {
        Self {
            width,
            height,
            format,
        }
    }
}

#[cfg(any(test, feature = "video-backend"))]
impl From<CaptureConfig> for CaptureFormatCandidate {
    fn from(config: CaptureConfig) -> Self {
        Self::new(config.width, config.height, config.format)
    }
}

/// Documented V4L2 fallback chain for Openterface capture devices.
#[cfg(any(test, feature = "video-backend"))]
pub(crate) const V4L2_FALLBACK_FORMATS: [CaptureFormatCandidate; 3] = [
    CaptureFormatCandidate::new(1920, 1080, PixelFormat::Mjpeg),
    CaptureFormatCandidate::new(1280, 720, PixelFormat::Mjpeg),
    CaptureFormatCandidate::new(1280, 720, PixelFormat::Yuyv),
];

/// Returns the requested format followed by the documented V4L2 fallbacks.
#[cfg(any(test, feature = "video-backend"))]
#[must_use]
pub(crate) fn v4l2_format_candidates(requested: CaptureConfig) -> Vec<CaptureFormatCandidate> {
    let requested = CaptureFormatCandidate::from(requested);
    let mut candidates = Vec::with_capacity(V4L2_FALLBACK_FORMATS.len() + 1);
    candidates.push(requested);
    candidates.extend(
        V4L2_FALLBACK_FORMATS
            .iter()
            .copied()
            .filter(|candidate| *candidate != requested),
    );
    candidates
}

/// Converts a V4L2 frame interval fraction (seconds per frame) to integer Hz.
#[cfg(any(test, feature = "video-backend"))]
#[must_use]
pub(crate) fn integer_fps_from_interval(numerator: u32, denominator: u32) -> Option<u32> {
    if numerator == 0 || denominator == 0 {
        return None;
    }

    let rounded = (u64::from(denominator) + u64::from(numerator) / 2) / u64::from(numerator);
    u32::try_from(rounded).ok().filter(|fps| *fps > 0)
}

/// A capability descriptor advertised by a [`VideoSource`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FormatDesc {
    /// The format this descriptor applies to.
    pub format: PixelFormat,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Frame rates (Hz) supported at this resolution/format.
    pub frame_rates: Vec<u32>,
}

/// A single captured frame.
///
/// `data` holds the *encoded* (MJPEG) or *packed* (YUYV) payload. For v1 the
/// payload is **owned** (copied out of the V4L2 buffer); a zero-copy/loaned
/// model is a future optimization. For packed formats, [`Frame::bytes_per_line`]
/// is the V4L2 stride and [`Frame::color_range`]/[`Frame::color_space`] drive
/// YUYV→RGB conversion.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    /// The format of [`Frame::data`].
    pub format: PixelFormat,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Row stride in bytes for packed formats (V4L2 `bytesperline`); `0` for
    /// encoded formats (MJPEG) where it is not meaningful.
    pub bytes_per_line: u32,
    /// Color quantization range (packed formats).
    pub color_range: ColorRange,
    /// Color space / matrix (packed formats).
    pub color_space: ColorSpace,
    /// Monotonic capture timestamp.
    pub timestamp: Duration,
    /// Encoded (MJPEG) or packed (YUYV) bytes.
    pub data: Vec<u8>,
}

/// A source of video frames.
///
/// `Send` is required so capture can run on its own thread.
pub trait VideoSource: Send {
    /// Returns the formats/resolutions/frame-rates the device advertises.
    fn supported_formats(&self) -> Result<Vec<FormatDesc>>;

    /// Negotiates the given configuration. May choose the closest supported
    /// match; callers should re-read via [`VideoSource::active_config`].
    fn configure(&mut self, config: CaptureConfig) -> Result<()>;

    /// Returns the currently negotiated configuration, if capture is configured.
    fn active_config(&self) -> Option<CaptureConfig>;

    /// Begins streaming.
    fn start(&mut self) -> Result<()>;

    /// Stops streaming.
    fn stop(&mut self) -> Result<()>;

    /// Blocks for the next frame, up to `timeout`.
    fn next_frame(&mut self, timeout: Duration) -> Result<Frame>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4l2_candidates_try_request_before_documented_fallbacks() {
        let requested = CaptureConfig {
            width: 640,
            height: 480,
            fps: 15,
            format: PixelFormat::Yuyv,
        };

        assert_eq!(
            v4l2_format_candidates(requested),
            vec![
                CaptureFormatCandidate::new(640, 480, PixelFormat::Yuyv),
                CaptureFormatCandidate::new(1920, 1080, PixelFormat::Mjpeg),
                CaptureFormatCandidate::new(1280, 720, PixelFormat::Mjpeg),
                CaptureFormatCandidate::new(1280, 720, PixelFormat::Yuyv),
            ]
        );
    }

    #[test]
    fn v4l2_candidates_do_not_repeat_request_when_it_is_a_fallback() {
        assert_eq!(
            v4l2_format_candidates(CaptureConfig::default()),
            V4L2_FALLBACK_FORMATS.to_vec()
        );
    }

    #[test]
    fn integer_fps_from_interval_rounds_ntsc_rates() {
        assert_eq!(integer_fps_from_interval(1, 30), Some(30));
        assert_eq!(integer_fps_from_interval(1001, 30_000), Some(30));
        assert_eq!(integer_fps_from_interval(0, 30), None);
        assert_eq!(integer_fps_from_interval(1, 0), None);
    }
}
