//! The text drawn over the clips: from the channel's settings or the clip's
//! name, placed, boxed and scrolled.

use std::{path::Path, sync::PoisonError};

use anyhow::{Context, Result, anyhow};
use cosmic_text::{Attrs, Buffer, Color, Metrics, Shaping, Weight};
use regex::Regex;

use super::{
    fonts::{TextRenderer, family_attrs, renderer, trim_glyph_cache},
    render::{
        Bounds, PixelRect, alpha_composite, draw_box, even_signed, rgba_color, rgba_to_yuva420p,
    },
};
use crate::{
    compositor::overlay::{OverlayFrame, blend_overlay, chroma_alpha},
    frame::VideoFrame,
    utils::{
        config::{RgbaColor, TextConfig, TextPosition, TextScroll, TextWeight},
        helper::even,
    },
};

pub struct TextOverlay {
    overlay: OverlayFrame,
    scroll: TextScroll,
    scroll_repeat: i32,
    fade_in_frames: i64,
    fade_out_frames: i64,
    fade_start_pts: i64,
    scroll_start_pts: i64,
    end_pts: Option<i64>,
    base_x: i32,
    base_y: i32,
    output_width: u32,
    fps: u32,
}

impl TextOverlay {
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        config: &TextConfig,
        media_path: &str,
        output_width: u32,
        output_height: u32,
        fps: u32,
        fade_start_pts: i64,
        scroll_start_pts: i64,
        end_pts: Option<i64>,
    ) -> Result<Option<Self>> {
        let Some(text) = overlay_text(config, media_path) else {
            return Ok(None);
        };

        if text.trim().is_empty() {
            return Ok(None);
        }

        let overlay = render_text_overlay(config, &text, output_width, output_height)?;
        let base_x = text_position(config.position_x, output_width, overlay.width);
        let base_y = text_position(config.position_y, output_height, overlay.height);

        let chroma_alpha = chroma_alpha(&overlay.frame, overlay.width, overlay.height);

        Ok(Some(Self {
            overlay: OverlayFrame {
                frame: overlay.frame,
                chroma_alpha,
                x: base_x,
                y: base_y,
                width: overlay.width,
                height: overlay.height,
                opacity: (config.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            },
            scroll: config.scroll,
            scroll_repeat: config.scroll_repeat,
            fade_in_frames: seconds_to_frames(config.fade_in_seconds, fps),
            fade_out_frames: seconds_to_frames(config.fade_out_seconds, fps),
            fade_start_pts,
            scroll_start_pts,
            end_pts,
            base_x,
            base_y,
            output_width,
            fps,
        }))
    }

    pub fn blend(&mut self, target: &mut VideoFrame, pts: i64, scroll_pts: i64) {
        let opacity = self.opacity_at(pts);

        if opacity <= 0.0 {
            return;
        }

        let x = self.x_at(scroll_pts);
        self.overlay.x = even_signed(x);
        self.overlay.y = even_signed(self.base_y);
        blend_overlay(target, self.overlay.as_ref(), opacity);
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.overlay.width, self.overlay.height)
    }

    fn opacity_at(&self, pts: i64) -> f64 {
        let mut opacity: f64 = 1.0;

        if self.fade_in_frames > 0 {
            let elapsed = (pts - self.fade_start_pts).max(0);
            opacity = opacity.min(elapsed as f64 / self.fade_in_frames as f64);
        }

        if self.fade_out_frames > 0
            && let Some(end_pts) = self.end_pts
        {
            let remaining = (end_pts - pts).max(0);
            opacity = opacity.min(remaining as f64 / self.fade_out_frames as f64);
        }
        opacity.clamp(0.0, 1.0)
    }

    fn x_at(&self, pts: i64) -> i32 {
        let elapsed = (pts - self.scroll_start_pts).max(0);

        match self.scroll {
            TextScroll::None => self.base_x,
            TextScroll::LeftToRight { pixels_per_second } => {
                let offset = self.scroll_offset(elapsed, pixels_per_second);
                -i32::try_from(self.overlay.width).unwrap_or(i32::MAX)
                    + i32::try_from(offset).unwrap_or(i32::MAX)
            }
            TextScroll::RightToLeft { pixels_per_second } => {
                let offset = self.scroll_offset(elapsed, pixels_per_second);
                i32::try_from(self.output_width).unwrap_or(i32::MAX)
                    - i32::try_from(offset).unwrap_or(i32::MAX)
            }
        }
    }

    fn scroll_offset(&self, elapsed: i64, pixels_per_second: u32) -> i64 {
        let offset = elapsed * i64::from(pixels_per_second) / i64::from(self.fps.max(1));
        let travel = i64::from(self.output_width) + i64::from(self.overlay.width);

        if travel <= 0 {
            return offset;
        }

        if self.scroll_repeat < 0 {
            return offset % travel;
        }

        let cycles = i64::from(self.scroll_repeat) + 1;

        if offset < travel.saturating_mul(cycles) {
            offset % travel
        } else {
            offset
        }
    }
}

pub(super) struct RenderedText {
    frame: VideoFrame,
    width: u32,
    height: u32,
}

