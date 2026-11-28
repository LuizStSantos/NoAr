//! The logo picture: loading, scaling and blending it, plain or as a model.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};

use super::{
    control::LogoFrame,
    position::{logo_dimensions, logo_position},
};
use crate::{
    compositor::graphics::{
        GraphicContent, GraphicSpec, RenderedGraphic, RoleImageBlend, render_graphic,
    },
    compositor::overlay::{OverlayRef, blend_overlay, chroma_alpha},
    frame::{PixelFormat, VideoFrame},
    utils::{config::LogoConfig, helper::even},
};

#[derive(Clone)]
pub struct LogoOverlay {
    pub(super) model: Option<Arc<LogoModel>>,
    pub(super) model_offset: (i32, i32),
    image_animation: Option<Arc<crate::compositor::graphics::images::ImageAnimation>>,
    pub frame: VideoFrame, // YUVA420P
    chroma_alpha: Vec<u8>,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub opacity: u8, // 0..255
}

pub(super) struct LogoModel {
    pub(super) spec: GraphicSpec,
    pub(super) graphic: RenderedGraphic,
    pub(super) primary: usize,
    pub(super) frame_size: (u32, u32),
}

pub(super) struct LogoModelUpdate<'a> {
    pub(super) spec: Option<&'a GraphicSpec>,
    pub(super) primary: usize,
    pub(super) clip_animation_ms: Option<u32>,
    pub(super) enabled: bool,
    pub(super) size: (u32, u32),
    pub(super) animate: bool,
}

impl LogoOverlay {
    pub fn load(config: &LogoConfig, output_width: u32, output_height: u32) -> Result<Self> {
        if !(0.0..=1.0).contains(&config.opacity) || !config.opacity.is_finite() {
            return Err(anyhow!("logo opacity must be between 0.0 and 1.0"));
        }

        let decoded = crate::gstreamer::load_image(&config.path)
            .with_context(|| format!("failed to load logo {}", config.path))?;
        let (width, height) = logo_dimensions(
            config.scale.as_deref(),
            decoded.width(),
            decoded.height(),
            output_width,
            output_height,
        )?;
        let frame =
            crate::gstreamer::FrameConverter::new(&decoded, PixelFormat::Yuva420p, width, height)?
                .run(&decoded)?;
        let (x, y) = logo_position(&config.position, output_width, output_height, width, height)?;

        Ok(Self {
            model: None,
            model_offset: (0, 0),
            image_animation: crate::compositor::graphics::images::load_animation(
                std::path::Path::new(&config.path),
                width,
                height,
                0.0,
            )?,
            chroma_alpha: chroma_alpha(&frame, width, height),
            frame,
            x: even(x),
            y: even(y),
            width,
            height,
            opacity: (config.opacity * 255.0).round().clamp(0.0, 255.0) as u8,
        })
    }

    pub(super) fn from_model(
        spec: &GraphicSpec,
        primary: usize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let graphic = render_graphic(spec, width, height)?;
        let layer = spec.layers.get(primary);
        let pixels = |share: f64, total: u32| {
            even((share * f64::from(total) / 100.0).round().max(0.0) as u32)
        };
        let (x, y, w, h) = layer.map_or((0, 0, width, height), |layer| {
            (
                pixels(layer.x, width),
                pixels(layer.y, height),
                pixels(layer.width, width).max(2),
                pixels(layer.height, height).max(2),
            )
        });

        Ok(Self {
            model: Some(Arc::new(LogoModel {
                spec: spec.clone(),
                graphic,
                primary,
                frame_size: (width, height),
            })),
            model_offset: (0, 0),
            image_animation: None,
            frame: VideoFrame::new(PixelFormat::Yuva420p, 2, 2),
            chroma_alpha: vec![0],
            x,
            y,
            width: w,
            height: h,
            opacity: 255,
        })
    }

