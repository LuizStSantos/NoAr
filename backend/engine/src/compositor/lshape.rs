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
use ffmpeg_next::{frame, util::format::pixel::Pixel};

use crate::utils::helper::even;

/// How long the picture takes to shrink or grow back.
pub const LSHAPE_SECONDS: f64 = 0.8;

/// How long the logo takes to go off before the picture shrinks, and to come
/// back after it grew back.
pub const LSHAPE_LOGO_SECONDS: f64 = 0.4;

/// A whole switch: the logo off, then the picture in (or the other way).
pub const LSHAPE_SWITCH_SECONDS: f64 = LSHAPE_LOGO_SECONDS + LSHAPE_SECONDS;

/// How far a switch `progress` (0 to 1) is, as the logo's share gone (0 to 1)
/// and the picture's way into the L-shape (0 to 1): the logo goes off first,
/// then the picture shrinks; coming out, the picture grows back first, then
/// the logo returns.
pub fn lshape_phases(progress: f64) -> (f64, f64) {
    let progress = progress.clamp(0.0, 1.0);
    let logo_share = LSHAPE_LOGO_SECONDS / LSHAPE_SWITCH_SECONDS;

    (
        (progress / logo_share).min(1.0),
        ((progress - logo_share) / (1.0 - logo_share)).max(0.0),
    )
}

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
        match value.trim() {
            "top-right" => Some(Self::TopRight),
            "top-left" => Some(Self::TopLeft),
            "bottom-right" => Some(Self::BottomRight),
            "bottom-left" => Some(Self::BottomLeft),
            _ => None,
        }
    }
}

impl fmt::Display for LShapeCorner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TopRight => f.write_str("top-right"),
            Self::TopLeft => f.write_str("top-left"),
            Self::BottomRight => f.write_str("bottom-right"),
            Self::BottomLeft => f.write_str("bottom-left"),
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
    background: Arc<frame::Video>,
    window: Rect,
    /// The window the graphic was made for, before any move from outside.
    home: Rect,
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

        let decoded = crate::gstreamer::load_image(&config.path)
            .with_context(|| format!("cannot load the L-shape graphic {}", config.path))?;
        let background =
            crate::gstreamer::FrameConverter::new(&decoded, Pixel::YUV420P, width, height)?
                .run(&decoded)?;

        let home = window(config, width, height);

        Ok(Self {
            background: Arc::new(background),
            window: home,
            home,
        })
    }

    /// The same graphic with the picture placed elsewhere, or back home.
    fn placed(&self, placement: Option<LShapePlacement>) -> Self {
        let (width, height) = (self.background.width(), self.background.height());
        let window = placement.map_or(self.home, |placement| {
            let scale = placement
                .scale
                .unwrap_or_else(|| f64::from(self.home.width) * 100.0 / f64::from(width.max(1)));

            window_in(placement.corner, scale, width, height)
        });

        Self {
            background: self.background.clone(),
            window,
            home: self.home,
        }
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
    window_in(config.corner, config.scale, width, height)
}

