//! Video capture contract and the frame model.
//!
//! The [`VideoSource`] trait abstracts a V4L2 capture device so the pipeline
//! can run against a [`crate::video`]-shaped simulator with no hardware. A
//! [`Frame`] carries *encoded* (MJPEG) or *packed* (YUYV) bytes; decoding to
//! RGBA happens in [`crate::decode`].

use std::time::Duration;

use crate::Result;

/// Pixel/encoding format of a captured frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// Motion-JPEG (hardware-compressed; lower bus bandwidth, needs decode).
    Mjpeg,
    /// Packed YUYV 4:2:2 (uncompressed).
    Yuyv,
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
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    /// The format of [`Frame::data`].
    pub format: PixelFormat,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
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
