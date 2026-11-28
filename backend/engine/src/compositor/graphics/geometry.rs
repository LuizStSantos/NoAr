//! Shared pixel snapping and rounded masks used by rasterization and image decoding.

use crate::{compositor::text::alpha_composite, utils::config::RgbaColor};

#[inline]
pub(super) fn even_down(value: i64) -> i64 {
    value & !1
}

/// Percent of `total` in whole pixels, rounded down to even for 4:2:0.
pub(super) fn percent_px(percent: f64, total: u32) -> i64 {
    even_down((percent / 100.0 * f64::from(total)).round() as i64)
}

/// Whether a pixel is inside a box with rounded corners of `radius` pixels.
pub(super) fn inside_rounded(x: u32, y: u32, width: u32, height: u32, radius: f64) -> bool {
    if radius <= 0.0 {
        return true;
    }

    let px = f64::from(x) + 0.5;
    let py = f64::from(y) + 0.5;
    let cx = px.clamp(radius, f64::from(width) - radius);
    let cy = py.clamp(radius, f64::from(height) - radius);

    (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius
}

pub(super) fn fill_rounded(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    radius: f64,
    color: RgbaColor,
) {
    let radius = radius.min(f64::from(width.min(height)) / 2.0);

    for y in 0..height {
        for x in 0..width {
            if inside_rounded(x, y, width, height, radius) {
                let idx = ((y * width + x) * 4) as usize;
                alpha_composite(&mut rgba[idx..idx + 4], color);
            }
        }
    }
}

/// Clear what falls outside the rounded corners of an RGBA block.
pub(super) fn cut_corners(rgba: &mut [u8], width: u32, height: u32, radius: f64) {
    let radius = radius.min(f64::from(width.min(height)) / 2.0);

    if radius <= 0.0 {
        return;
    }

    for y in 0..height {
        for x in 0..width {
            if !inside_rounded(x, y, width, height, radius) {
                let idx = ((y * width + x) * 4) as usize;
                rgba[idx..idx + 4].fill(0);
            }
        }
    }
}

/// Supersample only newly authored shapes. The legacy Box path stays exact.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_corners_are_cut() {
        assert!(!inside_rounded(0, 0, 40, 40, 10.0));
        assert!(inside_rounded(20, 0, 40, 40, 10.0));
        assert!(inside_rounded(5, 5, 40, 40, 10.0));
        assert!(inside_rounded(0, 0, 40, 40, 0.0));
    }
}
