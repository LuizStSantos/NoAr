//! Drawing text into RGBA blocks and lines, and into YUV pictures.

use std::sync::PoisonError;

use anyhow::{Result, anyhow};
use cosmic_text::{Align, Attrs, Buffer, Color, Metrics, Shaping, Weight, Wrap};

use super::fonts::{TextRenderer, family_attrs, renderer, trim_glyph_cache};
use crate::{
    frame::{PixelFormat, VideoFrame},
    utils::config::RgbaColor,
};

#[cfg(feature = "desktop-base")]
pub(crate) struct TextBitmap {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[cfg(feature = "desktop-base")]
pub(crate) fn render_wrapped_text_bitmap(
    text: &str,
    font_size: f32,
    font_weight: Weight,
    color: RgbaColor,
    max_width: u32,
) -> Result<TextBitmap> {
    render_text_bitmap(
        text,
        font_size,
        font_weight,
        color,
        Some(max_width.max(2)),
        Some(Wrap::WordOrGlyph),
        Some(Align::Center),
    )
}

#[cfg(feature = "desktop-base")]
pub(crate) fn render_left_aligned_text_bitmap(
    text: &str,
    font_size: f32,
    font_weight: Weight,
    color: RgbaColor,
    max_width: u32,
) -> Result<TextBitmap> {
    render_text_bitmap(
        text,
        font_size,
        font_weight,
        color,
        Some(max_width.max(2)),
        Some(Wrap::WordOrGlyph),
        Some(Align::Left),
    )
}

#[cfg(feature = "desktop-base")]
fn render_text_bitmap(
    text: &str,
    font_size: f32,
    font_weight: Weight,
    color: RgbaColor,
    max_width: Option<u32>,
    wrap: Option<Wrap>,
    align: Option<Align>,
) -> Result<TextBitmap> {
    if text.trim().is_empty() {
        return Err(anyhow!("text bitmap input is empty"));
    }

    let estimated_width = (text.chars().count() as f32 * font_size * 0.75).ceil() as u32 + 8;
    let width = max_width
        .unwrap_or(estimated_width)
        .min(estimated_width.max(2))
        .max(2);
    let line_height = font_size * 1.25;
    let estimated_lines =
        ((estimated_width as f32 / width as f32).ceil() as u32).max(text.lines().count() as u32);
    let height = ((line_height * estimated_lines as f32).ceil() as u32 + 4).max(2);
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];
    let mut renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
    let TextRenderer {
        font_system,
        swash_cache,
        cache_checked,
    } = &mut *renderer;
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(Some(width as f32), Some(height as f32));

    if let Some(wrap) = wrap {
        buffer.set_wrap(wrap);
    }
    buffer.set_text(
        text,
        &Attrs::new().weight(font_weight),
        Shaping::Advanced,
        align,
    );
    buffer.shape_until_scroll(font_system, false);

    let mut bounds = Bounds::default();
    buffer.draw(
        font_system,
        swash_cache,
        Color::rgba(color.r, color.g, color.b, color.a),
        |x, y, pixel_width, pixel_height, color| {
            let color = rgba_color(color);
            let Some(dest) = PixelRect::new(x, y, pixel_width, pixel_height, width, height) else {
                return;
            };

            bounds.include(dest.x, dest.y, dest.width, dest.height);

            for py in 0..dest.height {
                for px in 0..dest.width {
                    let idx = ((dest.y + py) * width as usize + dest.x + px) * 4;
                    alpha_composite(&mut rgba[idx..idx + 4], color);
                }
            }
        },
    );

    trim_glyph_cache(swash_cache, cache_checked);
    let Some(bounds) = bounds.finish() else {
        return Err(anyhow!("text bitmap produced no visible pixels"));
    };
    let mut cropped = vec![0_u8; bounds.width * bounds.height * 4];

    for y in 0..bounds.height {
        let src_start = ((bounds.y + y) * width as usize + bounds.x) * 4;
        let src_end = src_start + bounds.width * 4;
        let dst_start = y * bounds.width * 4;
        cropped[dst_start..dst_start + bounds.width * 4].copy_from_slice(&rgba[src_start..src_end]);
    }

    Ok(TextBitmap {
        pixels: cropped,
        width: bounds.width as u32,
        height: bounds.height as u32,
    })
}

