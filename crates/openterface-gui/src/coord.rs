//! Window → target coordinate mapping (pure, always built).
//!
//! Maps a pointer position inside the content area to the CH9329 absolute
//! coordinate space (`0..=4095` per axis), matching the C++ behavior of mapping
//! window coordinates to the target and clamping.

use openterface_core::event::{AbsPosition, ABS_MAX};

use crate::fit::contain_scale;

/// Maps a pointer at `(x, y)` within a content area of `width × height` pixels
/// to a CH9329 [`AbsPosition`] in `0..=4095`. Out-of-range inputs are clamped.
#[must_use]
pub fn window_to_abs(x: f64, y: f64, width: u32, height: u32) -> AbsPosition {
    let map = |v: f64, span: u32| -> u16 {
        if span <= 1 {
            return 0;
        }
        let frac = (v / f64::from(span - 1)).clamp(0.0, 1.0);
        (frac * f64::from(ABS_MAX)).round() as u16
    };
    AbsPosition {
        x: map(x, width),
        y: map(y, height),
    }
}

/// Maps a pointer at `(x, y)` in a `vw × vh` window, where a `tw × th` frame is
/// displayed letterboxed (aspect-preserving "contain" fit), to a CH9329
/// [`AbsPosition`]. The black bars are excluded from the target space: a pointer
/// inside a bar clamps to the nearest edge of the displayed frame, so input
/// always lands on the target and never in dead space. With a frame that already
/// fills the window (no bars) this is identical to [`window_to_abs`].
#[must_use]
pub fn window_to_abs_fit(x: f64, y: f64, tw: u32, th: u32, vw: u32, vh: u32) -> AbsPosition {
    let (sx, sy) = contain_scale(tw, th, vw, vh);
    // Maps a coordinate to `0..=ABS_MAX` over the centered displayed image,
    // whose length is `span * scale` (the rest of `span` is the two bars).
    let map = |v: f64, span: u32, scale: f32| -> u16 {
        let span = f64::from(span);
        let img_len = span * f64::from(scale);
        // `img_len - 1` keeps the first/last displayed pixel at the extremes,
        // matching `window_to_abs` exactly when there are no bars (scale = 1).
        let denom = img_len - 1.0;
        if denom <= 0.0 {
            return 0;
        }
        let offset = (span - img_len) / 2.0;
        let frac = ((v - offset) / denom).clamp(0.0, 1.0);
        (frac * f64::from(ABS_MAX)).round() as u16
    };
    AbsPosition {
        x: map(x, vw, sx),
        y: map(y, vh, sy),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_map_to_extremes() {
        assert_eq!(
            window_to_abs(0.0, 0.0, 1920, 1080),
            AbsPosition { x: 0, y: 0 }
        );
        let br = window_to_abs(1919.0, 1079.0, 1920, 1080);
        assert_eq!(br, AbsPosition { x: 4095, y: 4095 });
    }

    #[test]
    fn center_maps_to_midpoint() {
        let c = window_to_abs(959.5, 539.5, 1920, 1080);
        // ~2047/2048.
        assert!((2047..=2048).contains(&c.x));
        assert!((2047..=2048).contains(&c.y));
    }

    #[test]
    fn out_of_range_is_clamped() {
        assert_eq!(
            window_to_abs(-50.0, -50.0, 800, 600),
            AbsPosition { x: 0, y: 0 }
        );
        assert_eq!(
            window_to_abs(99999.0, 99999.0, 800, 600),
            AbsPosition { x: 4095, y: 4095 }
        );
    }

    #[test]
    fn degenerate_size_is_safe() {
        assert_eq!(window_to_abs(10.0, 10.0, 0, 0), AbsPosition { x: 0, y: 0 });
        assert_eq!(window_to_abs(10.0, 10.0, 1, 1), AbsPosition { x: 0, y: 0 });
    }

    #[test]
    fn fit_with_no_bars_matches_plain_mapping() {
        // A frame that already fills the window (matching aspect) must map
        // identically to the non-letterboxed path.
        for &(x, y) in &[(0.0, 0.0), (960.0, 540.0), (1919.0, 1079.0)] {
            assert_eq!(
                window_to_abs_fit(x, y, 1920, 1080, 1920, 1080),
                window_to_abs(x, y, 1920, 1080),
            );
        }
    }

    #[test]
    fn pillarbox_excludes_side_bars() {
        // 16:9 frame in a 2:1 (wider) window: vertical bars left/right.
        let (tw, th, vw, vh) = (1920, 1080, 2000, 1000);
        // A pointer in the left bar clamps to the frame's left edge (x = 0).
        assert_eq!(window_to_abs_fit(0.0, 500.0, tw, th, vw, vh).x, 0);
        // A pointer in the right bar clamps to the frame's right edge.
        assert_eq!(window_to_abs_fit(1999.0, 500.0, tw, th, vw, vh).x, 4095);
        // The vertical axis fills the window, so it maps top→0, bottom→max.
        assert_eq!(window_to_abs_fit(1000.0, 0.0, tw, th, vw, vh).y, 0);
        assert_eq!(window_to_abs_fit(1000.0, 999.0, tw, th, vw, vh).y, 4095);
        // The window center is the frame center on both axes.
        let c = window_to_abs_fit(1000.0, 500.0, tw, th, vw, vh);
        assert!((2040..=2055).contains(&c.x), "x={}", c.x);
        assert!((2040..=2055).contains(&c.y), "y={}", c.y);
    }

    #[test]
    fn letterbox_excludes_top_bottom_bars() {
        // 16:9 frame in a 1:1 (taller) window: horizontal bars top/bottom.
        let (tw, th, vw, vh) = (1920, 1080, 1000, 1000);
        // A pointer in the top bar clamps to the frame's top edge (y = 0).
        assert_eq!(window_to_abs_fit(500.0, 0.0, tw, th, vw, vh).y, 0);
        // A pointer in the bottom bar clamps to the frame's bottom edge.
        assert_eq!(window_to_abs_fit(500.0, 999.0, tw, th, vw, vh).y, 4095);
        // The horizontal axis fills the window.
        assert_eq!(window_to_abs_fit(0.0, 500.0, tw, th, vw, vh).x, 0);
        assert_eq!(window_to_abs_fit(999.0, 500.0, tw, th, vw, vh).x, 4095);
    }

    #[test]
    fn fit_degenerate_is_safe() {
        // Zero window dimensions clamp to origin and never panic.
        assert_eq!(
            window_to_abs_fit(10.0, 10.0, 1920, 1080, 0, 0),
            AbsPosition { x: 0, y: 0 }
        );
        // Zero frame dimensions fall back to an identity fit (full-window map).
        assert_eq!(
            window_to_abs_fit(10.0, 10.0, 0, 0, 800, 600),
            window_to_abs(10.0, 10.0, 800, 600),
        );
    }
}