fn window_in(corner: LShapeCorner, scale: f64, width: u32, height: u32) -> Rect {
    let share = scale.clamp(LSHAPE_MIN_SCALE, LSHAPE_MAX_SCALE) / 100.0;
    let window_width = even((f64::from(width) * share).round() as u32).max(2);
    let window_height = even((f64::from(height) * share).round() as u32).max(2);
    let right = width - window_width;
    let bottom = height - window_height;
    let (x, y) = match corner {
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

/// Where the picture goes when moved from outside, over any graphic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LShapePlacement {
    pub corner: LShapeCorner,
    /// Size of the picture, in percent of the screen; `None` keeps the
    /// graphic's.
    pub scale: Option<f64>,
}

/// Switches the L-shape on air from outside the playout. Clones share the
/// same state; it applies to clips, lives and ingest alike, and outlasts a
/// playout restart.
#[derive(Clone, Default)]
pub struct LShapeControl(Arc<Mutex<LShapeState>>);

#[derive(Default)]
struct LShapeState {
    /// The channel's graphic.
    graphic: Option<Arc<LShapeGraphic>>,
    /// Another graphic swapped in from outside, and its name.
    replacement: Option<(String, Arc<LShapeGraphic>)>,
    /// The graphic of the clip playing, for its marked stretches.
    clip: Option<(LShapeConfig, Arc<LShapeGraphic>)>,
    /// Where the picture was moved to from outside; it applies to the
    /// channel's graphic and the one swapped in, not to a clip's own.
    placement: Option<LShapePlacement>,
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
                let progress =
                    now.saturating_duration_since(at).as_secs_f64() / LSHAPE_SWITCH_SECONDS;

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
            .transpose()?;
        let mut state = self.lock();
        let placement = state.placement;

        state.graphic = graphic.map(|graphic| Arc::new(graphic.placed(placement)));

        Ok(())
    }

    /// Use the graphic of `config` in place of the channel's, from now on
    /// (also while it is on), until [`Self::reset`]. `label` names it in the
    /// state.
    pub fn replace(
        &self,
        label: &str,
        config: &LShapeConfig,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let graphic = LShapeGraphic::load(config, width, height)?;
        let mut state = self.lock();
        let placement = state.placement;

        state.replacement = Some((label.to_string(), Arc::new(graphic.placed(placement))));

        Ok(())
    }

    /// Put the picture somewhere else in the L-shape, on or off, until moved
    /// again; `None` puts it back where each graphic places it.
    pub fn place(&self, placement: Option<LShapePlacement>) -> Result<()> {
        if let Some(scale) = placement.and_then(|placement| placement.scale)
            && !(LSHAPE_MIN_SCALE..=LSHAPE_MAX_SCALE).contains(&scale)
        {
            return Err(anyhow!(
                "the picture in the L-shape must be between {LSHAPE_MIN_SCALE} and {LSHAPE_MAX_SCALE}% of the screen"
            ));
        }

        let mut state = self.lock();
        state.placement = placement;
        state.graphic = state
            .graphic
            .as_ref()
            .map(|graphic| Arc::new(graphic.placed(placement)));

        if let Some((label, graphic)) = state.replacement.take() {
            state.replacement = Some((label, Arc::new(graphic.placed(placement))));
        }

        Ok(())
    }

    /// Where the picture was moved to from outside, if it was.
    pub fn placement(&self) -> Option<LShapePlacement> {
        self.lock().placement
    }

    /// Back to the channel's graphic.
    pub fn reset(&self) {
        self.lock().replacement = None;
    }

    /// The graphic of the clip about to play, for the stretches marked in
    /// it, or none to use the channel's. Loaded once while the same clip
    /// graphic follows.
    pub fn set_clip_graphic(
        &self,
        config: Option<&LShapeConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let Some(config) = config else {
            self.lock().clip = None;
            return Ok(());
        };

        if self
            .lock()
            .clip
            .as_ref()
            .is_some_and(|(loaded, _)| loaded == config)
        {
            return Ok(());
        }

        let graphic = LShapeGraphic::load(config, width, height);
        let mut state = self.lock();

        match graphic {
            Ok(graphic) => {
                state.clip = Some((config.clone(), Arc::new(graphic)));
                Ok(())
            }
            Err(error) => {
                state.clip = None;
                Err(error)
            }
        }
    }

    /// The name of the graphic swapped in from outside, when there is one.
    pub fn replacement(&self) -> Option<String> {
        self.lock()
            .replacement
            .as_ref()
            .map(|(label, _)| label.clone())
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

    /// Whether there is a graphic to show when switched on.
    pub fn is_ready(&self) -> bool {
        let state = self.lock();

        state.graphic.is_some() || state.replacement.is_some()
    }

    /// For the frame being drawn, with the clip's marks putting it `marked`
    /// of the way in: the graphic, and how far in the L-shape is, between 0
    /// (full screen) and 1. A marked stretch uses the clip's own graphic
    /// when it has one; otherwise the one swapped in, else the channel's.
    pub(crate) fn frame(&self, marked: f64) -> Option<(Arc<LShapeGraphic>, f64)> {
        let state = self.lock();
        let switched = state.level(Instant::now());
        let current = state
            .replacement
            .as_ref()
            .map(|(_, graphic)| graphic.clone())
            .or_else(|| state.graphic.clone());

        if marked > switched
            && let Some((_, clip)) = &state.clip
        {
            return Some((clip.clone(), marked));
        }

        current.map(|graphic| (graphic, switched.max(marked)))
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
pub(crate) struct LShapeScaler(Option<(crate::gstreamer::FrameConverter, ScaleKey)>);

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
            crate::gstreamer::FrameConverter::new(frame, Pixel::YUV420P, rect.width, rect.height)?,
            key,
        ));
    }

    let (context, _) = scaler.0.as_mut().context("L-shape scaler")?;
    let picture = context.run(frame)?;

    let mut composed = (*graphic.background).clone();
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

        let home = window(&config, 1920, 1080);

        LShapeGraphic {
            background: Arc::new(background),
            window: home,
            home,
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
    fn a_corner_reads_back_as_its_name() {
        for corner in [
            LShapeCorner::TopRight,
            LShapeCorner::TopLeft,
            LShapeCorner::BottomRight,
            LShapeCorner::BottomLeft,
        ] {
            assert_eq!(LShapeCorner::parse(&corner.to_string()), Some(corner));
        }
        assert_eq!(LShapeCorner::parse("50%:50%"), None);
    }

    #[test]
    fn a_move_from_outside_places_every_graphic_until_it_is_undone() {
        let control = LShapeControl::default();
        control.lock().graphic = Some(Arc::new(graphic(LShapeCorner::TopRight)));
        let window = |control: &LShapeControl| control.frame(1.0).unwrap().0.window;
        let home = window(&control);

        control
            .place(Some(LShapePlacement {
                corner: LShapeCorner::BottomLeft,
                scale: Some(50.0),
            }))
            .unwrap();
        assert_eq!(
            window(&control),
            Rect {
                x: 0,
                y: 540,
                width: 960,
                height: 540
            }
        );

        // The size stays the graphic's when only the corner is given.
        control
            .place(Some(LShapePlacement {
                corner: LShapeCorner::TopLeft,
                scale: None,
            }))
            .unwrap();
        assert_eq!(window(&control).width, home.width);
        assert_eq!((window(&control).x, window(&control).y), (0, 0));

        assert!(
            control
                .place(Some(LShapePlacement {
                    corner: LShapeCorner::TopLeft,
                    scale: Some(10.0),
                }))
                .is_err()
        );

        control.place(None).unwrap();
        assert_eq!(window(&control), home);
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
            (control.level_after(Duration::from_secs_f64(LSHAPE_SWITCH_SECONDS / 2.0)) - 0.5).abs()
                < 0.01
        );
        assert_eq!(control.level_after(Duration::from_secs(2)), 1.0);

        control.lock().switched_at = None;
        control.off();
        assert_eq!(control.level_after(Duration::ZERO), 1.0);
        assert_eq!(control.level_after(Duration::from_secs(2)), 0.0);
        // No graphic: nothing to draw.
        assert!(control.frame(1.0).is_none());
    }

    #[test]
    fn a_swapped_or_clip_graphic_takes_the_channels_place() {
        let control = LShapeControl::default();
        let channel = Arc::new(graphic(LShapeCorner::TopRight));
        let other = Arc::new(graphic(LShapeCorner::BottomLeft));
        let clip = Arc::new(graphic(LShapeCorner::TopLeft));
        control.lock().graphic = Some(channel.clone());

        let (shown, level) = control.frame(0.0).unwrap();
        assert!(Arc::ptr_eq(&shown, &channel));
        assert_eq!(level, 0.0);

        control.lock().replacement = Some(("sponsor.png".to_string(), other.clone()));
        assert_eq!(control.replacement().as_deref(), Some("sponsor.png"));
        assert!(Arc::ptr_eq(&control.frame(0.0).unwrap().0, &other));

        // A marked stretch uses the clip's graphic.
        let config = LShapeConfig {
            path: String::new(),
            scale: 75.0,
            corner: LShapeCorner::TopLeft,
        };
        control.lock().clip = Some((config, clip.clone()));
        let (shown, level) = control.frame(0.6).unwrap();
        assert!(Arc::ptr_eq(&shown, &clip));
        assert_eq!(level, 0.6);
        // Without marks, the swapped one.
        assert!(Arc::ptr_eq(&control.frame(0.0).unwrap().0, &other));

        control.reset();
        assert!(control.replacement().is_none());
        control.set_clip_graphic(None, 1920, 1080).unwrap();
        assert!(Arc::ptr_eq(&control.frame(0.6).unwrap().0, &channel));
        assert!(
            control
                .set_clip_graphic(
                    Some(&LShapeConfig {
                        path: "/missing.png".to_string(),
                        scale: 75.0,
                        corner: LShapeCorner::TopLeft,
                    }),
                    1920,
                    1080
                )
                .is_err()
        );
    }

    #[test]
    fn the_logo_goes_off_before_the_picture_shrinks() {
        assert_eq!(lshape_phases(0.0), (0.0, 0.0));
        // A third of the way: the logo is gone, the picture not moved yet.
        let (logo, picture) = lshape_phases(LSHAPE_LOGO_SECONDS / LSHAPE_SWITCH_SECONDS);
        assert!((logo - 1.0).abs() < 1e-9 && picture.abs() < 1e-9);
        let (logo, picture) = lshape_phases(0.2);
        assert!(logo > 0.5 && logo < 1.0 && picture == 0.0);
        let (logo, picture) = lshape_phases(0.75);
        assert!((logo - 1.0).abs() < 1e-9 && picture > 0.5 && picture < 1.0);
        assert_eq!(lshape_phases(1.0), (1.0, 1.0));
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
