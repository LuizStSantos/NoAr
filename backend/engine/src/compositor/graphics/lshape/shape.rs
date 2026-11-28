//! The L-shape's timing, corners and configuration, and the graphic as
//! drawn with the window the picture takes in it.

use std::{fmt, sync::Arc};

use anyhow::{Context, Result, anyhow};

use super::control::LShapePlacement;
use crate::{
    compositor::graphics::{GraphicContent, GraphicSpec, RenderedGraphic, render_graphic},
    frame::{PixelFormat, VideoFrame},
    utils::helper::even,
};

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
#[derive(Clone)]
pub struct LShapeGraphic {
    pub(super) background: Arc<VideoFrame>,
    pub(super) animated_art: Option<Arc<RenderedGraphic>>,
    pub(super) window: Rect,
    /// The window the graphic was made for, before any move from outside.
    pub(super) home: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl LShapeGraphic {
    pub(in crate::compositor::graphics) fn inherit_motion(&mut self, previous: &Self) {
        if let (Some(next), Some(previous)) = (
            self.animated_art.as_mut().and_then(Arc::get_mut),
            &previous.animated_art,
        ) {
            next.inherit_motion(previous);
        }
    }

    /// A graphic from a background already at the output's size and the
    /// window the picture takes in it, in pixels.
    pub(crate) fn from_parts(
        background: VideoFrame,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Self {
        let frame_width = background.width();
        let frame_height = background.height();
        let x = even(x.min(frame_width.saturating_sub(2)));
        let y = even(y.min(frame_height.saturating_sub(2)));
        let window = Rect {
            x,
            y,
            width: even(width.min(frame_width - x)).max(2),
            height: even(height.min(frame_height - y)).max(2),
        };

        Self {
            background: Arc::new(background),
            animated_art: None,
            window,
            home: window,
        }
    }

    pub(crate) fn set_animated_art(&mut self, graphic: RenderedGraphic) {
        self.animated_art = Some(Arc::new(graphic));
    }

    pub fn load(config: &LShapeConfig, width: u32, height: u32) -> Result<Self> {
        if !(LSHAPE_MIN_SCALE..=LSHAPE_MAX_SCALE).contains(&config.scale) {
            return Err(anyhow!(
                "the picture in the L-shape must be between {LSHAPE_MIN_SCALE} and {LSHAPE_MAX_SCALE}% of the screen"
            ));
        }

        let decoded = crate::gstreamer::load_image(&config.path)
            .with_context(|| format!("cannot load the L-shape graphic {}", config.path))?;
        let background =
            crate::gstreamer::FrameConverter::new(&decoded, PixelFormat::Yuv420p, width, height)?
                .run(&decoded)?;

        let home = window(config, width, height);

        let spec = GraphicSpec {
            layers: vec![crate::GraphicLayerSpec {
                content: GraphicContent::Image {
                    path: config.path.clone().into(),
                },
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                opacity: 1.0,
                radius: 0.0,
                animation_in: crate::GraphicAnimation::None,
                animation_out: crate::GraphicAnimation::None,
                animation_ms: 0,
                delay_ms: 0,
            }],
            picture: None,
        };
        let animated_art = if std::path::Path::new(&config.path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
        {
            let rendered = render_graphic(&spec, width, height)?;
            rendered.is_animated().then(|| Arc::new(rendered))
        } else {
            None
        };

        Ok(Self {
            background: Arc::new(background),
            animated_art,
            window: home,
            home,
        })
    }

    /// The same graphic with the picture placed elsewhere, or back home.
    pub(super) fn placed(&self, placement: Option<LShapePlacement>) -> Self {
        let (width, height) = (self.background.width(), self.background.height());
        let window = placement.map_or(self.home, |placement| {
            let scale = placement
                .scale
                .unwrap_or_else(|| f64::from(self.home.width) * 100.0 / f64::from(width.max(1)));

            window_in(placement.corner, scale, width, height)
        });

        Self {
            background: self.background.clone(),
            animated_art: self.animated_art.clone(),
            window,
            home: self.home,
        }
    }

    /// Where the picture is when the L-shape is `level` of the way in (0 is
    /// the full screen, 1 the window), easing in and out.
    pub(super) fn rect_at(&self, level: f64) -> Rect {
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

pub(super) fn window(config: &LShapeConfig, width: u32, height: u32) -> Rect {
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
