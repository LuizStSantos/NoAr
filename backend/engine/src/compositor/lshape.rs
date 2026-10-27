//! The L-shape: the picture shrinks into a corner of the screen, over a
//! full-screen graphic that fills the band around it (news, sponsors, the
//! next programme), and grows back when it ends. It is switched from outside
//! the playout, like the logo, or by stretches marked in the video itself.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use anyhow::{Context, Result, anyhow};
use ffmpeg_next::{codec, frame, media, software::scaling, util::format::pixel::Pixel};

use crate::utils::helper::{even, open_media_input};

/// How long the picture takes to shrink or grow back.
pub const LSHAPE_SECONDS: f64 = 0.8;

/// The smallest and largest the picture gets, in percent of the screen.
pub const LSHAPE_MIN_SCALE: f64 = 30.0;
pub const LSHAPE_MAX_SCALE: f64 = 95.0;

/// The corner the picture shrinks into.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LShapeCorner {
    #[default]
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl LShapeCorner {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "top-right" => Some(Self::TopRight),
            "top-left" => Some(Self::TopLeft),
            "bottom-right" => Some(Self::BottomRight),
            "bottom-left" => Some(Self::BottomLeft),
            _ => None,
        }
    }
}

/// The graphic and where the picture goes in it.
#[derive(Debug, Clone, PartialEq)]
pub struct LShapeConfig {
    /// A full-screen image, sized to the output; its band frames the picture.
    pub path: String,
    /// Size of the picture in the L-shape, in percent of the screen.
    pub scale: f64,
    pub corner: LShapeCorner,
}

/// The graphic as drawn: the background at the output's size, and the
/// window the picture takes in it.
pub struct LShapeGraphic {
    background: frame::Video,
    window: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl LShapeGraphic {
    pub fn load(config: &LShapeConfig, width: u32, height: u32) -> Result<Self> {
        if !(LSHAPE_MIN_SCALE..=LSHAPE_MAX_SCALE).contains(&config.scale) {
            return Err(anyhow!(
                "the picture in the L-shape must be between {LSHAPE_MIN_SCALE} and {LSHAPE_MAX_SCALE}% of the screen"
            ));
        }

        let mut ictx = open_media_input(&config.path)
            .with_context(|| format!("cannot open the L-shape graphic {}", config.path))?;
        let stream = ictx
            .streams()
            .best(media::Type::Video)
            .ok_or_else(|| anyhow!("the L-shape graphic {} has no image", config.path))?;
        let index = stream.index();
        let mut decoder = codec::context::Context::from_parameters(stream.parameters())?
            .decoder()
            .video()?;
        let mut decoded = frame::Video::empty();
        let mut found = false;

        for (stream, packet) in ictx.packets() {
            if stream.index() == index
                && decoder.send_packet(&packet).is_ok()
                && decoder.receive_frame(&mut decoded).is_ok()
            {
                found = true;
                break;
            }
        }

        if !found {
            decoder.send_eof()?;
            found = decoder.receive_frame(&mut decoded).is_ok();
        }

        if !found {
            return Err(anyhow!("the L-shape graphic {} has no image", config.path));
        }

        // Stretched to the screen: the graphic is made for it.
        let mut background = frame::Video::empty();
        scaling::Context::get(
            decoded.format(),
            decoded.width(),
            decoded.height(),
            Pixel::YUV420P,
            width,
            height,
            scaling::flag::Flags::BICUBIC,
        )?
        .run(&decoded, &mut background)?;

        Ok(Self {
            background,
            window: window(config, width, height),
        })
    }

    /// Where the picture is when the L-shape is `level` of the way in (0 is
    /// the full screen, 1 the window), easing in and out.
    fn rect_at(&self, level: f64) -> Rect {
        let eased = level * level * (3.0 - 2.0 * level);
        let (width, height) = (self.background.width(), self.background.height());
        let mix = |full: u32, window: u32| {
            (f64::from(full) + (f64::from(window) - f64::from(full)) * eased).round() as u32
        };

        Rect {
            x: even(mix(0, self.window.x)),
            y: even(mix(0, self.window.y)),
            width: even(mix(width, self.window.width)).max(2),
            height: even(mix(height, self.window.height)).max(2),
        }
    }
}

fn window(config: &LShapeConfig, width: u32, height: u32) -> Rect {
    let share = config.scale / 100.0;
    let window_width = even((f64::from(width) * share).round() as u32).max(2);
    let window_height = even((f64::from(height) * share).round() as u32).max(2);
    let right = width - window_width;
    let bottom = height - window_height;
    let (x, y) = match config.corner {
        LShapeCorner::TopRight => (right, 0),
        LShapeCorner::TopLeft => (0, 0),
        LShapeCorner::BottomRight => (right, bottom),
        LShapeCorner::BottomLeft => (0, bottom),
    };

    Rect {
        x: even(x),
        y: even(y),
        width: window_width,
        height: window_height,
    }
}

/// Switches the L-shape on air from outside the playout. Clones share the
/// same state; it applies to clips, lives and ingest alike, and outlasts a
/// playout restart.
#[derive(Clone, Default)]
pub struct LShapeControl(Arc<Mutex<LShapeState>>);

#[derive(Default)]
struct LShapeState {
    graphic: Option<Arc<LShapeGraphic>>,
    on: bool,
    /// Level when the last switch began, and when that was.
    from: f64,
    switched_at: Option<Instant>,
}

impl LShapeState {
    fn level(&self, now: Instant) -> f64 {
        let target = if self.on { 1.0 } else { 0.0 };

        match self.switched_at {
            Some(at) => {
                let progress = now.saturating_duration_since(at).as_secs_f64() / LSHAPE_SECONDS;

                self.from + (target - self.from) * progress.min(1.0)
            }
            None => target,
        }
    }

