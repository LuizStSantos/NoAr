//! Drawing the L-shape into the frames: the picture scaled into its window,
//! and how far marked stretches put a frame into it.

use anyhow::{Context, Result};

use super::shape::{LShapeGraphic, Rect};
use crate::frame::{PixelFormat, VideoFrame};

/// Scales pictures into the L-shape window, reusing the scaler while the
/// size stays the same.
#[derive(Default)]
pub(crate) struct LShapeScaler(Option<(crate::gstreamer::FrameConverter, ScaleKey)>);

/// The source's format and size, and the window's size.
type ScaleKey = (PixelFormat, u32, u32, u32, u32);

/// Draw `frame` (YUV 4:2:0, the output's size) as `level` of the way into
/// the L-shape of `graphic`. Nothing changes at level 0.
pub(crate) fn apply_lshape(
    frame: &mut VideoFrame,
    graphic: &LShapeGraphic,
    level: f64,
    scaler: &mut LShapeScaler,
) -> Result<()> {
    if level <= 0.0
        || frame.format() != PixelFormat::Yuv420p
        || frame.width() != graphic.background.width()
        || frame.height() != graphic.background.height()
    {
        return Ok(());
    }

    let rect = graphic.rect_at(level.min(1.0));
    let key = (
        frame.format(),
        frame.width(),
        frame.height(),
        rect.width,
        rect.height,
    );

    if scaler.0.as_ref().is_none_or(|(_, size)| *size != key) {
        scaler.0 = Some((
            crate::gstreamer::FrameConverter::new(
                frame,
                PixelFormat::Yuv420p,
                rect.width,
                rect.height,
            )?,
            key,
        ));
    }

    let (context, _) = scaler.0.as_mut().context("L-shape scaler")?;
    let picture = context.run(frame)?;

    let mut composed = (*graphic.background).clone();

    if let Some(art) = &graphic.animated_art {
        composed.data_mut(0).fill(16);
        composed.data_mut(1).fill(128);
        composed.data_mut(2).fill(128);
        art.blend_steady(&mut composed);
    }

    paste(&picture, &mut composed, rect);
    composed.set_pts(frame.pts());
    *frame = composed;

    Ok(())
}

/// Copy `picture` into `target` at `rect`, plane by plane.
fn paste(picture: &VideoFrame, target: &mut VideoFrame, rect: Rect) {
    for plane in 0..3 {
        let shift = u32::from(plane > 0);
        let (x, y) = ((rect.x >> shift) as usize, (rect.y >> shift) as usize);
        let width = ((rect.width >> shift) as usize)
            .min(picture.plane_width(plane) as usize)
            .min(target.plane_width(plane) as usize - x);
        let height = ((rect.height >> shift) as usize)
            .min(picture.plane_height(plane) as usize)
            .min(target.plane_height(plane) as usize - y);
        let (from_stride, to_stride) = (picture.stride(plane), target.stride(plane));
        let source = picture.data(plane);
        let destination = target.data_mut(plane);

        for row in 0..height {
            let from = row * from_stride;
            let to = (y + row) * to_stride + x;
            destination[to..to + width].copy_from_slice(&source[from..from + width]);
        }
    }
}

/// How far into the L-shape stretches marked in a clip put the frame at
/// `pts`: 1 inside one, easing over `frames` on both sides.
pub(crate) fn marked_level(spans: &[(i64, i64)], pts: i64, frames: i64) -> f64 {
    spans
        .iter()
        .filter(|&&(from, to)| pts >= from && pts <= to)
        .map(|&(from, to)| {
            // Grows in over the first frames of the stretch, back out over
            // the last ones.
            if frames <= 0 {
                1.0
            } else {
                ((pts - from).min(to - pts) as f64 / frames as f64).min(1.0)
            }
        })
        .fold(0.0, f64::max)
}
