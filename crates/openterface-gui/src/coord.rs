//! Window → target coordinate mapping (pure, always built).
//!
//! Maps a pointer position inside the content area to the CH9329 absolute
//! coordinate space (`0..=4095` per axis), matching the C++ behavior of mapping
//! window coordinates to the target and clamping.

use openterface_core::event::{AbsPosition, ABS_MAX};

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
}