    fn set_on(&mut self, on: bool) {
        if on != self.on {
            let now = Instant::now();

            self.from = self.level(now);
            self.on = on;
            self.switched_at = Some(now);
        }
    }
}

impl fmt::Debug for LShapeControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LShapeControl")
            .field("on", &self.is_on())
            .field("ready", &self.is_ready())
            .finish()
    }
}

impl LShapeControl {
    fn lock(&self) -> MutexGuard<'_, LShapeState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Use the graphic of `config` for an output of `width` x `height`, or
    /// none: without a graphic the L-shape never shows.
    pub fn set_graphic(
        &self,
        config: Option<&LShapeConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let graphic = config
            .map(|config| LShapeGraphic::load(config, width, height))
            .transpose()?
            .map(Arc::new);

        self.lock().graphic = graphic;

        Ok(())
    }

    /// Shrink the picture into the L-shape.
    pub fn on(&self) {
        self.lock().set_on(true);
    }

    /// Bring the picture back to the full screen.
    pub fn off(&self) {
        self.lock().set_on(false);
    }

    /// Switched on from outside (a marked stretch also shows it).
    pub fn is_on(&self) -> bool {
        self.lock().on
    }

    /// Whether there is a graphic to show.
    pub fn is_ready(&self) -> bool {
        self.lock().graphic.is_some()
    }

    /// For the frame being drawn: the graphic, and how far in the L-shape
    /// is from outside, between 0 (full screen) and 1.
    pub(crate) fn frame(&self) -> Option<(Arc<LShapeGraphic>, f64)> {
        let state = self.lock();

        state
            .graphic
            .clone()
            .map(|graphic| (graphic, state.level(Instant::now())))
    }

    #[cfg(test)]
    fn level_after(&self, elapsed: std::time::Duration) -> f64 {
        let state = self.lock();
        let at = state.switched_at.unwrap_or_else(Instant::now);

        state.level(at + elapsed)
    }
}

/// Scales pictures into the L-shape window, reusing the scaler while the
/// size stays the same.
#[derive(Default)]
pub(crate) struct LShapeScaler(Option<(scaling::Context, ScaleKey)>);

/// The source's format and size, and the window's size.
type ScaleKey = (Pixel, u32, u32, u32, u32);

/// Draw `frame` (YUV 4:2:0, the output's size) as `level` of the way into
/// the L-shape of `graphic`. Nothing changes at level 0.
pub(crate) fn apply_lshape(
    frame: &mut frame::Video,
    graphic: &LShapeGraphic,
    level: f64,
    scaler: &mut LShapeScaler,
) -> Result<()> {
    if level <= 0.0
        || frame.format() != Pixel::YUV420P
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
            scaling::Context::get(
                key.0,
                key.1,
                key.2,
                Pixel::YUV420P,
                rect.width,
                rect.height,
                scaling::flag::Flags::BILINEAR,
            )?,
            key,
        ));
    }

    let (context, _) = scaler.0.as_mut().context("L-shape scaler")?;
    let mut picture = frame::Video::empty();
    context.run(frame, &mut picture)?;

    let mut composed = graphic.background.clone();
    paste(&picture, &mut composed, rect);
    composed.set_pts(frame.pts());
    *frame = composed;

    Ok(())
}

