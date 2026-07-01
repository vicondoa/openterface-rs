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

fn fps_for_candidate(desc: &FormatDesc, preferred_fps: u32) -> u32 {
    if desc.frame_rates.is_empty() || desc.frame_rates.contains(&preferred_fps) {
        return preferred_fps;
    }
    desc.frame_rates
        .iter()
        .copied()
        .filter(|fps| *fps <= preferred_fps)
        .max()
        .or_else(|| desc.frame_rates.iter().copied().min())
        .unwrap_or(preferred_fps)
}

fn fitted_rect(frame_w: u32, frame_h: u32, viewport_w: u32, viewport_h: u32) -> (u32, u32) {
    if frame_w == 0 || frame_h == 0 || viewport_w == 0 || viewport_h == 0 {
        return (viewport_w, viewport_h);
    }

    let by_width_h = (u64::from(viewport_w) * u64::from(frame_h)) / u64::from(frame_w);
    if by_width_h <= u64::from(viewport_h) {
        (viewport_w, by_width_h.max(1) as u32)
    } else {
        let by_height_w = (u64::from(viewport_h) * u64::from(frame_w)) / u64::from(frame_h);
        (by_height_w.max(1) as u32, viewport_h)
    }
}

fn aspect_close(desc: &FormatDesc, preferred: CaptureConfig) -> bool {
    let lhs = u64::from(desc.width) * u64::from(preferred.height.max(1));
    let rhs = u64::from(desc.height) * u64::from(preferred.width.max(1));
    lhs.abs_diff(rhs)
        <= (u64::from(preferred.width.max(1)) * u64::from(preferred.height.max(1)) / 50)
}

fn fits_with_tolerance(desc: &FormatDesc, fit_w: u32, fit_h: u32) -> bool {
    let max_w = u64::from(fit_w) * 110 / 100;
    let max_h = u64::from(fit_h) * 110 / 100;
    u64::from(desc.width) <= max_w && u64::from(desc.height) <= max_h
}

/// Selects a capture mode for a physical display viewport.
///
/// The selector prefers the largest supported mode that fits, with a small
/// downscale tolerance, inside the image rectangle that would be displayed in
/// `viewport_w` x `viewport_h` physical pixels. If the viewport is smaller than
/// every supported same-aspect mode, it chooses the smallest same-aspect mode.
#[must_use]
pub fn select_capture_config_for_viewport(
    formats: &[FormatDesc],
    viewport_w: u32,
    viewport_h: u32,
    preferred: CaptureConfig,
) -> CaptureConfig {
    if formats.is_empty() || viewport_w == 0 || viewport_h == 0 {
        return preferred;
    }

    let (fit_w, fit_h) = fitted_rect(
        preferred.width.max(1),
        preferred.height.max(1),
        viewport_w,
        viewport_h,
    );

    let mut candidates: Vec<&FormatDesc> = formats
        .iter()
        .filter(|desc| matches!(desc.format, PixelFormat::Mjpeg | PixelFormat::Yuyv))
        .collect();
    if candidates.is_empty() {
        return preferred;
    }
    let aspect_candidates: Vec<&FormatDesc> = candidates
        .iter()
        .copied()
        .filter(|desc| aspect_close(desc, preferred))
        .collect();
    if !aspect_candidates.is_empty() {
        candidates = aspect_candidates;
    }

    let fits = |desc: &&FormatDesc| fits_with_tolerance(desc, fit_w, fit_h);
    let best = candidates
        .iter()
        .copied()
        .filter(fits)
        .max_by_key(|desc| {
            (
                desc.width as u64 * desc.height as u64,
                matches!(desc.format, PixelFormat::Mjpeg),
                desc.frame_rates.contains(&preferred.fps),
            )
        })
        .or_else(|| {
            candidates.sort_by_key(|desc| {
                (
                    desc.width as u64 * desc.height as u64,
                    !matches!(desc.format, PixelFormat::Mjpeg),
                )
            });
            candidates.into_iter().next()
        })
        .expect("non-empty candidates");

    CaptureConfig {
        width: best.width,
        height: best.height,
        fps: fps_for_candidate(best, preferred.fps),
        format: best.format,
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

    fn desc(format: PixelFormat, width: u32, height: u32, rates: &[u32]) -> FormatDesc {
        FormatDesc {
            format,
            width,
            height,
            frame_rates: rates.to_vec(),
        }
    }

    fn openterface_formats() -> Vec<FormatDesc> {
        vec![
            desc(PixelFormat::Mjpeg, 1920, 1080, &[60, 50, 30]),
            desc(PixelFormat::Mjpeg, 1360, 768, &[60, 50, 30]),
            desc(PixelFormat::Mjpeg, 1280, 720, &[60, 50, 30]),
            desc(PixelFormat::Mjpeg, 800, 600, &[60, 30]),
            desc(PixelFormat::Yuyv, 1920, 1080, &[10, 5]),
        ]
    }

    #[test]
    fn adaptive_selects_max_when_scaled_viewport_exceeds_capture() {
        assert_eq!(
            select_capture_config_for_viewport(
                &openterface_formats(),
                2880,
                1620,
                CaptureConfig::default(),
            ),
            CaptureConfig::default()
        );
    }

    #[test]
    fn adaptive_selects_display_sized_mode_on_scaled_small_window() {
        assert_eq!(
            select_capture_config_for_viewport(
                &openterface_formats(),
                1440,
                810,
                CaptureConfig::default(),
            ),
            CaptureConfig {
                width: 1360,
                height: 768,
                fps: 30,
                format: PixelFormat::Mjpeg,
            }
        );
    }

    #[test]
    fn adaptive_uses_smallest_mode_when_viewport_is_tiny() {
        assert_eq!(
            select_capture_config_for_viewport(
                &openterface_formats(),
                640,
                360,
                CaptureConfig::default(),
            ),
            CaptureConfig {
                width: 1280,
                height: 720,
                fps: 30,
                format: PixelFormat::Mjpeg,
            }
        );
    }

    #[test]
    fn adaptive_allows_small_downscale_to_preserve_quality() {
        assert_eq!(
            select_capture_config_for_viewport(
                &openterface_formats(),
                1920,
                1000,
                CaptureConfig::default(),
            ),
            CaptureConfig::default()
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
