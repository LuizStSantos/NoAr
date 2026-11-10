//! Prepared single-line text strips, clipped and repeated inside a layer.

use std::time::Duration;

use anyhow::{Result, ensure};

use crate::{
    compositor::{
        overlay::{OverlayRef, chroma_alpha},
        text::{BlockAlign, TextStyle, render_text_line, rgba_to_yuva420p},
    },
    frame::VideoFrame,
};

use super::geometry::even_down;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use crate::{compositor::overlay::OverlayFrame, frame::PixelFormat};

    use super::*;

    fn strip(direction: ScrollDirection) -> RenderedScroll {
        let mut frame = VideoFrame::new(PixelFormat::Yuva420p, 80, 20);
        frame.data_mut(0).fill(235);
        frame.data_mut(1).fill(128);
        frame.data_mut(2).fill(128);
        frame.data_mut(3).fill(255);

        RenderedScroll {
            alpha: chroma_alpha(&frame, 80, 20),
            frame,
            options: TextScroll {
                direction,
                speed: 80.0,
                gap: 20,
            },
        }
    }

    #[test]
    fn movement_uses_elapsed_time_output_pixels_and_authored_repeat_gap() {
        let left = strip(ScrollDirection::Left);
        let right = strip(ScrollDirection::Right);
        assert_eq!(
            left.positions(Duration::ZERO, 100).collect::<Vec<_>>(),
            [100]
        );
        assert_eq!(
            right.positions(Duration::ZERO, 100).collect::<Vec<_>>(),
            [-80]
        );
        assert_eq!(
            left.positions(Duration::from_secs(1), 100)
                .collect::<Vec<_>>(),
            [20]
        );
        assert_eq!(
            right
                .positions(Duration::from_secs(1), 100)
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(
            left.positions(Duration::from_secs(2), 100)
                .collect::<Vec<_>>(),
            [-60, 40]
        );
        assert_eq!(
            right
                .positions(Duration::from_secs(2), 100)
                .collect::<Vec<_>>(),
            [-20, 80]
        );
        // A long-running take keeps a bounded number of visible repetitions.
        assert!(left.positions(Duration::from_secs(72 * 3600), 100).count() <= 3);
    }

    fn background(alpha: u8) -> OverlayFrame {
        let mut frame = VideoFrame::new(PixelFormat::Yuva420p, 100, 20);
        frame.data_mut(0).fill(80);
        frame.data_mut(1).fill(128);
        frame.data_mut(2).fill(128);
        frame.data_mut(3).fill(alpha);

        OverlayFrame {
            chroma_alpha: chroma_alpha(&frame, 100, 20),
            frame,
            x: 20,
            y: 20,
            width: 100,
            height: 20,
            opacity: 255,
        }
    }

    #[test]
    fn scrolling_is_clipped_to_the_layer_and_respects_opacity() {
        let mut target = VideoFrame::new(PixelFormat::Yuv420p, 160, 60);
        target.data_mut(0).fill(16);
        target.data_mut(1).fill(128);
        target.data_mut(2).fill(128);
        strip(ScrollDirection::Left).blend(
            &mut target,
            background(0).as_ref(),
            100,
            Duration::from_secs(2),
            0.5,
        );
        let stride = target.stride(0);

        for y in 0..60 {
            for x in 0..160 {
                let luma = target.data(0)[y * stride + x];

                if (20..40).contains(&y) && ((20..40).contains(&x) || (60..120).contains(&x)) {
                    assert!((124..=126).contains(&luma), "{x},{y}: {luma}");
                } else {
                    assert_eq!(luma, 16, "pixels escaped the viewport at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn a_scrolling_layer_applies_opacity_once_to_text_and_fill() {
        let mut target = VideoFrame::new(PixelFormat::Yuv420p, 160, 60);
        target.data_mut(0).fill(16);
        target.data_mut(1).fill(128);
        target.data_mut(2).fill(128);
        strip(ScrollDirection::Left).blend(
            &mut target,
            background(255).as_ref(),
            100,
            Duration::from_secs(2),
            0.5,
        );
        let stride = target.stride(0);
        assert_eq!(
            target.data(0)[30 * stride + 30],
            126,
            "text must blend directly with the original picture"
        );
        assert_eq!(
            target.data(0)[30 * stride + 50],
            48,
            "the gap must retain the layer fill"
        );
        assert_eq!(
            target.data(0)[30 * stride + 10],
            16,
            "nothing may escape the viewport"
        );
    }

    #[test]
    fn full_text_is_shaped_without_wrapping_and_oversized_strips_are_rejected() {
        let style = TextStyle {
            font_family: None,
            font_size: 18.0,
            bold: false,
            color: crate::RgbaColor::opaque(255, 255, 255),
            align: BlockAlign::Left,
        };
        let options = strip(ScrollDirection::Left).options;
        let text = "São Paulo · USD 5.10 · EUR 5.60 · A complete long headline";
        let scroll = RenderedScroll::prepare(text, &style, 24, options).unwrap();
        assert!(scroll.frame.width() > 100);
        assert!(scroll.alpha.iter().any(|&value| value > 0));
        let large = TextStyle {
            font_size: 500.0,
            ..style
        };
        assert!(RenderedScroll::prepare(&"W".repeat(1000), &large, 500, options).is_err());
    }
}

/// Speed and spacing use output pixels, independent of browser preview size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextScroll {
    pub direction: ScrollDirection,
    pub speed: f64,
    pub gap: u32,
}

pub(super) struct RenderedScroll {
    frame: VideoFrame,
    alpha: Vec<u8>,
    options: TextScroll,
}

impl RenderedScroll {
    pub fn prepare(
        text: &str,
        style: &TextStyle<'_>,
        height: u32,
        options: TextScroll,
    ) -> Result<Self> {
        ensure!(
            options.speed.is_finite() && (1.0..=2000.0).contains(&options.speed),
            "scroll speed must be 1 to 2000 output pixels per second"
        );
        ensure!(
            options.gap <= 4096,
            "scroll gap must be 0 to 4096 output pixels"
        );
        let style = TextStyle {
            align: BlockAlign::Left,
            ..*style
        };
        let (width, rgba) = render_text_line(text, &style, height)?;
        let frame = rgba_to_yuva420p(&rgba, width, height)?;

        Ok(Self {
            alpha: chroma_alpha(&frame, width, height),
            frame,
            options,
        })
    }

    pub fn bytes(&self) -> usize {
        self.frame.width() as usize * self.frame.height() as usize * 3
    }

    /// Start outside the viewport, then repeat with only the authored gap.
    /// Elapsed time comes from the take, so refreshing data never resets motion.
    pub fn positions(&self, elapsed: Duration, viewport: u32) -> impl Iterator<Item = i32> {
        let text_width = self.frame.width();
        let period = f64::from(text_width + self.options.gap).max(2.0);
        let distance = elapsed.as_secs_f64() * self.options.speed;
        let first = ((distance - f64::from(viewport + text_width)) / period)
            .ceil()
            .max(0.0);
        let last = (distance / period).floor();
        let direction = self.options.direction;

        (0..((last - first + 1.0).max(0.0) as usize)).map(move |index| {
            let repetition = if direction == ScrollDirection::Left {
                first + index as f64
            } else {
                last - index as f64
            };
            let x = f64::from(viewport) - distance + repetition * period;
            let x = if direction == ScrollDirection::Left {
                x
            } else {
                f64::from(viewport) - f64::from(text_width) - x
            };

            even_down(x.floor() as i64) as i32
        })
    }

    /// Composite foreground and background together, then apply group opacity once.
    /// All glyphs and alpha planes are cached; this path allocates no frame buffers.
    pub fn blend(
        &self,
        target: &mut VideoFrame,
        background: OverlayRef<'_>,
        clip_width: u32,
        elapsed: Duration,
        opacity: f64,
    ) {
        let opacity = (f64::from(background.opacity) * opacity.clamp(0.0, 1.0)).round() as u64;

        if opacity == 0 {
            return;
        }

        for plane in 0..3 {
            let factor = if plane == 0 { 1 } else { 2 };
            let x = background.x / factor as i32;
            let y = background.y / factor as i32;
            let left = (-background.x).max(0) as usize / factor;
            let top = (-background.y).max(0) as usize / factor;
            let right = clip_width.min((target.width() as i32 - background.x).max(0) as u32)
                as usize
                / factor;
            let bottom = background
                .height
                .min((target.height() as i32 - background.y).max(0) as u32)
                as usize
                / factor;
            let glyph_width = self.frame.width() as usize / factor;
            let background_stride = background.frame.stride(plane);
            let foreground_stride = self.frame.stride(plane);
            let target_stride = target.stride(plane);
            let (background_alpha, background_alpha_stride) = if plane == 0 {
                (background.frame.data(3), background.frame.stride(3))
            } else {
                (background.chroma_alpha, background.chroma_alpha_stride)
            };
            let (foreground_alpha, foreground_alpha_stride) = if plane == 0 {
                (self.frame.data(3), self.frame.stride(3))
            } else {
                (self.alpha.as_slice(), glyph_width)
            };
            let background_pixels = background.frame.data(plane);
            let foreground_pixels = self.frame.data(plane);
            let target_pixels = target.data_mut(plane);

            for row in top..bottom {
                let mut offsets = self
                    .positions(elapsed, background.width)
                    .map(|offset| offset / factor as i32);
                let mut offset = offsets.next();

                for column in left..right {
                    while offset.is_some_and(|offset| column as i32 >= offset + glyph_width as i32)
                    {
                        offset = offsets.next();
                    }

                    let foreground_x = offset
                        .filter(|&offset| offset <= column as i32)
                        .map(|offset| (column as i32 - offset) as usize);
                    let alpha = foreground_x.map_or(0, |column| {
                        u64::from(foreground_alpha[row * foreground_alpha_stride + column])
                    });
                    let foreground = foreground_x.map_or(0, |column| {
                        u64::from(foreground_pixels[row * foreground_stride + column])
                    });
                    let background_alpha =
                        u64::from(background_alpha[row * background_alpha_stride + column]);
                    let foreground_weight = alpha * 255;
                    let background_weight = background_alpha * (255 - alpha);
                    let total_weight = (foreground_weight + background_weight) * opacity;

                    if total_weight == 0 {
                        continue;
                    }

                    let index =
                        (y + row as i32) as usize * target_stride + (x + column as i32) as usize;
                    let background = u64::from(background_pixels[row * background_stride + column]);
                    let original = u64::from(target_pixels[index]);
                    let denominator = 255 * 255 * 255;
                    let mixed = (foreground * foreground_weight + background * background_weight)
                        * opacity
                        + original * (denominator - total_weight);
                    target_pixels[index] = ((mixed + denominator / 2) / denominator) as u8;
                }
            }
        }
    }
}
