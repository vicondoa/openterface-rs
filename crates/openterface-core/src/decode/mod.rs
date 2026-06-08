//! Frame decoding to RGBA.
//!
//! Decodes a captured [`Frame`] (MJPEG via `zune-jpeg`, or packed YUYV) into a
//! tightly-packed [`RgbaImage`] for upload to the GPU. YUYV->RGB honors the
//! frame's [`crate::video::ColorRange`] / [`crate::video::ColorSpace`].

use crate::video::{ColorRange, ColorSpace, Frame, PixelFormat};
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

impl RgbaImage {
    /// Returns the number of bytes a fully-decoded image of this size occupies.
    #[must_use]
    pub fn byte_len(width: u32, height: u32) -> usize {
        width as usize * height as usize * 4
    }
}

/// Decodes a captured [`Frame`] to [`RgbaImage`].
pub fn decode_frame(frame: &Frame) -> Result<RgbaImage> {
    match frame.format {
        PixelFormat::Mjpeg => decode_mjpeg(&frame.data),
        PixelFormat::Yuyv => decode_yuyv(frame),
    }
}

/// Decodes an MJPEG/JPEG payload to RGBA using `zune-jpeg`.
pub fn decode_mjpeg(data: &[u8]) -> Result<RgbaImage> {
    use zune_jpeg::zune_core::colorspace::ColorSpace as JpegColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    use zune_jpeg::JpegDecoder;

    let options = DecoderOptions::default().jpeg_set_out_colorspace(JpegColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(data, options);
    let pixels = decoder
        .decode()
        .map_err(|e| Error::Decode(format!("mjpeg: {e}")))?;
    let (w, h) = decoder
        .dimensions()
        .ok_or_else(|| Error::Decode("mjpeg: no dimensions after decode".to_string()))?;
    let (width, height) = (w as u32, h as u32);
    if pixels.len() != RgbaImage::byte_len(width, height) {
        return Err(Error::Decode(format!(
            "mjpeg: decoded {} bytes, expected {} for {width}x{height}",
            pixels.len(),
            RgbaImage::byte_len(width, height)
        )));
    }
    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// Converts packed YUYV 4:2:2 to RGBA, honoring stride/range/colorspace.
fn decode_yuyv(frame: &Frame) -> Result<RgbaImage> {
    let width = frame.width;
    let height = frame.height;
    if width == 0 || height == 0 || !width.is_multiple_of(2) {
        return Err(Error::Decode(format!(
            "yuyv: invalid dimensions {width}x{height} (width must be non-zero and even)"
        )));
    }
    // Stride: explicit bytes_per_line, else the packed minimum (2 bytes/pixel).
    let stride = if frame.bytes_per_line == 0 {
        width as usize * 2
    } else {
        frame.bytes_per_line as usize
    };
    let needed = stride * height as usize;
    if frame.data.len() < needed {
        return Err(Error::Decode(format!(
            "yuyv: {} bytes < {needed} needed ({stride} stride x {height} rows)",
            frame.data.len()
        )));
    }

    let coeffs = YuvCoeffs::for_space(frame.color_space);
    let full_range = matches!(frame.color_range, ColorRange::Full);
    let mut pixels = vec![0u8; RgbaImage::byte_len(width, height)];

    for row in 0..height as usize {
        let line = &frame.data[row * stride..row * stride + width as usize * 2];
        let out_row = &mut pixels[row * width as usize * 4..(row + 1) * width as usize * 4];
        // Each 4 bytes (Y0 U Y1 V) yields two RGBA pixels.
        for (i, chunk) in line.chunks_exact(4).enumerate() {
            let y0 = chunk[0];
            let u = chunk[1];
            let y1 = chunk[2];
            let v = chunk[3];
            let p = i * 8;
            yuv_to_rgba(y0, u, v, full_range, coeffs, &mut out_row[p..p + 4]);
            yuv_to_rgba(y1, u, v, full_range, coeffs, &mut out_row[p + 4..p + 8]);
        }
    }

    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// BT.601 / BT.709 chroma coefficients (fixed-point 16.16).
#[derive(Clone, Copy)]
struct YuvCoeffs {
    r_v: i32,
    g_u: i32,
    g_v: i32,
    b_u: i32,
}

impl YuvCoeffs {
    fn for_space(space: ColorSpace) -> YuvCoeffs {
        match space {
            ColorSpace::Bt709 => YuvCoeffs {
                r_v: 103_206,
                g_u: -12_276,
                g_v: -30_679,
                b_u: 121_609,
            },
            // BT.601 (and Unknown defaults to 601, the SD/webcam norm).
            ColorSpace::Bt601 | ColorSpace::Unknown => YuvCoeffs {
                r_v: 91_881,
                g_u: -22_554,
                g_v: -46_802,
                b_u: 116_129,
            },
        }
    }
}

#[inline]
fn yuv_to_rgba(y: u8, u: u8, v: u8, full_range: bool, c: YuvCoeffs, out: &mut [u8]) {
    // Luma: limited range maps 16..235 to 0..255; full range is 1:1.
    let yf: i32 = if full_range {
        i32::from(y) << 16
    } else {
        (i32::from(y) - 16) * 76_309 // 255/219 in 16.16
    };
    let uf = i32::from(u) - 128;
    let vf = i32::from(v) - 128;

    // Add half (1<<15) before the >>16 shift so we round-to-nearest instead of
    // truncating (otherwise limited-range white Y=235 lands at 254, not 255).
    let half = 1 << 15;
    let r = (yf + c.r_v * vf + half) >> 16;
    let g = (yf + c.g_u * uf + c.g_v * vf + half) >> 16;
    let b = (yf + c.b_u * uf + half) >> 16;

    out[0] = r.clamp(0, 255) as u8;
    out[1] = g.clamp(0, 255) as u8;
    out[2] = b.clamp(0, 255) as u8;
    out[3] = 255;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn yuyv_frame(width: u32, height: u32, data: Vec<u8>) -> Frame {
        Frame {
            format: PixelFormat::Yuyv,
            width,
            height,
            bytes_per_line: 0,
            color_range: ColorRange::Limited,
            color_space: ColorSpace::Bt601,
            timestamp: Duration::ZERO,
            data,
        }
    }

    #[test]
    fn yuyv_black_is_black() {
        // Limited-range black: Y=16, U=V=128.
        let frame = yuyv_frame(2, 1, vec![16, 128, 16, 128]);
        let img = decode_frame(&frame).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        assert_eq!(&img.pixels[0..4], &[0, 0, 0, 255]);
        assert_eq!(&img.pixels[4..8], &[0, 0, 0, 255]);
    }

    #[test]
    fn yuyv_white_is_white() {
        // Limited-range white: Y=235, U=V=128.
        let frame = yuyv_frame(2, 1, vec![235, 128, 235, 128]);
        let img = decode_frame(&frame).unwrap();
        assert_eq!(&img.pixels[0..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn yuyv_alpha_is_opaque() {
        let frame = yuyv_frame(2, 1, vec![100, 128, 150, 128]);
        let img = decode_frame(&frame).unwrap();
        assert_eq!(img.pixels[3], 255);
        assert_eq!(img.pixels[7], 255);
    }

    #[test]
    fn yuyv_rejects_short_buffer() {
        let frame = yuyv_frame(4, 1, vec![16, 128, 16, 128]); // needs 16 bytes
        assert!(decode_frame(&frame).is_err());
    }

    #[test]
    fn yuyv_rejects_odd_width() {
        let frame = yuyv_frame(3, 1, vec![16, 128, 16, 128, 16, 128]);
        assert!(decode_frame(&frame).is_err());
    }

    #[test]
    fn yuyv_honors_stride_padding() {
        // 2px row (4 bytes) + 4 bytes padding = stride 8.
        let mut frame = yuyv_frame(2, 2, vec![0u8; 16]);
        frame.bytes_per_line = 8;
        frame.data[0..4].copy_from_slice(&[235, 128, 235, 128]); // row 0 white
        frame.data[8..12].copy_from_slice(&[16, 128, 16, 128]); // row 1 black
        let img = decode_frame(&frame).unwrap();
        assert_eq!(&img.pixels[0..4], &[255, 255, 255, 255]); // row 0
        assert_eq!(&img.pixels[8..12], &[0, 0, 0, 255]); // row 1 (8 bytes/row, 2px)
    }

    // A real 16x16 baseline JPEG fixture (generated with ffmpeg).
    const GRAY16_JPEG: &[u8] = include_bytes!("../../tests/fixtures/gray16.jpg");

    #[test]
    fn mjpeg_decodes_real_jpeg_fixture() {
        let frame = Frame {
            format: PixelFormat::Mjpeg,
            width: 16,
            height: 16,
            bytes_per_line: 0,
            color_range: ColorRange::Limited,
            color_space: ColorSpace::Unknown,
            timestamp: Duration::ZERO,
            data: GRAY16_JPEG.to_vec(),
        };
        let img = decode_frame(&frame).unwrap();
        assert_eq!(img.width, 16);
        assert_eq!(img.height, 16);
        assert_eq!(img.pixels.len(), RgbaImage::byte_len(16, 16));
        // Every pixel opaque.
        assert!(img.pixels.chunks_exact(4).all(|p| p[3] == 255));
    }

    #[test]
    fn mjpeg_rejects_garbage() {
        let frame = Frame {
            format: PixelFormat::Mjpeg,
            width: 2,
            height: 2,
            bytes_per_line: 0,
            color_range: ColorRange::Limited,
            color_space: ColorSpace::Unknown,
            timestamp: Duration::ZERO,
            data: vec![0xFF, 0xD8, 0x00, 0x01, 0x02],
        };
        assert!(decode_frame(&frame).is_err());
    }
}
