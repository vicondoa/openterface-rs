//! Frame decoding to RGBA.
//!
//! W2.3 implements MJPEG (via `zune-jpeg`) and YUYV→RGBA here. W0 defines the
//! output type and the function signature the GUI renders from.

use crate::video::Frame;
use crate::{Error, Result};

/// A decoded, tightly-packed RGBA8888 image (`pixels.len() == width*height*4`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RgbaImage {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Row-major RGBA bytes.
    pub pixels: Vec<u8>,
}

/// Decodes a captured [`Frame`] to [`RgbaImage`].
///
/// Implemented in W2.3; the W0 skeleton returns [`Error::Decode`] so callers
/// compile against the final signature.
pub fn decode_frame(_frame: &Frame) -> Result<RgbaImage> {
    Err(Error::Decode(
        "decode not yet implemented (W2.3)".to_string(),
    ))
}