/// Copy `picture` into `target` at `rect`, plane by plane.
fn paste(picture: &frame::Video, target: &mut frame::Video, rect: Rect) {
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn graphic(corner: LShapeCorner) -> LShapeGraphic {
        let mut background = frame::Video::new(Pixel::YUV420P, 1920, 1080);
        for plane in 0..3 {
            background
                .data_mut(plane)
                .fill(if plane == 0 { 200 } else { 128 });
        }
        let config = LShapeConfig {
            path: String::new(),
            scale: 75.0,
            corner,
        };

        LShapeGraphic {
            background,
            window: window(&config, 1920, 1080),
        }
    }

    #[test]
    fn the_window_sits_in_its_corner() {
        let config = |corner| LShapeConfig {
            path: String::new(),
            scale: 75.0,
            corner,
        };

        assert_eq!(
            window(&config(LShapeCorner::TopRight), 1920, 1080),
            Rect {
                x: 480,
                y: 0,
                width: 1440,
                height: 810
            }
        );
        assert_eq!(
            window(&config(LShapeCorner::BottomLeft), 1920, 1080),
            Rect {
                x: 0,
                y: 270,
                width: 1440,
                height: 810
            }
        );
        assert_eq!(
            LShapeCorner::parse("bottom-right"),
            Some(LShapeCorner::BottomRight)
        );
        assert_eq!(LShapeCorner::parse("middle"), None);
    }

    #[test]
    fn the_picture_moves_from_the_full_screen_to_the_window() {
        let graphic = graphic(LShapeCorner::TopRight);

        assert_eq!(
            graphic.rect_at(0.0),
            Rect {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(graphic.rect_at(1.0), graphic.window);
        let half = graphic.rect_at(0.5);
        assert!(half.width < 1920 && half.width > 1440, "{half:?}");
        assert_eq!(half.y, 0);
    }

    #[test]
    fn the_frame_is_drawn_into_the_window_over_the_graphic() {
        let graphic = graphic(LShapeCorner::TopRight);
        let mut frame = frame::Video::new(Pixel::YUV420P, 1920, 1080);
        for plane in 0..3 {
            frame
                .data_mut(plane)
                .fill(if plane == 0 { 50 } else { 128 });
        }
        frame.set_pts(Some(7));
        let mut scaler = LShapeScaler::default();

        apply_lshape(&mut frame, &graphic, 1.0, &mut scaler).unwrap();

        let luma = |x: usize, y: usize| frame.data(0)[y * frame.stride(0) + x];
        // The band shows the graphic, the window the picture.
        assert_eq!(luma(100, 500), 200);
        assert_eq!(luma(100, 1000), 200);
        assert_eq!(luma(1000, 400), 50);
        assert_eq!(frame.pts(), Some(7));

        // Off: untouched.
        let mut plain = frame::Video::new(Pixel::YUV420P, 1920, 1080);
        plain.data_mut(0).fill(50);
        apply_lshape(&mut plain, &graphic, 0.0, &mut scaler).unwrap();
        assert_eq!(plain.data(0)[0], 50);
    }

    #[test]
    fn switching_animates_both_ways() {
        let control = LShapeControl::default();

        control.on();
        assert!(control.is_on());
        assert!(control.level_after(Duration::ZERO) < 0.01);
        assert!(
            (control.level_after(Duration::from_secs_f64(LSHAPE_SECONDS / 2.0)) - 0.5).abs() < 0.01
        );
        assert_eq!(control.level_after(Duration::from_secs(2)), 1.0);

        control.lock().switched_at = None;
        control.off();
        assert_eq!(control.level_after(Duration::ZERO), 1.0);
        assert_eq!(control.level_after(Duration::from_secs(2)), 0.0);
        // No graphic: nothing to draw.
        assert!(control.frame().is_none());
    }

    #[test]
    fn marked_stretches_ease_in_and_out() {
        let spans = [(100, 200)];

        assert_eq!(marked_level(&spans, 50, 20), 0.0);
        assert_eq!(marked_level(&spans, 100, 20), 0.0);
        assert!((marked_level(&spans, 110, 20) - 0.5).abs() < 0.01);
        assert_eq!(marked_level(&spans, 150, 20), 1.0);
        assert!((marked_level(&spans, 190, 20) - 0.5).abs() < 0.01);
        assert_eq!(marked_level(&spans, 201, 20), 0.0);
        assert_eq!(marked_level(&spans, 150, 0), 1.0);
    }
}