    pub(super) fn from_model_config(config: &LogoConfig, width: u32, height: u32) -> Result<Self> {
        use crate::{GraphicAnimation, GraphicLayerSpec};

        let native = Self::load(config, width, height)?;
        let percent = |value: u32, total: u32| f64::from(value) * 100.0 / f64::from(total);
        let spec = GraphicSpec {
            picture: None,
            layers: vec![GraphicLayerSpec {
                content: GraphicContent::Image {
                    path: config.path.clone().into(),
                },
                x: percent(native.x, width),
                y: percent(native.y, height),
                width: percent(native.width, width),
                height: percent(native.height, height),
                opacity: config.opacity,
                radius: 0.0,
                animation_in: GraphicAnimation::None,
                animation_out: GraphicAnimation::None,
                animation_ms: 400,
                delay_ms: 0,
            }],
        };

        Self::from_model(&spec, 0, width, height)
    }

    pub(super) fn replaced_image(&self, path: &str, width: u32, height: u32) -> Result<Self> {
        self.replaced_image_with(path, width, height, &mut Self::from_model)
    }

    pub(super) fn replaced_image_with(
        &self,
        path: &str,
        width: u32,
        height: u32,
        render: &mut impl FnMut(&GraphicSpec, usize, u32, u32) -> Result<Self>,
    ) -> Result<Self> {
        let model = self
            .model
            .as_ref()
            .ok_or_else(|| anyhow!("missing layer model"))?;
        let mut spec = model.spec.clone();
        let layer = spec
            .layers
            .get_mut(model.primary)
            .ok_or_else(|| anyhow!("missing controlled image layer"))?;
        layer.content = GraphicContent::Image { path: path.into() };

        render(&spec, model.primary, width, height)
    }

    pub(super) fn inherit_motion(&mut self, previous: &Self) {
        if let (Some(next), Some(previous)) =
            (self.model.as_mut().and_then(Arc::get_mut), &previous.model)
        {
            next.graphic.inherit_motion(&previous.graphic);
        }
    }

    pub(crate) fn is_model(&self) -> bool {
        self.model.is_some()
    }

    pub(crate) fn blend_model(
        &self,
        frame: &mut VideoFrame,
        timing: &LogoFrame,
        mask: f64,
        image_factor: f64,
        primary_only: bool,
    ) {
        if let Some(model) = &self.model {
            let levels = model.graphic.role_levels(
                timing.model_elapsed,
                timing.model_hidden,
                &timing.model_from,
            );
            model.graphic.blend_role(
                frame,
                &levels,
                timing.model_hidden,
                mask,
                RoleImageBlend {
                    index: model.primary,
                    opacity: image_factor,
                    only: primary_only,
                    offset: self.model_offset,
                },
            );
        }
    }

    /// The same image placed at `position` (an `X:Y` expression) in an
    /// output of `width` x `height`.
    pub(crate) fn moved_to(&self, position: &str, width: u32, height: u32) -> Result<Self> {
        let (x, y) = logo_position(position, width, height, self.width, self.height)?;

        let mut moved = self.clone();

        // Keep the prepared model (including GIF caches, text strips and clocks)
        // shared. This path also runs on the output thread, so only translate
        // its blend coordinates; never rasterize or open an image here.
        if self.model.is_some() {
            moved.model_offset.0 += even(x) as i32 - self.x as i32;
            moved.model_offset.1 += even(y) as i32 - self.y as i32;
        }

        moved.x = even(x);
        moved.y = even(y);

        Ok(moved)
    }

    fn as_overlay(&self) -> OverlayRef<'_> {
        let animated = self
            .image_animation
            .as_ref()
            .map(|animation| animation.current());
        let (frame, alpha) = animated.map_or((&self.frame, &self.chroma_alpha), |image| {
            (&image.frame, &image.alpha)
        });

        OverlayRef {
            frame,
            chroma_alpha: alpha,
            chroma_alpha_stride: self.width as usize / 2,
            x: self.x as i32,
            y: self.y as i32,
            width: self.width,
            height: self.height,
            opacity: self.opacity,
        }
    }
}

pub fn blend_logo(target: &mut VideoFrame, logo: &LogoOverlay, opacity_factor: f64) {
    if logo.is_model() {
        logo.blend_model(target, &LogoFrame::default(), opacity_factor, 1.0, false);
        return;
    }
    blend_overlay(target, logo.as_overlay(), opacity_factor);
}
