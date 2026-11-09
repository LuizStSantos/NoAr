//! Layer rasterization and frame blending for the Graphics compositor.
//!
//! Rasterization (including image I/O) runs before a model is published; the
//! blending helpers only read prepared layers and cached animation frames.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, ensure};

use crate::{
    compositor::{
        overlay::{OverlayRef, blend_overlay, chroma_alpha},
        text::{TextStyle, alpha_composite, render_text_block, rgba_to_yuva420p},
    },
    frame::{PixelFormat, VideoFrame},
    utils::config::RgbaColor,
};

use super::{
    GraphicAnimation, GraphicContent, GraphicLayerSpec, GraphicShape, GraphicSpec, RenderedGraphic,
    RenderedLayer,
    geometry::{cut_corners, even_down, fill_rounded, percent_px},
    images,
    lshape::LShapeGraphic,
};

fn render_shape(
    shape: GraphicShape,
    width: u32,
    height: u32,
    radius: f64,
    stroke_width: f64,
    fill: Option<RgbaColor>,
    stroke: Option<RgbaColor>,
) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let (w, h) = (f64::from(width), f64::from(height));
    let line_width = stroke_width.max(0.0).min(w.min(h));
    let inside = |x: f64, y: f64, inset: f64| {
        let (iw, ih) = (w - inset * 2.0, h - inset * 2.0);

        if iw <= 0.0 || ih <= 0.0 || x < inset || y < inset || x > w - inset || y > h - inset {
            return false;
        }

        match shape {
            GraphicShape::Ellipse => {
                ((x - w / 2.0) / (iw / 2.0)).powi(2) + ((y - h / 2.0) / (ih / 2.0)).powi(2) <= 1.0
            }
            GraphicShape::Rounded => {
                let r = (radius - inset).max(0.0).min(iw.min(ih) / 2.0);
                let dx = ((x - w / 2.0).abs() - (iw / 2.0 - r)).max(0.0);
                let dy = ((y - h / 2.0).abs() - (ih / 2.0 - r)).max(0.0);
                dx * dx + dy * dy <= r * r
            }
            _ => true,
        }
    };

    for row in 0..height {
        for column in 0..width {
            let mut sum = [0.0; 4];

            for dy in [0.25, 0.75] {
                for dx in [0.25, 0.75] {
                    let (x, y) = (f64::from(column) + dx, f64::from(row) + dy);
                    let color = if shape == GraphicShape::Line {
                        if (y - h / 2.0).abs() <= line_width / 2.0 {
                            stroke.or(fill)
                        } else {
                            None
                        }
                    } else if inside(x, y, 0.0) {
                        if stroke.is_some() && line_width > 0.0 && !inside(x, y, line_width) {
                            stroke
                        } else {
                            fill
                        }
                    } else {
                        None
                    };

                    if let Some(color) = color {
                        sum[0] += f64::from(color.r) * f64::from(color.a);
                        sum[1] += f64::from(color.g) * f64::from(color.a);
                        sum[2] += f64::from(color.b) * f64::from(color.a);
                        sum[3] += f64::from(color.a);
                    }
                }
            }

            let offset = (row as usize * width as usize + column as usize) * 4;

            if sum[3] > 0.0 {
                for channel in 0..3 {
                    pixels[offset + channel] = (sum[channel] / sum[3]).round() as u8;
                }
                pixels[offset + 3] = (sum[3] / 4.0).round() as u8;
            }
        }
    }

    pixels
}