/// How a text sits across its box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlockAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Draw `text` into a `width` x `height` RGBA block, wrapped to the width
/// and centred top to bottom; what does not fit is cut off.
pub(crate) struct TextStyle<'a> {
    pub font_family: Option<&'a str>,
    /// Pixels.
    pub font_size: f32,
    pub bold: bool,
    pub color: RgbaColor,
    pub align: BlockAlign,
}

pub(crate) fn render_text_block(
    text: &str,
    style: &TextStyle<'_>,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    render_graphic_text(text, style, Some(width), height).map(|(_, pixels)| pixels)
}

/// Shape a complete single line before allocating its bounded scrolling strip.
pub(crate) fn render_text_line(
    text: &str,
    style: &TextStyle<'_>,
    height: u32,
) -> Result<(u32, Vec<u8>)> {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");

    render_graphic_text(&line, style, None, height)
}

fn render_graphic_text(
    text: &str,
    style: &TextStyle<'_>,
    width: Option<u32>,
    height: u32,
) -> Result<(u32, Vec<u8>)> {
    let TextStyle {
        font_family,
        font_size,
        bold,
        color,
        align,
    } = *style;

    if !font_size.is_finite() || font_size <= 0.0 {
        return Err(anyhow!("text font size must be a positive number"));
    }

    let scrolling = width.is_none();
    let line_height = (font_size * 1.2).ceil();
    let mut renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
    let TextRenderer {
        font_system,
        swash_cache,
        cache_checked,
    } = &mut *renderer;
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
    let attrs = match font_family.filter(|family| !family.trim().is_empty()) {
        Some(family) => family_attrs(font_system, family, weight),
        None => Attrs::new().weight(weight),
    };

    buffer.set_size(width.map(|width| width as f32), None);
    buffer.set_wrap(if scrolling {
        Wrap::None
    } else {
        Wrap::WordOrGlyph
    });
    buffer.set_text(
        text,
        &attrs,
        Shaping::Advanced,
        Some(match align {
            BlockAlign::Left => Align::Left,
            BlockAlign::Center => Align::Center,
            BlockAlign::Right => Align::Right,
        }),
    );
    buffer.shape_until_scroll(font_system, false);

    let measured = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0_f32, f32::max);
    let width = width.unwrap_or(((measured.ceil() as u32 + 3) & !1).max(2));
    anyhow::ensure!(
        !scrolling || (width <= 32768 && u64::from(width) * u64::from(height) <= 8 * 1024 * 1024),
        "scrolling text strip exceeds 32768 pixels or 32 MiB; reduce text or font size"
    );
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];

    if text.trim().is_empty() || width < 2 || height < 2 {
        return Ok((width, rgba));
    }

    let text_height: f32 = buffer.layout_runs().map(|run| run.line_height).sum();
    let top = ((height as f32 - text_height) / 2.0).round() as i32;

    buffer.draw(
        font_system,
        swash_cache,
        Color::rgba(color.r, color.g, color.b, color.a),
        |x, y, pixel_width, pixel_height, color| {
            let color = rgba_color(color);
            let Some(dest) = PixelRect::new(x, y + top, pixel_width, pixel_height, width, height)
            else {
                return;
            };

            for py in 0..dest.height {
                for px in 0..dest.width {
                    let idx = ((dest.y + py) * width as usize + dest.x + px) * 4;
                    alpha_composite(&mut rgba[idx..idx + 4], color);
                }
            }
        },
    );

    trim_glyph_cache(swash_cache, cache_checked);

    Ok((width, rgba))
}

pub(crate) fn rgba_to_yuva420p(rgba: &[u8], width: u32, height: u32) -> Result<VideoFrame> {
    let mut frame = VideoFrame::new(PixelFormat::Yuva420p, width, height);
    let width = width as usize;
    let height = height as usize;

    let y_stride = frame.stride(0);
    let u_stride = frame.stride(1);
    let v_stride = frame.stride(2);
    let a_stride = frame.stride(3);

    for y in 0..height {
        for x in 0..width {
            let src_idx = (y * width + x) * 4;
            let (yy, _, _) = rgb_to_yuv(rgba[src_idx], rgba[src_idx + 1], rgba[src_idx + 2]);
            frame.data_mut(0)[y * y_stride + x] = yy;
            frame.data_mut(3)[y * a_stride + x] = rgba[src_idx + 3];
        }
    }

    for y in 0..height / 2 {
        for x in 0..width / 2 {
            let mut r = 0_u16;
            let mut g = 0_u16;
            let mut b = 0_u16;
            let mut count = 0_u16;

            for dy in 0..2 {
                for dx in 0..2 {
                    let px = x * 2 + dx;
                    let py = y * 2 + dy;
                    let src_idx = (py * width + px) * 4;
                    r += rgba[src_idx] as u16;
                    g += rgba[src_idx + 1] as u16;
                    b += rgba[src_idx + 2] as u16;
                    count += 1;
                }
            }

            let (_, u, v) = rgb_to_yuv((r / count) as u8, (g / count) as u8, (b / count) as u8);
            frame.data_mut(1)[y * u_stride + x] = u;
            frame.data_mut(2)[y * v_stride + x] = v;
        }
    }

    Ok(frame)
}

fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = i32::from(r);
    let g = i32::from(g);
    let b = i32::from(b);
    let y = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
    let u = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
    (clamp_u8(y), clamp_u8(u), clamp_u8(v))
}

pub(super) fn rgba_color(color: Color) -> RgbaColor {
    let [r, g, b, a] = color.as_rgba();
    RgbaColor { r, g, b, a }
}

pub(crate) fn alpha_composite(dst: &mut [u8], src: RgbaColor) {
    let alpha = src.a as u16;
    let inv_alpha = 255 - alpha;
    dst[0] = (((u16::from(dst[0]) * inv_alpha) + (u16::from(src.r) * alpha) + 127) / 255) as u8;
    dst[1] = (((u16::from(dst[1]) * inv_alpha) + (u16::from(src.g) * alpha) + 127) / 255) as u8;
    dst[2] = (((u16::from(dst[2]) * inv_alpha) + (u16::from(src.b) * alpha) + 127) / 255) as u8;
    dst[3] = (u16::from(dst[3]) + alpha - ((u16::from(dst[3]) * alpha + 127) / 255)) as u8;
}

pub(super) fn draw_box(rgba: &mut [u8], width: u32, bounds: &ResolvedBounds, color: RgbaColor) {
    let width = width as usize;

    for y in bounds.y..bounds.y + bounds.height {
        for x in bounds.x..bounds.x + bounds.width {
            let idx = (y * width + x) * 4;
            alpha_composite(&mut rgba[idx..idx + 4], color);
        }
    }
}

#[inline]
fn clamp_u8(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

#[inline]
pub(super) fn even_signed(value: i32) -> i32 {
    value & !1
}

pub(super) struct PixelRect {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

impl PixelRect {
    pub(super) fn new(
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        max_width: u32,
        max_height: u32,
    ) -> Option<Self> {
        let left = x.max(0);
        let top = y.max(0);
        let right = (x + width as i32).min(max_width as i32);
        let bottom = (y + height as i32).min(max_height as i32);

        if left >= right || top >= bottom {
            return None;
        }

        Some(Self {
            x: left as usize,
            y: top as usize,
            width: (right - left) as usize,
            height: (bottom - top) as usize,
        })
    }
}

#[derive(Default)]
pub(super) struct Bounds {
    pub(super) x_min: Option<usize>,
    pub(super) y_min: Option<usize>,
    pub(super) x_max: usize,
    pub(super) y_max: usize,
}

impl Bounds {
    pub(super) fn include(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.x_min = Some(self.x_min.map_or(x, |min| min.min(x)));
        self.y_min = Some(self.y_min.map_or(y, |min| min.min(y)));
        self.x_max = self.x_max.max(x + width);
        self.y_max = self.y_max.max(y + height);
    }

    pub(super) fn finish(self) -> Option<ResolvedBounds> {
        let x = self.x_min?;
        let y = self.y_min?;
        Some(ResolvedBounds {
            x,
            y,
            width: self.x_max.saturating_sub(x).max(1),
            height: self.y_max.saturating_sub(y).max(1),
        })
    }
}

pub(super) struct ResolvedBounds {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) width: usize,
    pub(super) height: usize,
}

impl ResolvedBounds {
    pub(super) fn expand(&mut self, padding: usize, max_width: usize, max_height: usize) {
        let x = self.x.saturating_sub(padding);
        let y = self.y.saturating_sub(padding);
        let right = (self.x + self.width + padding).min(max_width);
        let bottom = (self.y + self.height + padding).min(max_height);
        self.x = x;
        self.y = y;
        self.width = right.saturating_sub(x).max(1);
        self.height = bottom.saturating_sub(y).max(1);
    }
}