fn overlay_text(config: &TextConfig, media_path: &str) -> Option<String> {
    if config.use_filename {
        if let Some(regex) = &config.filename_regex
            && let Ok(regex) = Regex::new(regex)
            && let Some(captures) = regex.captures(media_path)
            && let Some(capture) = captures.get(1)
        {
            return Some(capture.as_str().to_string());
        }

        return Path::new(media_path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
    }

    config.text.clone()
}

pub(super) fn render_text_overlay(
    config: &TextConfig,
    text: &str,
    output_width: u32,
    output_height: u32,
) -> Result<RenderedText> {
    if !config.font_size.is_finite() || config.font_size <= 0.0 {
        return Err(anyhow!("text font size must be a positive number"));
    }

    let padding = config
        .background
        .map(|background| background.padding)
        .unwrap_or(0);
    let render_width = render_width(config, text, output_width, padding);
    let text_width = render_width
        .saturating_sub(padding.saturating_mul(2))
        .max(2);
    let text_height = output_height
        .saturating_sub(padding.saturating_mul(2))
        .max(2);
    let line_height = (config.font_size + config.line_spacing.max(0.0))
        .ceil()
        .max(config.font_size);

    let mut rgba = vec![0_u8; (render_width as usize) * (output_height as usize) * 4];
    let mut renderer = renderer().lock().unwrap_or_else(PoisonError::into_inner);
    let TextRenderer {
        font_system,
        swash_cache,
        cache_checked,
    } = &mut *renderer;
    let metrics = Metrics::new(config.font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(text_width as f32), Some(text_height as f32));

    let weight = match config.font_weight {
        TextWeight::Normal => Weight::NORMAL,
        TextWeight::Semibold => Weight::SEMIBOLD,
        TextWeight::Bold => Weight::BOLD,
    };
    let attrs = match &config.font_family {
        Some(family) => family_attrs(font_system, family, weight),
        None => Attrs::new().weight(weight),
    };
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    let text_color = Color::rgba(
        config.text_color.r,
        config.text_color.g,
        config.text_color.b,
        config.text_color.a,
    );
    let mut bounds = Bounds::default();
    buffer.draw(
        font_system,
        swash_cache,
        text_color,
        |x, y, width, height, color| {
            let color = rgba_color(color);
            let Some(dest) = PixelRect::new(
                x + i32::try_from(padding).unwrap_or(0),
                y + i32::try_from(padding).unwrap_or(0),
                width,
                height,
                render_width,
                output_height,
            ) else {
                return;
            };
            bounds.include(dest.x, dest.y, dest.width, dest.height);

            for py in 0..dest.height {
                for px in 0..dest.width {
                    let idx = ((dest.y + py) * render_width as usize + dest.x + px) * 4;
                    alpha_composite(&mut rgba[idx..idx + 4], color);
                }
            }
        },
    );

    trim_glyph_cache(swash_cache, cache_checked);
    let Some(mut bounds) = bounds.finish() else {
        return Err(anyhow!("text overlay produced no visible pixels"));
    };
    bounds.expand(
        padding as usize,
        render_width as usize,
        output_height as usize,
    );

    if let Some(background) = config.background {
        let mut boxed = vec![0_u8; rgba.len()];
        draw_box(&mut boxed, render_width, &bounds, background.color);

        for y in bounds.y..bounds.y + bounds.height {
            for x in bounds.x..bounds.x + bounds.width {
                let idx = (y * render_width as usize + x) * 4;
                let text = RgbaColor {
                    r: rgba[idx],
                    g: rgba[idx + 1],
                    b: rgba[idx + 2],
                    a: rgba[idx + 3],
                };
                alpha_composite(&mut boxed[idx..idx + 4], text);
            }
        }

        rgba = boxed;
    }

    let width = even(bounds.width as u32).max(2);
    let height = even(bounds.height as u32).max(2);
    let mut cropped = vec![0_u8; width as usize * height as usize * 4];

    for y in 0..height as usize {
        for x in 0..width as usize {
            let src_idx = ((bounds.y + y) * render_width as usize + bounds.x + x) * 4;
            let dst_idx = (y * width as usize + x) * 4;
            cropped[dst_idx..dst_idx + 4].copy_from_slice(&rgba[src_idx..src_idx + 4]);
        }
    }

    Ok(RenderedText {
        frame: rgba_to_yuva420p(&cropped, width, height)
            .context("failed to create text overlay frame")?,
        width,
        height,
    })
}

fn render_width(config: &TextConfig, text: &str, output_width: u32, padding: u32) -> u32 {
    if matches!(config.scroll, TextScroll::None) {
        return output_width.max(2);
    }

    let estimated_text_width = (text.chars().count() as f32 * config.font_size * 0.75).ceil();
    let estimated = estimated_text_width as u32 + padding.saturating_mul(2);
    let max_width = output_width.saturating_mul(8).max(output_width);
    even(estimated.max(output_width).min(max_width)).max(2)
}

fn text_position(position: TextPosition, output: u32, overlay: u32) -> i32 {
    match position {
        TextPosition::Pixels(value) => value,
        TextPosition::Center => ((output as i32 - overlay as i32) / 2).max(0),
        TextPosition::End(offset) => output as i32 - overlay as i32 - offset,
    }
}

fn seconds_to_frames(seconds: f64, fps: u32) -> i64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        0
    } else {
        (seconds * f64::from(fps)).round().max(1.0) as i64
    }
}