fn render_layer(
    layer: &GraphicLayerSpec,
    frame_width: u32,
    frame_height: u32,
) -> Result<Option<RenderedLayer>> {
    let x = percent_px(layer.x, frame_width);
    let y = percent_px(layer.y, frame_height);
    let width = percent_px(layer.width, frame_width).max(0) as u32;
    let height = percent_px(layer.height, frame_height).max(0) as u32;

    if width < 2 || height < 2 {
        return Ok(None);
    }

    let radius = layer.radius.max(0.0) / 100.0 * f64::from(frame_height);
    let image_animation = match &layer.content {
        GraphicContent::Image { path } => images::load_animation(path, width, height, radius)?,
        _ => None,
    };
    let mut scroll = None;
    let frame = match &layer.content {
        GraphicContent::Box { fill } => {
            let mut rgba = vec![0_u8; width as usize * height as usize * 4];
            fill_rounded(&mut rgba, width, height, radius, *fill);
            rgba_to_yuva420p(&rgba, width, height)?
        }
        GraphicContent::Shape {
            shape,
            fill,
            stroke,
            stroke_width,
        } => {
            let rgba = render_shape(
                *shape,
                width,
                height,
                radius,
                stroke_width / 100.0 * f64::from(frame_height),
                *fill,
                *stroke,
            );
            rgba_to_yuva420p(&rgba, width, height)?
        }
        GraphicContent::Text {
            text,
            scroll: options,
            font_family,
            font_size,
            bold,
            color,
            align,
            fill,
        } => {
            let font_px = (font_size / 100.0 * f64::from(frame_height)) as f32;
            let padding = if fill.is_some() {
                (f64::from(font_px) * 0.4).round() as u32
            } else {
                0
            };
            let inner = width.saturating_sub(padding * 2).max(2);
            let style = TextStyle {
                font_family: font_family.as_deref(),
                font_size: font_px,
                bold: *bold,
                color: *color,
                align: *align,
            };
            let text_rgba = if let Some(options) = options {
                scroll = Some(super::scroll::RenderedScroll::prepare(
                    text, &style, height, *options,
                )?);
                vec![0; inner as usize * height as usize * 4]
            } else {
                render_text_block(text, &style, inner, height)?
            };
            let mut rgba = vec![0_u8; width as usize * height as usize * 4];

            if let Some(fill) = fill {
                fill_rounded(&mut rgba, width, height, radius, *fill);
            }

            for row in 0..height as usize {
                for column in 0..inner.min(width - padding) as usize {
                    let src = (row * inner as usize + column) * 4;
                    let dst = (row * width as usize + column + padding as usize) * 4;
                    let pixel = RgbaColor {
                        r: text_rgba[src],
                        g: text_rgba[src + 1],
                        b: text_rgba[src + 2],
                        a: text_rgba[src + 3],
                    };
                    alpha_composite(&mut rgba[dst..dst + 4], pixel);
                }
            }

            rgba_to_yuva420p(&rgba, width, height)?
        }
        GraphicContent::Image { path } => {
            if let Some(animation) = &image_animation {
                animation.current().frame.clone()
            } else {
                let decoded = crate::gstreamer::load_image(&path.to_string_lossy())
                    .with_context(|| format!("failed to load graphic image {}", path.display()))?;
                let mut frame = crate::gstreamer::FrameConverter::new(
                    &decoded,
                    PixelFormat::Yuva420p,
                    width,
                    height,
                )?
                .run(&decoded)?;

                if radius > 0.0 {
                    let stride = frame.stride(3);
                    let alpha = frame.data_mut(3);
                    let mut mask = vec![255_u8; width as usize * height as usize * 4];
                    cut_corners(&mut mask, width, height, radius);

                    for row in 0..height as usize {
                        for column in 0..width as usize {
                            if mask[(row * width as usize + column) * 4 + 3] == 0 {
                                alpha[row * stride + column] = 0;
                            }
                        }
                    }
                }

                frame
            }
        }
    };

    Ok(Some(RenderedLayer {
        source_index: 0,
        chroma_alpha: chroma_alpha(&frame, width, height),
        frame,
        image_animation,
        scroll,
        prepared_at: Instant::now(),
        x: x as i32,
        y: y as i32,
        width,
        height,
        opacity: (layer.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        animation_in: layer.animation_in,
        animation_out: layer.animation_out,
        animation: Duration::from_millis(u64::from(layer.animation_ms)),
        delay: Duration::from_millis(u64::from(layer.delay_ms)),
    }))
}

/// Draw a graphic for a frame size. Call it off the playout thread: text and
/// images take time to draw.
pub fn render_graphic(
    spec: &GraphicSpec,
    frame_width: u32,
    frame_height: u32,
) -> Result<RenderedGraphic> {
    if frame_width < 2 || frame_height < 2 {
        return Err(anyhow!(
            "graphic frame size {frame_width}x{frame_height} is too small"
        ));
    }

    let mut layers = Vec::with_capacity(spec.layers.len());
    let mut scroll_bytes = 0;

    for (source_index, layer) in spec.layers.iter().enumerate() {
        if let Some(mut rendered) = render_layer(layer, frame_width, frame_height)? {
            rendered.source_index = source_index;
            scroll_bytes += rendered
                .scroll
                .as_ref()
                .map_or(0, super::scroll::RenderedScroll::bytes);
            ensure!(
                scroll_bytes <= 64 * 1024 * 1024,
                "scrolling text exceeds 64 MiB in this composition"
            );
            layers.push(rendered);
        }
    }
    let animation_bytes: usize = layers
        .iter()
        .filter_map(|layer| layer.image_animation.as_ref())
        .map(|animation| animation.bytes)
        .sum();
    ensure!(
        animation_bytes <= 64 * 1024 * 1024,
        "GIF frames exceed 64 MiB in this composition"
    );

    let Some(window) = spec.picture else {
        return Ok(RenderedGraphic {
            layers,
            frame_width,
            frame_height,
            picture: None,
        });
    };

    // The art is the layers drawn once, fully in, over black; the picture
    // covers the window and the art shows around it.
    let mut background = VideoFrame::new(PixelFormat::Yuv420p, frame_width, frame_height);
    background.data_mut(0).fill(16);
    background.data_mut(1).fill(128);
    background.data_mut(2).fill(128);

    for layer in &layers {
        blend_layer(
            &mut background,
            layer,
            1.0,
            GraphicAnimation::None,
            Duration::ZERO,
        );
    }

    let px = |percent: f64, total: u32| percent_px(percent, total).max(0) as u32;
    let animated = layers
        .iter()
        .any(|layer| layer.image_animation.is_some() || layer.scroll.is_some());
    let mut graphic = LShapeGraphic::from_parts(
        background,
        px(window.x, frame_width),
        px(window.y, frame_height),
        px(window.width, frame_width),
        px(window.height, frame_height),
    );

    if animated {
        graphic.set_animated_art(RenderedGraphic {
            layers,
            frame_width,
            frame_height,
            picture: None,
        });
    }

    Ok(RenderedGraphic {
        layers: Vec::new(),
        frame_width,
        frame_height,
        picture: Some(Arc::new(graphic)),
    })
}

/// Smooth start and end of a 0..1 progress.
fn ease(progress: f64) -> f64 {
    let progress = progress.clamp(0.0, 1.0);

    progress * progress * (3.0 - 2.0 * progress)
}

/// How far in a layer is (0 off, 1 in place) at `since_take`, and with
/// which animation, given when the graphic was taken out.
pub(super) fn layer_level(
    layer: &RenderedLayer,
    since_take: Duration,
    since_out: Option<Duration>,
) -> (f64, GraphicAnimation) {
    let progress = |elapsed: Duration, animation: GraphicAnimation| {
        if animation == GraphicAnimation::None || layer.animation.is_zero() {
            1.0
        } else {
            ease(elapsed.as_secs_f64() / layer.animation.as_secs_f64())
        }
    };

    if let Some(since_out) = since_out {
        let entered = if since_take >= layer.delay {
            progress(since_take - layer.delay, layer.animation_in)
        } else {
            0.0
        };
        let left = if layer.animation_out == GraphicAnimation::None {
            1.0
        } else {
            progress(since_out, layer.animation_out)
        };

        return (
            (entered * (1.0 - left)).min(1.0 - left),
            layer.animation_out,
        );
    }

    if since_take < layer.delay {
        return (0.0, layer.animation_in);
    }

    (
        progress(since_take - layer.delay, layer.animation_in),
        layer.animation_in,
    )
}

pub(super) fn blend_layer(
    target: &mut VideoFrame,
    layer: &RenderedLayer,
    level: f64,
    animation: GraphicAnimation,
    elapsed: Duration,
) {
    blend_layer_with_visibility(target, layer, level, animation, 1.0, elapsed);
}

pub(super) fn blend_layer_with_visibility(
    target: &mut VideoFrame,
    layer: &RenderedLayer,
    level: f64,
    animation: GraphicAnimation,
    visibility: f64,
    elapsed: Duration,
) {
    blend_layer_at(target, layer, level, animation, visibility, elapsed, (0, 0));
}

pub(super) fn blend_layer_at(
    target: &mut VideoFrame,
    layer: &RenderedLayer,
    level: f64,
    animation: GraphicAnimation,
    visibility: f64,
    elapsed: Duration,
    offset: (i32, i32),
) {
    if level <= 0.0 || visibility <= 0.0 {
        return;
    }

    let mut x = layer.x + offset.0;
    let mut y = layer.y + offset.1;
    let mut width = layer.width;
    let mut opacity = level;
    let away = 1.0 - level;

    let shift_x = even_down((away * f64::from(layer.width)) as i64) as i32;
    let shift_y = even_down((away * f64::from(layer.height)) as i64) as i32;

    match animation {
        GraphicAnimation::None => opacity = 1.0,
        GraphicAnimation::Fade | GraphicAnimation::Scale => {}
        GraphicAnimation::SlideLeft => x -= shift_x,
        GraphicAnimation::SlideRight => x += shift_x,
        GraphicAnimation::SlideUp => y += shift_y,
        GraphicAnimation::SlideDown => y -= shift_y,
        GraphicAnimation::Wipe => {
            width = even_down((level * f64::from(layer.width)) as i64).max(0) as u32;
            opacity = 1.0;
        }
    }

    if width < 2 {
        return;
    }

    let animated = layer
        .image_animation
        .as_ref()
        .map(|animation| animation.current());
    let (pixels, alpha) = animated.map_or((&layer.frame, &layer.chroma_alpha), |frame| {
        (&frame.frame, &frame.alpha)
    });
    let overlay = OverlayRef {
        frame: pixels,
        chroma_alpha: alpha,
        chroma_alpha_stride: layer.width as usize / 2,
        x,
        y,
        width: layer.width,
        height: layer.height,
        opacity: layer.opacity,
    };

    if let Some(scroll) = &layer.scroll {
        scroll.blend(target, overlay, width, elapsed, opacity * visibility);
    } else {
        blend_overlay(
            target,
            OverlayRef { width, ..overlay },
            opacity * visibility,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: RgbaColor = RgbaColor {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    #[test]
    fn new_shape_rasters_have_transparent_corners_correct_strokes_and_line_width() {
        let red = RgbaColor {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        };
        let ellipse = render_shape(
            GraphicShape::Ellipse,
            20,
            20,
            0.0,
            2.0,
            Some(WHITE),
            Some(red),
        );
        let pixel = |x: usize, y: usize| &ellipse[(y * 20 + x) * 4..(y * 20 + x + 1) * 4];
        assert_eq!(pixel(0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(10, 10), [255, 255, 255, 255]);
        assert_eq!(pixel(0, 10), [255, 0, 0, 255]);
        let line = render_shape(GraphicShape::Line, 20, 20, 0.0, 2.0, None, Some(red));
        assert_eq!(line[(9 * 20 + 10) * 4 + 3], 255);
        assert_eq!(line[(8 * 20 + 10) * 4 + 3], 0);
        let rounded = render_shape(GraphicShape::Rounded, 20, 20, 4.0, 0.0, Some(WHITE), None);
        assert_eq!(rounded[3], 0);
        assert_eq!(rounded[(10 * 20 + 10) * 4 + 3], 255);
    }
}
