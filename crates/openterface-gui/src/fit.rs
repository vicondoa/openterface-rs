//! Aspect-ratio "contain" fit for the video frame.
//!
//! The renderer draws the captured frame onto a full-viewport triangle; without
//! correction that stretches the image to the window shape. [`contain_scale`]
//! computes the per-axis UV scale that fits the frame inside the viewport while
//! preserving its aspect ratio, leaving symmetric black bars (letterbox or
//! pillarbox). It is pure arithmetic, so it lives outside the `display` feature
//! and is unit-tested with no GPU.

/// Computes the per-axis UV scale that fits a `tw × th` image inside a
/// `vw × vh` viewport while preserving aspect ratio ("contain"): one axis is
/// `1.0` (fills the viewport) and the other is `<= 1.0` (the image occupies that
/// fraction, leaving symmetric black bars). Degenerate zero dimensions fall back
/// to an identity fit.
pub fn contain_scale(tw: u32, th: u32, vw: u32, vh: u32) -> (f32, f32) {
    if tw == 0 || th == 0 || vw == 0 || vh == 0 {
        return (1.0, 1.0);
    }
    let image_aspect = tw as f32 / th as f32;
    let view_aspect = vw as f32 / vh as f32;
    if view_aspect > image_aspect {
        // Viewport is wider than the image: pillarbox (bars left/right).
        (image_aspect / view_aspect, 1.0)
    } else {
        // Viewport is taller than the image: letterbox (bars top/bottom).
        (1.0, view_aspect / image_aspect)
    }
}

/// Packs the contain-fit scale into the 16-byte uniform layout the renderer's
/// `Params` shader struct expects (`scale.xy` followed by 8 bytes of padding),
/// little-endian.
pub fn pack_scale(sx: f32, sy: f32) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&sx.to_le_bytes());
    out[4..8].copy_from_slice(&sy.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::{contain_scale, pack_scale};

    #[test]
    fn exact_aspect_match_fills_viewport() {
        // A 16:9 image in a 16:9 viewport needs no bars (scale 1,1).
        assert_eq!(contain_scale(1920, 1080, 1280, 720), (1.0, 1.0));
        assert_eq!(contain_scale(16, 9, 16, 9), (1.0, 1.0));
    }

    #[test]
    fn wider_viewport_pillarboxes() {
        // 16:9 image in a 2:1 (wider) viewport: bars left/right, full height.
        let (sx, sy) = contain_scale(1920, 1080, 2000, 1000);
        assert!(sx < 1.0, "expected horizontal shrink, got {sx}");
        assert_eq!(sy, 1.0);
        // The image aspect (16:9) over the view aspect (2:1).
        assert!((sx - (16.0 / 9.0) / 2.0).abs() < 1e-6);
    }

    #[test]
    fn taller_viewport_letterboxes() {
        // 16:9 image in a 1:1 (taller) viewport: bars top/bottom, full width.
        let (sx, sy) = contain_scale(1920, 1080, 1000, 1000);
        assert_eq!(sx, 1.0);
        assert!(sy < 1.0, "expected vertical shrink, got {sy}");
        assert!((sy - 1.0 / (16.0 / 9.0)).abs() < 1e-6);
    }

    #[test]
    fn degenerate_dimensions_fall_back_to_identity() {
        assert_eq!(contain_scale(0, 1080, 1280, 720), (1.0, 1.0));
        assert_eq!(contain_scale(1920, 0, 1280, 720), (1.0, 1.0));
        assert_eq!(contain_scale(1920, 1080, 0, 720), (1.0, 1.0));
        assert_eq!(contain_scale(1920, 1080, 1280, 0), (1.0, 1.0));
    }

    #[test]
    fn pack_scale_is_little_endian_xy_with_padding() {
        let bytes = pack_scale(1.0, 0.5);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(&bytes[4..8], &0.5f32.to_le_bytes());
        assert_eq!(&bytes[8..16], &[0u8; 8]);
    }
}
