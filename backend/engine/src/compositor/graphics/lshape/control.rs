//! What L-shape is on air, switched from outside the playout, and its
//! bound model.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Instant,
};

use anyhow::{Result, anyhow};

use super::shape::{
    LSHAPE_MAX_SCALE, LSHAPE_MIN_SCALE, LSHAPE_SWITCH_SECONDS, LShapeConfig, LShapeCorner,
    LShapeGraphic, window,
};
use crate::compositor::graphics::{GraphicContent, GraphicSpec, render_graphic};

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
    model_generation: u64,
    model: Option<(GraphicSpec, usize)>,
    replacement_image: Option<String>,
    replacement_uses_model: bool,
    clip_uses_model: bool,
    clip_inherits_window: bool,
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
    fn begin_model_update(&self) -> u64 {
        let mut state = self.lock();
        state.model_generation = state.model_generation.wrapping_add(1);

        state.model_generation
    }

    /// Replace the channel model without changing its on/off phase, remote
    /// replacement, placement or clip override.
    pub(crate) fn set_rendered_graphic(
        &self,
        graphic: Option<Arc<LShapeGraphic>>,
        spec: &GraphicSpec,
        primary: usize,
        generation: u64,
    ) -> Result<()> {
        self.set_rendered_graphic_with(graphic, spec, primary, generation, render_model_art)
    }

    fn set_rendered_graphic_with(
        &self,
        graphic: Option<Arc<LShapeGraphic>>,
        spec: &GraphicSpec,
        primary: usize,
        generation: u64,
        render: impl FnMut(&(GraphicSpec, usize), &str, u32, u32) -> Result<LShapeGraphic>,
    ) -> Result<()> {
        self.set_rendered_graphic_guarded(graphic, (spec, primary), generation, render, || Some(()))
    }

    fn set_rendered_graphic_guarded<G>(
        &self,
        mut graphic: Option<Arc<LShapeGraphic>>,
        definition: (&GraphicSpec, usize),
        generation: u64,
        mut render: impl FnMut(&(GraphicSpec, usize), &str, u32, u32) -> Result<LShapeGraphic>,
        mut before_commit: impl FnMut() -> Option<G>,
    ) -> Result<()> {
        let model = (definition.0.clone(), definition.1);

        loop {
            let (
                previous,
                replacement,
                replacement_image,
                clip,
                use_replacement,
                use_clip,
                inherit_window,
            ) = {
                let state = self.lock();

                if state.model_generation != generation {
                    return Ok(());
                }

                (
                    state.graphic.clone(),
                    state.replacement.clone(),
                    state.replacement_image.clone(),
                    state.clip.clone(),
                    state.replacement_uses_model,
                    state.clip_uses_model,
                    state.clip_inherits_window,
                )
            };

            if let (Some(next), Some(previous)) =
                (graphic.as_mut().and_then(Arc::get_mut), &previous)
            {
                next.inherit_motion(previous);
            }

            let mut next_replacement = None;
            let mut next_clip = None;

            if let Some(graphic) = &graphic {
                let (width, height) = (graphic.background.width(), graphic.background.height());

                // Preparing overrides can decode images; frames must remain able
                // to snapshot the current artwork throughout that work.
                if use_replacement
                    && let Some(path) = &replacement_image
                    && let Some((label, old)) = &replacement
                {
                    let mut next = render(&model, path, width, height)?;
                    next.inherit_motion(old);
                    next_replacement = Some((label.clone(), next));
                }

                if use_clip && let Some((config, old)) = &clip {
                    let mut next = render(&model, &config.path, width, height)?;
                    next.inherit_motion(old);

                    if !inherit_window {
                        next.home = old.home;
                        next.window = old.home;
                    }

                    next_clip = Some((config.clone(), Arc::new(next)));
                }
            }

            let Some(_publication) = before_commit() else {
                return Ok(());
            };
            let mut state = self.lock();

            if state.model_generation != generation {
                return Ok(());
            }

            if !super::super::same_prepared(
                state.replacement.as_ref().map(|(_, image)| image),
                replacement.as_ref().map(|(_, image)| image),
            ) || !super::super::same_prepared(
                state.clip.as_ref().map(|(_, image)| image),
                clip.as_ref().map(|(_, image)| image),
            ) {
                continue;
            }

            let placement = state.placement;

            if let Some((label, next)) = next_replacement {
                state.replacement = Some((label, Arc::new(next.placed(placement))));
            }

            if next_clip.is_some() {
                state.clip = next_clip;
            }

            state.graphic = graphic.map(|graphic| Arc::new(graphic.placed(placement)));
            state.model = Some(model);

            return Ok(());
        }
    }

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
        let generation = self.begin_model_update();
        let graphic = config
            .map(|config| LShapeGraphic::load(config, width, height))
            .transpose()?;
        let mut state = self.lock();

        if state.model_generation != generation {
            return Ok(());
        }

        let placement = state.placement;
        state.graphic = graphic.map(|graphic| Arc::new(graphic.placed(placement)));
        state.model = None;

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
        self.replace_graphic(label, config, width, height, false)
    }

    /// Replace channel artwork while keeping the assigned model's picture
    /// window. Legacy corner/scale requests still apply through `place`.
    pub fn replace_model_art(
        &self,
        label: &str,
        config: &LShapeConfig,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.replace_graphic(label, config, width, height, true)
    }

    fn replace_graphic(
        &self,
        label: &str,
        config: &LShapeConfig,
        width: u32,
        height: u32,
        inherit_window: bool,
    ) -> Result<()> {
        let model = self.lock().model.clone().filter(|_| inherit_window);
        let mut graphic = match model {
            Some(model) => render_model_art(&model, &config.path, width, height)?,
            None => LShapeGraphic::load(config, width, height)?,
        };
        let mut state = self.lock();
        let placement = state.placement;

        if inherit_window && let Some(model) = &state.graphic {
            graphic.home = model.home;
            graphic.window = model.home;
        }

        state.replacement = Some((label.to_string(), Arc::new(graphic.placed(placement))));
        state.replacement_image = Some(config.path.clone());
        state.replacement_uses_model = inherit_window;

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
        let mut state = self.lock();
        state.replacement = None;
        state.replacement_image = None;
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
        self.set_clip_graphic_with_window(config, width, height, false, false)
    }

    /// Clip image-only overrides inherit the assigned model's free window.
    pub fn set_clip_model_art(
        &self,
        config: Option<&LShapeConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.set_clip_graphic_with_window(config, width, height, true, true)
    }

    pub fn set_clip_model_art_placed(
        &self,
        config: Option<&LShapeConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.set_clip_graphic_with_window(config, width, height, false, true)
    }

    fn set_clip_graphic_with_window(
        &self,
        config: Option<&LShapeConfig>,
        width: u32,
        height: u32,
        inherit_window: bool,
        use_model: bool,
    ) -> Result<()> {
        let Some(config) = config else {
            self.lock().clip = None;
            return Ok(());
        };

        let state = self.lock();
        let home = state.graphic.as_ref().map(|model| model.home);
        let expected_home = if inherit_window && home.is_some() {
            home
        } else if width >= 2 && height >= 2 {
            Some(window(config, width, height))
        } else {
            None
        };
        let cached = state.clip.as_ref().is_some_and(|(loaded, graphic)| {
            loaded == config
                && state.clip_uses_model == use_model
                && state.clip_inherits_window == inherit_window
                && expected_home == Some(graphic.home)
                && graphic.background.width() == width
                && graphic.background.height() == height
        });
        drop(state);

        if cached {
            return Ok(());
        }

        let model = self.lock().model.clone().filter(|_| use_model);
        let loaded = match model {
            Some(model) => render_model_art(&model, &config.path, width, height),
            None => LShapeGraphic::load(config, width, height),
        };
        let graphic = loaded.map(|mut graphic| {
            if inherit_window && let Some(home) = home {
                graphic.home = home;
                graphic.window = home;
            } else if !inherit_window {
                graphic.home = window(config, width, height);
                graphic.window = graphic.home;
            }

            graphic
        });
        let mut state = self.lock();

        match graphic {
            Ok(graphic) => {
                state.clip = Some((config.clone(), Arc::new(graphic)));
                state.clip_uses_model = use_model;
                state.clip_inherits_window = inherit_window;
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

impl LShapeControl {
    /// Draw a bound L-shape model, preserving its complete picture geometry.
    /// Loading it does not take it on or off air.
    pub fn set_template(&self, spec: &GraphicSpec, width: u32, height: u32) -> Result<()> {
        let primary = spec
            .layers
            .iter()
            .position(|layer| matches!(layer.content, GraphicContent::Image { .. }))
            .unwrap_or(0);

        self.set_template_controlled(spec, primary, width, height)
    }

    pub fn set_template_controlled(
        &self,
        spec: &GraphicSpec,
        primary: usize,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let generation = self.begin_model_update();
        let rendered = render_graphic(spec, width, height)?;
        anyhow::ensure!(
            rendered.picture.is_some(),
            "channel L-shape needs a video layer"
        );
        self.set_rendered_graphic(rendered.picture, spec, primary, generation)?;
        Ok(())
    }

    /// Prepare artwork without holding the owner's short publication guard.
    pub fn set_template_controlled_guarded<G>(
        &self,
        spec: &GraphicSpec,
        primary: usize,
        size: (u32, u32),
        mut before_commit: impl FnMut() -> Option<G>,
    ) -> Result<()> {
        let generation = {
            let Some(_publication) = before_commit() else {
                return Ok(());
            };

            self.begin_model_update()
        };
        let rendered = render_graphic(spec, size.0, size.1)?;
        anyhow::ensure!(
            rendered.picture.is_some(),
            "channel L-shape needs a video layer"
        );
        self.set_rendered_graphic_guarded(
            rendered.picture,
            (spec, primary),
            generation,
            render_model_art,
            before_commit,
        )
    }
}

fn render_model_art(
    model: &(GraphicSpec, usize),
    path: &str,
    width: u32,
    height: u32,
) -> Result<LShapeGraphic> {
    let mut spec = model.0.clone();
    if let Some(layer) = spec.layers.get_mut(model.1) {
        layer.content = GraphicContent::Image { path: path.into() };
    } else {
        use crate::{GraphicAnimation, GraphicLayerSpec};

        spec.layers.insert(
            0,
            GraphicLayerSpec {
                content: GraphicContent::Image { path: path.into() },
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
                opacity: 1.0,
                radius: 0.0,
                animation_in: GraphicAnimation::None,
                animation_out: GraphicAnimation::None,
                animation_ms: 0,
                delay_ms: 0,
            },
        );
    }
    let rendered = render_graphic(&spec, width, height)?;

    rendered
        .picture_graphic()
        .cloned()
        .ok_or_else(|| anyhow!("model has no video window"))
}

#[cfg(test)]
mod tests;
