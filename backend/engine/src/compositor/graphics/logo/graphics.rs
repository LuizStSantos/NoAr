//! Compatibility logo controls over a layered Graphics model.
//!
//! External placement/replacement, clip overrides and fade state keep one runtime
//! handle; authored pixels are prepared by the shared Graphics renderer.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};

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
    model: Option<Arc<LogoModel>>,
    model_offset: (i32, i32),
    image_animation: Option<Arc<crate::compositor::graphics::images::ImageAnimation>>,
    pub frame: VideoFrame, // YUVA420P
    chroma_alpha: Vec<u8>,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub opacity: u8, // 0..255
}

struct LogoModel {
    spec: GraphicSpec,
    graphic: RenderedGraphic,
    primary: usize,
    frame_size: (u32, u32),
}

struct LogoModelUpdate<'a> {
    spec: Option<&'a GraphicSpec>,
    primary: usize,
    clip_animation_ms: Option<u32>,
    enabled: bool,
    size: (u32, u32),
    animate: bool,
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

    fn from_model(spec: &GraphicSpec, primary: usize, width: u32, height: u32) -> Result<Self> {
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

    fn from_model_config(config: &LogoConfig, width: u32, height: u32) -> Result<Self> {
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

    fn replaced_image(&self, path: &str, width: u32, height: u32) -> Result<Self> {
        self.replaced_image_with(path, width, height, &mut Self::from_model)
    }

    fn replaced_image_with(
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

    fn inherit_motion(&mut self, previous: &Self) {
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

fn logo_dimensions(
    scale: Option<&str>,
    input_width: u32,
    input_height: u32,
    output_width: u32,
    output_height: u32,
) -> Result<(u32, u32)> {
    let Some(scale) = scale.filter(|scale| !scale.trim().is_empty()) else {
        return Ok((even(input_width).max(2), even(input_height).max(2)));
    };
    let (width, height) = scale
        .split_once(':')
        .or_else(|| scale.split_once('x'))
        .ok_or_else(|| anyhow!("logo scale must use WIDTH:HEIGHT or WIDTHxHEIGHT"))?;
    let width = parse_logo_dimension(width, input_width, output_width)?;
    let height = parse_logo_dimension(height, input_height, output_height)?;

    let (width, height) = match (width, height) {
        (Some(width), Some(height)) => (width, height),
        (Some(width), None) => (
            width,
            ((u64::from(width) * u64::from(input_height)) / u64::from(input_width)) as u32,
        ),
        (None, Some(height)) => (
            ((u64::from(height) * u64::from(input_width)) / u64::from(input_height)) as u32,
            height,
        ),
        (None, None) => (input_width, input_height),
    };

    Ok((even(width).max(2), even(height).max(2)))
}

fn parse_logo_dimension(value: &str, input: u32, output: u32) -> Result<Option<u32>> {
    let value = value.trim();

    if value == "-1" {
        return Ok(None);
    }

    if value == "iw" || value == "ih" {
        return Ok(Some(input));
    }

    if value == "W" || value == "H" || value == "main_w" || value == "main_h" {
        return Ok(Some(output));
    }

    if let Some(percent) = value.strip_suffix('%') {
        let percent = percent
            .trim()
            .parse::<f64>()
            .map_err(|_| anyhow!("unsupported logo scale expression {value:?}"))?;
        if !percent.is_finite() || percent <= 0.0 {
            return Err(anyhow!("unsupported logo scale expression {value:?}"));
        }

        return Ok(Some(((f64::from(output) * percent) / 100.0).round() as u32));
    }
    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| anyhow!("unsupported logo scale expression {value:?}"))
}

pub(crate) fn logo_position(
    position: &str,
    output_width: u32,
    output_height: u32,
    logo_width: u32,
    logo_height: u32,
) -> Result<(u32, u32)> {
    let (x, y) = position
        .split_once(':')
        .ok_or_else(|| anyhow!("logo position must use X:Y"))?;
    let x = eval_position_expr(x, output_width, logo_width)?;
    let y = eval_position_expr(y, output_height, logo_height)?;

    Ok((
        x.clamp(0, i64::from(output_width.saturating_sub(logo_width))) as u32,
        y.clamp(0, i64::from(output_height.saturating_sub(logo_height))) as u32,
    ))
}

fn eval_position_expr(expr: &str, main: u32, overlay: u32) -> Result<i64> {
    let normalized = expr
        .replace("main_w", "M")
        .replace("main_h", "M")
        .replace("overlay_w", "O")
        .replace("overlay_h", "O")
        .replace(['W', 'H'], "M")
        .replace(['w', 'h'], "O")
        .replace('-', "+-");
    let mut total = 0_i64;

    for part in normalized.split('+') {
        let part = part.trim();

        if part.is_empty() {
            continue;
        }

        let (sign, part) = part
            .strip_prefix('-')
            .map_or((1_i64, part), |part| (-1_i64, part));
        // A term may be divided by a whole number, so `M/2-O/2` centers the logo.
        let (part, divisor) = match part.split_once('/') {
            Some((part, divisor)) => (
                part.trim(),
                divisor
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .filter(|divisor| *divisor > 0)
                    .ok_or_else(|| anyhow!("unsupported logo position expression {expr:?}"))?,
            ),
            None => (part, 1),
        };
        let value = match part {
            "M" => i64::from(main),
            "O" => i64::from(overlay),
            // A share of the room the logo can move in: 0% is the left (or
            // top) edge, 100% the right (or bottom) one.
            _ if part.ends_with('%') => {
                let percent = part
                    .trim_end_matches('%')
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|percent| (0.0..=100.0).contains(percent))
                    .ok_or_else(|| anyhow!("unsupported logo position expression {expr:?}"))?;

                (f64::from(main.saturating_sub(overlay)) * percent / 100.0).round() as i64
            }
            _ => part
                .parse::<i64>()
                .map_err(|_| anyhow!("unsupported logo position expression {expr:?}"))?,
        };
        total += sign * value / divisor;
    }

    Ok(total)
}

pub fn blend_logo(target: &mut VideoFrame, logo: &LogoOverlay, opacity_factor: f64) {
    if logo.is_model() {
        logo.blend_model(target, &LogoFrame::default(), opacity_factor, 1.0, false);
        return;
    }
    blend_overlay(target, logo.as_overlay(), opacity_factor);
}

/// How long the logo takes to fade out or back in when it is hidden or shown
/// from outside, unless the channel sets its own length.
const LOGO_SWITCH: Duration = Duration::from_millis(400);

/// The longest transition a channel can set, in seconds.
pub const MAX_LOGO_TRANSITION_SECONDS: f64 = 10.0;

/// How the logo appears or disappears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LogoTransition {
    /// It fades in or out.
    #[default]
    Fade,
    /// It appears or disappears at once.
    Cut,
}

/// How the logo appears, disappears and gives way to another image.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LogoTransitions {
    pub show: LogoTransition,
    pub hide: LogoTransition,
    /// Length of a fade, and of the cross-fade when another image takes the
    /// logo's place, in seconds. `None` keeps the built-in lengths: 1 s on
    /// clips, 0.4 s when the logo is switched from outside.
    pub seconds: Option<f64>,
}

impl LogoTransitions {
    fn length(&self) -> Option<f64> {
        self.seconds
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)
            .map(|seconds| seconds.min(MAX_LOGO_TRANSITION_SECONDS))
    }

    /// Length of a fade on clips, in seconds: the channel's, or `default`.
    pub(crate) fn fade_seconds(&self, default: f64) -> f64 {
        self.length().unwrap_or(default)
    }

    /// Length of a switch from outside and of a cross-fade.
    fn switch(&self) -> Duration {
        self.length().map_or(LOGO_SWITCH, Duration::from_secs_f64)
    }
}

/// The logo on air as changed from outside the playout, for example by an
/// encoder whose live has a commercial break: hidden, or another image in
/// place of the configured one. Clones share the same state; it applies to
/// clips, lives and ingest alike, and outlasts a playout restart.
#[derive(Clone, Default)]
pub struct LogoControl(Arc<Mutex<LogoOverride>>);

#[derive(Default)]
struct LogoOverride {
    model_generation: u64,
    scheduled_hidden: Option<bool>,
    scheduled_restore_hidden: Option<bool>,
    configured_enabled: bool,
    layer_managed: bool,
    model_definition: Option<Arc<LogoOverlay>>,
    model_from: Vec<f64>,
    clip_animation_ms: Option<u32>,
    hidden: bool,
    /// Visibility when the last switch began, and when that was.
    from: f64,
    switched_at: Option<Instant>,
    replacement: Option<(String, Arc<LogoOverlay>)>,
    /// The image giving way to the one now shown, while they cross-fade.
    outgoing: Option<Outgoing>,
    transitions: LogoTransitions,
    /// Where the logo is moved to (`X:Y`), in place of the configured spot.
    position: Option<String>,
    /// Where the clip playing puts the logo, when it has a place of its own;
    /// a move from outside goes first.
    clip_position: Option<String>,
    /// The logo of the clip playing, in place of the configured one, and the
    /// image it was read from; a replacement from outside goes first.
    clip_image: Option<(String, Arc<LogoOverlay>)>,
    /// The channel's logo as saved since the playout started, in place of
    /// the one it started with; `Some(None)` turns it off. `None` keeps the
    /// logo the playout was started with.
    configured: Option<Option<Arc<LogoOverlay>>>,
}

struct Outgoing {
    /// `None` is the configured logo.
    image: Option<Arc<LogoOverlay>>,
    since: Instant,
}

/// What the logo looks like in the frame being drawn.
#[derive(Default)]
pub(crate) struct LogoFrame {
    model_elapsed: Option<Duration>,
    model_hidden: bool,
    model_from: Vec<f64>,
    /// How visible the logo is, between 0 (hidden) and 1.
    pub visibility: f64,
    /// The image replacing the configured logo.
    pub replacement: Option<Arc<LogoOverlay>>,
    /// The image giving way to it (`None` inside: the configured logo), and
    /// how much of it is still drawn, between 0 and 1.
    pub outgoing: Option<(Option<Arc<LogoOverlay>>, f64)>,
    /// Where the configured logo is moved to, when it is.
    pub position: Option<String>,
    /// The clip's own logo, drawn in place of the configured one.
    pub clip_image: Option<Arc<LogoOverlay>>,
    /// The configured logo saved while on air (`Some(None)`: no logo), in
    /// place of the one the playout started with.
    pub configured: Option<Option<Arc<LogoOverlay>>>,
}

impl LogoOverride {
    /// Visibility now, between 0 (hidden) and 1, fading after a switch.
    fn level(&self, now: Instant) -> f64 {
        let target = if self.hidden { 0.0 } else { 1.0 };
        let transition = if self.hidden {
            self.transitions.hide
        } else {
            self.transitions.show
        };

        match self.switched_at {
            Some(at) if transition == LogoTransition::Fade => {
                let progress = now.saturating_duration_since(at).as_secs_f64()
                    / self.transitions.switch().as_secs_f64();

                self.from + (target - self.from) * progress.min(1.0)
            }
            _ => target,
        }
    }

    /// Let the image on air cross-fade to the next one, when it is visible.
    fn hand_over(&mut self, now: Instant) {
        if !self.hidden && self.level(now) > 0.0 {
            self.outgoing = Some(Outgoing {
                image: self
                    .replacement
                    .as_ref()
                    .map(|(_, overlay)| overlay.clone()),
                since: now,
            });
        }
    }

    fn set_hidden(&mut self, hidden: bool) {
        if hidden != self.hidden {
            let now = Instant::now();

            if let Some(Some(overlay)) = &self.configured
                && let Some(model) = &overlay.model
            {
                self.model_from = model.graphic.role_levels(
                    self.switched_at.map(|at| now.saturating_duration_since(at)),
                    self.hidden,
                    &self.model_from,
                );
            }

            self.from = self.level(now);
            self.hidden = hidden;
            self.switched_at = Some(now);
        }
    }
}

/// What the logo on air is doing, as set from outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogoState {
    pub hidden: bool,
    /// The image shown in place of the configured logo.
    pub replacement: Option<String>,
    /// Where the logo is moved to (`X:Y`), when it is.
    pub position: Option<String>,
}

impl fmt::Debug for LogoControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("LogoControl").field(&self.state()).finish()
    }
}

impl LogoControl {
    /// A schedule temporarily enables the saved model without changing startup
    /// configuration, replacement artwork or the operator's previous visibility.
    pub fn set_scheduled_visible(&self, active: bool) {
        let mut state = self.lock();

        if active {
            if state.scheduled_hidden.is_none() {
                state.scheduled_hidden = Some(
                    state
                        .scheduled_restore_hidden
                        .take()
                        .unwrap_or(state.hidden),
                );
            }

            let empty = state.configured.as_ref().is_none_or(Option::is_none)
                && state.replacement.is_none()
                && state.clip_image.is_none();

            if empty {
                // A disabled definition has no previous pixels. Enter its layers
                // from zero instead of treating an unhidden empty slot as on air.
                state.hidden = true;
                state.from = 0.0;
                state.switched_at = None;
                state.model_from.clear();
            }

            state.configured = Some(state.model_definition.clone());
            state.set_hidden(false);
        } else if let Some(hidden) = state.scheduled_hidden.take() {
            if !state.configured_enabled
                && state.replacement.is_none()
                && state.clip_image.is_none()
            {
                // Keep pixels through the layer exit, then restore the disabled
                // definition and original raw hidden flag on the frame thread.
                state.scheduled_restore_hidden = Some(hidden);
                state.set_hidden(true);
            } else {
                state.configured = Some(if state.configured_enabled {
                    state.model_definition.clone()
                } else {
                    None
                });
                state.set_hidden(hidden);
            }
        }
    }

    /// An indefinite airing relinquishes ownership after an operator hides it.
    /// Keep that new visibility instead of restoring the older schedule snapshot.
    pub fn abandon_scheduled_visibility(&self) {
        let mut state = self.lock();

        if state.scheduled_hidden.take().is_some() {
            if !state.configured_enabled
                && state.replacement.is_none()
                && state.clip_image.is_none()
            {
                state.scheduled_restore_hidden = Some(state.hidden);
                state.set_hidden(true);
            } else {
                state.configured = Some(if state.configured_enabled {
                    state.model_definition.clone()
                } else {
                    None
                });
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, LogoOverride> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Requested visibility of a loaded channel image or override. Clip fades
    /// and L-shape suppression are applied separately on each output frame.
    pub fn is_visible(&self) -> bool {
        let state = self.lock();
        let loaded = state.configured.as_ref().is_some_and(Option::is_some)
            || state.replacement.is_some()
            || state.clip_image.is_some();

        loaded && !state.hidden
    }

    /// Fade the logo out, whichever image it is.
    pub fn hide(&self) {
        let mut state = self.lock();

        if state.scheduled_restore_hidden.is_some() {
            state.scheduled_restore_hidden = Some(true);
        }

        state.set_hidden(true);
    }

    /// Fade the configured logo back in, dropping any replacement, which
    /// cross-fades to it when it is on air.
    pub fn show(&self) {
        let mut state = self.lock();

        if state.scheduled_restore_hidden.is_some() {
            state.scheduled_restore_hidden = Some(false);
        }

        if state.replacement.is_some() {
            state.hand_over(Instant::now());
        }
        state.replacement = None;
        state.set_hidden(false);
    }

    /// How the logo appears, disappears and gives way to another image from
    /// now on, on clips and when switched from outside.
    pub fn set_transitions(&self, transitions: LogoTransitions) {
        self.lock().transitions = transitions;
    }

    pub fn transitions(&self) -> LogoTransitions {
        self.lock().transitions
    }

    /// Show the image of `config` in place of the configured logo, sized and
    /// placed for an output of `width` x `height`. `label` names it in
    /// [`LogoState`].
    pub fn replace(&self, label: &str, config: &LogoConfig, width: u32, height: u32) -> Result<()> {
        let configured = {
            let state = self.lock();
            state
                .model_definition
                .clone()
                .or_else(|| state.configured.clone().flatten())
        };
        let mut overlay = match configured.filter(|overlay| overlay.is_model()) {
            Some(model) => model.replaced_image(&config.path, width, height)?,
            None if self.lock().layer_managed => {
                LogoOverlay::from_model_config(config, width, height)?
            }
            None => LogoOverlay::load(config, width, height)?,
        };
        let mut state = self.lock();

        if let Some(position) = &state.position {
            overlay = overlay.moved_to(position, width, height)?;
        }

        state.hand_over(Instant::now());
        state.replacement = Some((label.to_string(), Arc::new(overlay)));
        state.set_hidden(false);

        Ok(())
    }

    /// Put the logo, whichever image it is, at `position` (an `X:Y`
    /// expression such as `W-w-12:12`) in an output of `width` x `height`;
    /// `None` puts it back where the channel places it. It stays there
    /// until moved again or the server restarts.
    pub fn move_to(&self, position: Option<&str>, width: u32, height: u32) -> Result<()> {
        let mut state = self.lock();

        match position {
            Some(position) => {
                // Checked here, so a wrong expression is refused, not drawn wrong.
                logo_position(position, width, height, 2, 2)?;

                if let Some((label, overlay)) = &state.replacement {
                    let moved = overlay.moved_to(position, width, height)?;
                    state.replacement = Some((label.clone(), Arc::new(moved)));
                }
                state.position = Some(position.to_string());
            }
            None => state.position = None,
        }

        Ok(())
    }

    /// Where the clip about to play puts the logo (an `X:Y` expression), or
    /// none for the channel's place. A move from outside still goes first.
    pub fn set_clip_position(&self, position: Option<&str>, width: u32, height: u32) -> Result<()> {
        if let Some(position) = position {
            logo_position(position, width, height, 2, 2)?;
        }

        self.lock().clip_position = position.map(str::to_string);

        Ok(())
    }

    /// The logo of the clip about to play, in place of the configured one
    /// (`None` for the configured logo), sized and placed like it for an
    /// output of `width` x `height`. The same image is not read again. A
    /// replacement from outside still goes first.
    pub fn set_clip_image(
        &self,
        config: Option<&LogoConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let Some(config) = config else {
            self.lock().clip_image = None;
            return Ok(());
        };

        if self
            .lock()
            .clip_image
            .as_ref()
            .is_some_and(|(path, _)| path == &config.path)
        {
            return Ok(());
        }

        let configured = self.lock().configured.clone().flatten();
        let loaded = match configured.filter(|overlay| overlay.is_model()) {
            Some(model) => model.replaced_image(&config.path, width, height),
            None => LogoOverlay::load(config, width, height),
        };
        let overlay = match loaded {
            Ok(overlay) => overlay,
            Err(error) => {
                self.lock().clip_image = None;
                return Err(error);
            }
        };
        self.lock().clip_image = Some((config.path.clone(), Arc::new(overlay)));

        Ok(())
    }

    /// Take the channel's logo settings on air: `config` (`None` for no
    /// logo) replaces the configured logo, sized and placed for an output of
    /// `width` x `height`, without restarting the playout. A visible logo
    /// cross-fades to the new image. Replacements, moves and the clip's own
    /// logo still go first.
    pub fn set_configured(
        &self,
        config: Option<&LogoConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let generation = {
            let mut state = self.lock();
            state.model_generation = state.model_generation.wrapping_add(1);
            state.model_generation
        };
        // Read outside the lock: frames keep being drawn meanwhile.
        let overlay = config
            .map(|config| LogoOverlay::load(config, width, height))
            .transpose()?
            .map(Arc::new);
        let mut state = self.lock();

        if state.model_generation != generation {
            return Ok(());
        }

        let now = Instant::now();

        if state.replacement.is_none()
            && !state.hidden
            && state.level(now) > 0.0
            && let Some(Some(previous)) = &state.configured
        {
            state.outgoing = Some(Outgoing {
                image: Some(previous.clone()),
                since: now,
            });
        }
        state.configured = Some(overlay);
        state.model_definition = None;
        state.layer_managed = false;
        state.clip_animation_ms = None;

        Ok(())
    }

    /// The assigned template is the only configured logo. Native setters remain
    /// available to SDK clients; application compatibility controls edit overrides.
    pub fn set_model(
        &self,
        spec: Option<&GraphicSpec>,
        primary: usize,
        clip_animation_ms: Option<u32>,
        enabled: bool,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.set_model_inner(
            spec,
            primary,
            clip_animation_ms,
            enabled,
            (width, height),
            true,
        )
    }

    /// Value refreshes must preserve active fades, visibility and placement overrides.
    pub fn refresh_model(
        &self,
        spec: Option<&GraphicSpec>,
        primary: usize,
        clip_animation_ms: Option<u32>,
        enabled: bool,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.set_model_inner(
            spec,
            primary,
            clip_animation_ms,
            enabled,
            (width, height),
            false,
        )
    }

    /// Refresh a model only while the owner grants a short publication guard.
    /// Pixel preparation runs before the final guard is acquired.
    pub fn refresh_model_guarded<G>(
        &self,
        spec: Option<&GraphicSpec>,
        primary: usize,
        clip_animation_ms: Option<u32>,
        enabled: bool,
        size: (u32, u32),
        before_commit: impl FnMut() -> Option<G>,
    ) -> Result<()> {
        self.set_model_with_guarded(
            LogoModelUpdate {
                spec,
                primary,
                clip_animation_ms,
                enabled,
                size,
                animate: false,
            },
            LogoOverlay::from_model,
            before_commit,
        )
    }

    fn set_model_inner(
        &self,
        spec: Option<&GraphicSpec>,
        primary: usize,
        clip_animation_ms: Option<u32>,
        enabled: bool,
        size: (u32, u32),
        animate: bool,
    ) -> Result<()> {
        self.set_model_with(
            LogoModelUpdate {
                spec,
                primary,
                clip_animation_ms,
                enabled,
                size,
                animate,
            },
            LogoOverlay::from_model,
        )
    }

    fn set_model_with(
        &self,
        update: LogoModelUpdate<'_>,
        render: impl FnMut(&GraphicSpec, usize, u32, u32) -> Result<LogoOverlay>,
    ) -> Result<()> {
        self.set_model_with_guarded(update, render, || Some(()))
    }

    fn set_model_with_guarded<G>(
        &self,
        update: LogoModelUpdate<'_>,
        mut render: impl FnMut(&GraphicSpec, usize, u32, u32) -> Result<LogoOverlay>,
        mut before_commit: impl FnMut() -> Option<G>,
    ) -> Result<()> {
        let LogoModelUpdate {
            spec,
            primary,
            clip_animation_ms,
            enabled,
            size,
            animate,
        } = update;
        let (width, height) = size;
        let generation = {
            let Some(_publication) = before_commit() else {
                return Ok(());
            };
            let mut state = self.lock();
            state.model_generation = state.model_generation.wrapping_add(1);
            state.configured_enabled = enabled;

            if enabled {
                state.scheduled_restore_hidden = None;
            }

            let active = enabled
                || state.scheduled_hidden.is_some()
                || state.scheduled_restore_hidden.is_some();
            let current = state.model_definition.as_ref();
            let unchanged = match (spec, current) {
                (None, None) => true,
                (Some(spec), Some(overlay)) => overlay.model.as_ref().is_some_and(|model| {
                    model.spec == *spec
                        && model.primary == primary
                        && model.frame_size == (width, height)
                }),
                _ => false,
            };

            if state.layer_managed
                && unchanged
                && state.clip_animation_ms == clip_animation_ms
                && state.configured.as_ref().and_then(Option::as_ref).is_some()
                    == (active && spec.is_some())
            {
                return Ok(());
            }

            state.model_generation
        };

        loop {
            let (previous, clip, replacement, position) = {
                let state = self.lock();

                if state.model_generation != generation {
                    return Ok(());
                }

                (
                    state.model_definition.clone(),
                    state.clip_image.clone(),
                    state.replacement.clone(),
                    state.position.clone(),
                )
            };
            // Rasterization and image decoding must never hold the frame mutex.
            let mut overlay = spec
                .map(|spec| render(spec, primary, width, height))
                .transpose()?;

            if !animate && let (Some(next), Some(previous)) = (&mut overlay, &previous) {
                next.inherit_motion(previous);
            }

            let mut next_clip = None;
            let mut next_replacement = None;

            if let Some(model) = &overlay {
                if let Some((path, old)) = &clip {
                    let mut next = model.replaced_image_with(path, width, height, &mut render)?;

                    if !animate {
                        next.inherit_motion(old);
                    }

                    next_clip = Some((path.clone(), Arc::new(next)));
                }

                if let Some((label, old)) = &replacement
                    && let Some(previous_model) = &old.model
                    && let Some(layer) = previous_model.spec.layers.get(previous_model.primary)
                    && let GraphicContent::Image { path } = &layer.content
                {
                    let mut next = model.replaced_image_with(
                        &path.to_string_lossy(),
                        width,
                        height,
                        &mut render,
                    )?;

                    if !animate {
                        next.inherit_motion(old);
                    }

                    if let Some(position) = &position {
                        next = next.moved_to(position, width, height)?;
                    }

                    next_replacement = Some((label.clone(), Arc::new(next)));
                }
            }

            let Some(_publication) = before_commit() else {
                return Ok(());
            };
            let mut state = self.lock();

            if state.model_generation != generation {
                return Ok(());
            }

            // An override may have changed while its pixels were being prepared.
            // Retry against that new snapshot rather than resurrecting old artwork.
            if !super::same_prepared(
                state.clip_image.as_ref().map(|(_, image)| image),
                clip.as_ref().map(|(_, image)| image),
            ) || !super::same_prepared(
                state.replacement.as_ref().map(|(_, image)| image),
                replacement.as_ref().map(|(_, image)| image),
            ) || state.position != position
            {
                continue;
            }

            if animate
                && !state.hidden
                && let Some(Some(previous)) = &state.configured
            {
                state.outgoing = Some(Outgoing {
                    image: Some(previous.clone()),
                    since: Instant::now(),
                });
            }

            if next_clip.is_some() {
                state.clip_image = next_clip;
            }

            if next_replacement.is_some() {
                state.replacement = next_replacement;
            }

            let active = state.configured_enabled
                || state.scheduled_hidden.is_some()
                || state.scheduled_restore_hidden.is_some();
            let overlay = overlay.map(Arc::new);
            state.layer_managed = true;
            state.model_definition = overlay.clone();
            state.configured = Some(overlay.filter(|_| active));
            state.clip_animation_ms = clip_animation_ms;

            return Ok(());
        }
    }

    pub(crate) fn clip_transitions(&self) -> LogoTransitions {
        let state = self.lock();
        let mut transitions = state.transitions;

        if let Some(milliseconds) = state.clip_animation_ms {
            transitions.seconds = Some(f64::from(milliseconds) / 1000.0);

            if let Some(model) = state
                .model_definition
                .as_ref()
                .and_then(|overlay| overlay.model.as_ref())
                && let Some(layer) = model.spec.layers.get(model.primary)
            {
                let transition = |animation| {
                    if milliseconds == 0 || animation == crate::GraphicAnimation::None {
                        LogoTransition::Cut
                    } else {
                        LogoTransition::Fade
                    }
                };
                transitions.show = transition(layer.animation_in);
                transitions.hide = transition(layer.animation_out);
            }
        }

        transitions
    }

    pub fn state(&self) -> LogoState {
        let state = self.lock();

        LogoState {
            hidden: state.hidden,
            replacement: state.replacement.as_ref().map(|(label, _)| label.clone()),
            position: state.position.clone(),
        }
    }

    /// Whether a logo image is set from any source (tests check the state
    /// with it).
    #[cfg(test)]
    pub(crate) fn has_replacement(&self) -> bool {
        let state = self.lock();

        state.replacement.is_some()
            || state.clip_image.is_some()
            || matches!(state.configured, Some(Some(_)))
    }

    /// For the frame being drawn: how visible the logo is, the image
    /// replacing the configured one, and the one cross-fading away.
    pub(crate) fn frame(&self) -> LogoFrame {
        self.frame_at(Instant::now())
    }

    fn frame_at(&self, now: Instant) -> LogoFrame {
        let mut state = self.lock();
        if let Some(hidden) = state.scheduled_restore_hidden {
            let levels = state
                .model_definition
                .as_ref()
                .and_then(|overlay| overlay.model.as_ref())
                .map(|model| {
                    model.graphic.role_levels(
                        state
                            .switched_at
                            .map(|at| now.saturating_duration_since(at)),
                        true,
                        &state.model_from,
                    )
                });

            if levels.is_none_or(|levels| levels.iter().all(|level| *level <= 0.0)) {
                state.scheduled_restore_hidden = None;
                state.configured = Some(None);
                state.hidden = hidden;
                state.switched_at = None;
                state.from = if hidden { 0.0 } else { 1.0 };
            }
        }

        let switch = state.transitions.switch().as_secs_f64();
        let outgoing = state.outgoing.as_ref().and_then(|outgoing| {
            let left = 1.0 - now.saturating_duration_since(outgoing.since).as_secs_f64() / switch;

            (left > 0.0).then(|| (outgoing.image.clone(), left))
        });

        if outgoing.is_none() {
            state.outgoing = None;
        }

        LogoFrame {
            model_elapsed: state
                .switched_at
                .map(|at| now.saturating_duration_since(at)),
            model_hidden: state.hidden,
            model_from: state.model_from.clone(),
            visibility: state.level(now),
            replacement: state
                .replacement
                .as_ref()
                .map(|(_, overlay)| overlay.clone()),
            outgoing,
            position: state
                .position
                .clone()
                .or_else(|| state.clip_position.clone()),
            clip_image: state
                .clip_image
                .as_ref()
                .map(|(_, overlay)| overlay.clone()),
            configured: state.configured.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        LogoConfig, LogoControl, LogoOverride, LogoTransition, LogoTransitions, logo_dimensions,
        logo_position,
    };

    fn logo_asset() -> LogoConfig {
        LogoConfig {
            path: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests_assets/storage/logo.png"
            )
            .to_string(),
            scale: Some("10%:-1".to_string()),
            opacity: 1.0,
            position: "W-w-12:12".to_string(),
        }
    }

    fn scrolling_model_spec() -> crate::GraphicSpec {
        use crate::{GraphicAnimation, GraphicContent, GraphicLayerSpec, GraphicSpec, RgbaColor};
        let image = GraphicLayerSpec {
            content: GraphicContent::Image {
                path: "unavailable.png".into(),
            },
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
            opacity: 1.0,
            radius: 0.0,
            animation_in: GraphicAnimation::SlideLeft,
            animation_out: GraphicAnimation::Wipe,
            animation_ms: 1000,
            delay_ms: 200,
        };
        GraphicSpec {
            picture: None,
            layers: vec![
                image.clone(),
                GraphicLayerSpec {
                    content: GraphicContent::Text {
                        text: "Scrolling model".to_string(),
                        scroll: Some(crate::GraphicTextScroll {
                            direction: crate::ScrollDirection::Left,
                            speed: 80.0,
                            gap: 40,
                        }),
                        font_family: None,
                        font_size: 6.0,
                        bold: false,
                        color: RgbaColor::opaque(255, 255, 255),
                        align: crate::BlockAlign::Left,
                        fill: None,
                    },
                    y: 70.0,
                    width: 80.0,
                    ..image
                },
            ],
        }
    }

    fn synthetic_model(
        spec: &crate::GraphicSpec,
        primary: usize,
        width: u32,
        height: u32,
    ) -> anyhow::Result<super::LogoOverlay> {
        // Prepared test images have no dependency on native image I/O.
        let mut drawable = spec.clone();
        for layer in &mut drawable.layers {
            if matches!(layer.content, crate::GraphicContent::Image { .. }) {
                layer.content = crate::GraphicContent::Box {
                    fill: crate::RgbaColor::opaque(255, 255, 255),
                };
            }
        }
        let mut overlay = super::LogoOverlay::from_model(&drawable, primary, width, height)?;
        std::sync::Arc::get_mut(overlay.model.as_mut().unwrap())
            .unwrap()
            .spec = spec.clone();

        Ok(overlay)
    }

    #[test]
    fn moved_models_share_prepared_pixels_scroll_epoch_and_layer_animations() {
        let spec = scrolling_model_spec();
        let base = synthetic_model(&spec, 0, 320, 180).unwrap();
        let moved = base.moved_to("80:40", 320, 180).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            base.model.as_ref().unwrap(),
            moved.model.as_ref().unwrap()
        ));
        assert_eq!(moved.model_offset, (48, 22));
        assert_eq!(moved.x, 80);
        assert_eq!(moved.y, 40);
        let returned = moved.moved_to("32:18", 320, 180).unwrap();
        assert_eq!(returned.model_offset, (0, 0));
        let model = returned.model.as_ref().unwrap();
        assert!(model.graphic.layers[1].scroll.is_some());
        assert_eq!(
            model.graphic.layers[0].animation_in,
            crate::GraphicAnimation::SlideLeft
        );
        assert_eq!(model.graphic.layers[0].delay, Duration::from_millis(200));
    }

    #[test]
    fn moving_a_gif_model_keeps_its_decoded_animation_without_reopening_the_source() {
        let mut spec = scrolling_model_spec();
        spec.layers[0].content = crate::GraphicContent::Image {
            path: concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests_assets/graphics.gif"
            )
            .into(),
        };
        let mut base = super::LogoOverlay::from_model(&spec, 0, 320, 180).unwrap();
        let model = std::sync::Arc::get_mut(base.model.as_mut().unwrap()).unwrap();
        assert!(model.graphic.layers[0].image_animation.is_some());
        model.spec.layers[0].content = crate::GraphicContent::Image {
            path: "missing-after-preparation.gif".into(),
        };
        let moved = base.moved_to("80:40", 320, 180).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            base.model.as_ref().unwrap(),
            moved.model.as_ref().unwrap()
        ));
    }

    #[test]
    fn logo_refresh_keeps_independent_motion_epochs_for_definition_and_overrides() {
        use std::sync::Arc;
        let control = LogoControl::default();
        let spec = scrolling_model_spec();
        let epochs = [5, 7, 9].map(|seconds| Instant::now() - Duration::from_secs(seconds));
        let overlays: Vec<_> = epochs
            .iter()
            .map(|epoch| {
                let mut overlay = synthetic_model(&spec, 0, 320, 180).unwrap();
                for layer in &mut Arc::get_mut(overlay.model.as_mut().unwrap())
                    .unwrap()
                    .graphic
                    .layers
                {
                    layer.prepared_at = *epoch;
                }
                Arc::new(overlay)
            })
            .collect();
        {
            let mut state = control.lock();
            state.model_definition = Some(overlays[0].clone());
            state.configured = Some(Some(overlays[0].clone()));
            state.clip_image = Some(("clip.png".to_string(), overlays[1].clone()));
            state.replacement = Some(("external".to_string(), overlays[2].clone()));
        }
        let mut next = spec.clone();
        next.layers[1].opacity = 0.7;
        control
            .set_model_with(
                super::LogoModelUpdate {
                    spec: Some(&next),
                    primary: 0,
                    clip_animation_ms: None,
                    enabled: true,
                    size: (320, 180),
                    animate: false,
                },
                synthetic_model,
            )
            .unwrap();
        let state = control.lock();
        let images = [
            state.model_definition.as_ref().unwrap(),
            &state.clip_image.as_ref().unwrap().1,
            &state.replacement.as_ref().unwrap().1,
        ];
        for (image, epoch) in images.into_iter().zip(epochs) {
            assert_eq!(
                image.model.as_ref().unwrap().graphic.layers[1].prepared_at,
                epoch
            );
        }
    }

    #[test]
    fn override_refresh_releases_frame_mutex_and_discards_changed_inputs() {
        use std::sync::Arc;
        let control = LogoControl::default();
        let spec = scrolling_model_spec();
        let old = Arc::new(synthetic_model(&spec, 0, 320, 180).unwrap());
        {
            let mut state = control.lock();
            state.model_definition = Some(old.clone());
            state.configured = Some(Some(old.clone()));
            state.clip_image = Some(("clip.png".to_string(), old.clone()));
            state.replacement = Some(("external".to_string(), old));
        }
        let mut next = spec.clone();
        next.layers[1].opacity = 0.7;
        let mut preparations = 0;
        control
            .set_model_with(
                super::LogoModelUpdate {
                    spec: Some(&next),
                    primary: 0,
                    clip_animation_ms: None,
                    enabled: true,
                    size: (320, 180),
                    animate: false,
                },
                |spec, primary, width, height| {
                    preparations += 1;
                    assert!(
                        control.0.try_lock().is_ok(),
                        "preparation holds the frame mutex"
                    );
                    let _ = control.frame();
                    if preparations == 2 {
                        control.set_clip_image(None, width, height).unwrap();
                        control.show();
                    }

                    synthetic_model(spec, primary, width, height)
                },
            )
            .unwrap();
        assert!(preparations >= 3);
        let state = control.lock();
        assert!(state.clip_image.is_none());
        assert!(state.replacement.is_none());
        assert_eq!(
            state
                .model_definition
                .as_ref()
                .unwrap()
                .model
                .as_ref()
                .unwrap()
                .spec,
            next
        );
    }

    #[test]
    fn cancelled_owner_cannot_publish_a_logo_after_preparation() {
        use crate::{GraphicContent, GraphicSpec};
        use std::sync::{
            Arc, Barrier,
            atomic::{AtomicBool, Ordering},
        };

        let control = LogoControl::default();
        let original = GraphicSpec {
            layers: Vec::new(),
            picture: None,
        };
        let mut next = scrolling_model_spec();
        next.layers[0].content = GraphicContent::Box {
            fill: crate::RgbaColor::opaque(255, 0, 0),
        };
        control
            .set_model(Some(&original), 0, None, true, 320, 180)
            .unwrap();
        let allowed = Arc::new(AtomicBool::new(true));
        let prepared = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_control = control.clone();
        let worker_allowed = allowed.clone();
        let worker_prepared = prepared.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            let mut calls = 0;
            worker_control
                .refresh_model_guarded(Some(&next), 0, None, true, (320, 180), || {
                    calls += 1;

                    if calls == 2 {
                        worker_prepared.wait();
                        worker_release.wait();
                    }

                    worker_allowed.load(Ordering::SeqCst).then_some(())
                })
                .unwrap();
        });
        prepared.wait();
        let frames_unlocked = control.0.try_lock().is_ok();
        allowed.store(false, Ordering::SeqCst);
        release.wait();
        worker.join().unwrap();
        assert!(frames_unlocked, "preparation must leave frames unlocked");
        let state = control.lock();
        assert_eq!(
            state
                .model_definition
                .as_ref()
                .unwrap()
                .model
                .as_ref()
                .unwrap()
                .spec,
            original
        );
    }

    #[test]
    fn older_logo_preparation_cannot_republish_after_a_newer_model_clear() {
        let control = LogoControl::default();
        let spec = scrolling_model_spec();
        control
            .set_model_with(
                super::LogoModelUpdate {
                    spec: Some(&spec),
                    primary: 0,
                    clip_animation_ms: None,
                    enabled: true,
                    size: (320, 180),
                    animate: false,
                },
                |spec, primary, width, height| {
                    control
                        .set_model(None, 0, None, false, width, height)
                        .unwrap();

                    synthetic_model(spec, primary, width, height)
                },
            )
            .unwrap();
        assert!(control.lock().model_definition.is_none());
    }

    #[test]
    fn dynamic_role_refresh_preserves_the_active_fade_and_hidden_state() {
        let control = LogoControl::default();
        let spec = crate::GraphicSpec {
            picture: None,
            layers: vec![crate::GraphicLayerSpec {
                content: crate::GraphicContent::Box {
                    fill: crate::RgbaColor {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    },
                },
                x: 80.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
                opacity: 1.0,
                radius: 0.0,
                animation_in: crate::GraphicAnimation::Fade,
                animation_out: crate::GraphicAnimation::Fade,
                animation_ms: 1000,
                delay_ms: 0,
            }],
        };
        control
            .set_model(Some(&spec), 0, None, true, 320, 180)
            .unwrap();
        control.hide();
        let switched = control.lock().switched_at;
        let mut next = spec.clone();
        next.layers[0].opacity = 0.7;
        control
            .refresh_model(Some(&next), 0, None, true, 320, 180)
            .unwrap();
        let state = control.lock();
        assert!(state.hidden);
        assert_eq!(state.switched_at, switched);
        assert!(state.outgoing.is_none());
        assert_eq!(
            state
                .model_definition
                .as_ref()
                .unwrap()
                .model
                .as_ref()
                .unwrap()
                .spec,
            next
        );
    }

    #[test]
    fn a_layer_logo_matches_native_pixels_and_keeps_other_layers_during_image_overrides() {
        use super::{LogoFrame, LogoOverlay, blend_logo};
        use crate::frame::{PixelFormat, VideoFrame};
        use crate::{GraphicAnimation, GraphicContent, GraphicLayerSpec, GraphicSpec, RgbaColor};

        let mut config = logo_asset();
        config.scale = Some("80:48".to_string());
        config.position = "12:12".to_string();
        config.opacity = 0.6;
        let layer = GraphicLayerSpec {
            content: GraphicContent::Image {
                path: config.path.clone().into(),
            },
            x: 1.875,
            y: 100.0 * 12.0 / 360.0,
            width: 12.5,
            height: 100.0 * 48.0 / 360.0,
            opacity: 0.6,
            radius: 0.0,
            animation_in: GraphicAnimation::Fade,
            animation_out: GraphicAnimation::Fade,
            animation_ms: 1000,
            delay_ms: 0,
        };
        let spec = GraphicSpec {
            picture: None,
            layers: vec![layer.clone()],
        };
        let native = LogoOverlay::load(&config, 640, 360).unwrap();
        let model = LogoOverlay::from_model(&spec, 0, 640, 360).unwrap();

        for visibility in [0.0, 0.25, 0.5, 1.0] {
            let mut before = VideoFrame::new(PixelFormat::Yuv420p, 640, 360);
            let mut after = before.clone();
            blend_logo(&mut before, &native, visibility);
            model.blend_model(&mut after, &LogoFrame::default(), visibility, 1.0, false);

            for plane in 0..3 {
                assert_eq!(
                    before.data(plane),
                    after.data(plane),
                    "plane {plane}, opacity {visibility}"
                );
            }
        }

        let mut decoration = layer;
        decoration.content = GraphicContent::Box {
            fill: RgbaColor::opaque(255, 255, 255),
        };
        decoration.x = 50.0;
        decoration.opacity = 1.0;
        let composite = GraphicSpec {
            picture: None,
            layers: vec![spec.layers[0].clone(), decoration],
        };
        let control = LogoControl::default();
        control
            .set_model(Some(&composite), 0, Some(1000), true, 640, 360)
            .unwrap();
        control.replace("replacement", &config, 640, 360).unwrap();
        let replacement = control.frame().replacement.unwrap();
        assert_eq!(
            replacement.model.as_ref().unwrap().spec.layers[1],
            composite.layers[1]
        );
        control.hide();
        control
            .set_model(Some(&composite), 0, Some(1000), true, 640, 360)
            .unwrap();
        assert!(
            control.state().hidden,
            "saving does not show a hidden model"
        );
    }

    #[test]
    fn a_clip_logo_is_drawn_until_the_next_clip_has_none() {
        let control = LogoControl::default();

        control
            .set_clip_image(Some(&logo_asset()), 1920, 1080)
            .unwrap();
        assert!(control.frame().clip_image.is_some());
        assert!(control.has_replacement());

        // The same image is kept, not read again.
        let first = control.frame().clip_image.unwrap();
        control
            .set_clip_image(Some(&logo_asset()), 1920, 1080)
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &first,
            &control.frame().clip_image.unwrap()
        ));

        control.set_clip_image(None, 1920, 1080).unwrap();
        assert!(control.frame().clip_image.is_none());
        assert!(!control.has_replacement());

        let mut missing = logo_asset();
        missing.path.push_str(".missing");
        assert!(control.set_clip_image(Some(&missing), 1920, 1080).is_err());
        assert!(control.frame().clip_image.is_none());
    }

    #[test]
    fn hiding_the_logo_fades_it_out_and_showing_fades_it_back() {
        let mut state = LogoOverride::default();
        let start = Instant::now();

        assert_eq!(state.level(start), 1.0);

        state.set_hidden(true);
        let at = state.switched_at.unwrap();
        assert!((state.level(at + Duration::from_millis(200)) - 0.5).abs() < 0.01);
        assert_eq!(state.level(at + Duration::from_secs(1)), 0.0);

        // Shown again halfway: it fades back in from where it was.
        state.from = 0.0;
        state.switched_at = Some(at - Duration::from_secs(1));
        state.set_hidden(false);
        let back = state.switched_at.unwrap();
        assert!(state.level(back) < 0.01);
        assert_eq!(state.level(back + Duration::from_secs(1)), 1.0);
    }

    #[test]
    fn a_cut_hides_and_shows_the_logo_at_once() {
        let mut state = LogoOverride {
            transitions: LogoTransitions {
                show: LogoTransition::Cut,
                hide: LogoTransition::Cut,
                seconds: None,
            },
            ..LogoOverride::default()
        };

        state.set_hidden(true);
        assert_eq!(state.level(state.switched_at.unwrap()), 0.0);
        state.set_hidden(false);
        assert_eq!(state.level(state.switched_at.unwrap()), 1.0);
    }

    #[test]
    fn the_channel_sets_how_long_a_switch_takes() {
        let mut state = LogoOverride {
            transitions: LogoTransitions {
                seconds: Some(2.0),
                ..LogoTransitions::default()
            },
            ..LogoOverride::default()
        };

        state.set_hidden(true);
        let at = state.switched_at.unwrap();
        assert!((state.level(at + Duration::from_secs(1)) - 0.5).abs() < 0.01);
        assert_eq!(state.level(at + Duration::from_secs(2)), 0.0);
    }

    #[test]
    fn a_new_image_cross_fades_with_the_one_on_air() {
        let control = LogoControl::default();
        control.set_transitions(LogoTransitions {
            seconds: Some(1.0),
            ..LogoTransitions::default()
        });

        control.replace("a.png", &logo_asset(), 1920, 1080).unwrap();
        let since = control.lock().outgoing.as_ref().unwrap().since;

        // The configured logo gives way to the new image over the second.
        let frame = control.frame_at(since + Duration::from_millis(250));
        let (image, left) = frame.outgoing.unwrap();
        assert!(image.is_none());
        assert!((left - 0.75).abs() < 0.01);
        assert!(frame.replacement.is_some());
        assert!(
            control
                .frame_at(since + Duration::from_secs(1))
                .outgoing
                .is_none()
        );

        // Back to the configured logo: the replacement fades away under it.
        control.show();
        let since = control.lock().outgoing.as_ref().unwrap().since;
        let (image, _) = control.frame_at(since).outgoing.unwrap();
        assert!(image.is_some());

        // A hidden logo has nothing on air to cross-fade from.
        let hidden = LogoControl::default();
        hidden.hide();
        hidden.lock().switched_at = None;
        hidden.replace("b.png", &logo_asset(), 1920, 1080).unwrap();
        assert!(hidden.lock().outgoing.is_none());
    }

    #[test]
    fn a_saved_logo_goes_on_air_without_a_restart() {
        let control = LogoControl::default();
        // Nothing saved yet: the logo the playout started with stays.
        assert!(control.frame().configured.is_none());

        control
            .set_configured(Some(&logo_asset()), 1920, 1080)
            .unwrap();
        let first = control.frame().configured.unwrap().unwrap();
        assert!(control.has_replacement());
        // The first save has no earlier saved image to cross-fade from.
        assert!(control.lock().outgoing.is_none());

        // A second save cross-fades from the image on air.
        let mut moved = logo_asset();
        moved.position = "12:12".to_string();
        control.set_configured(Some(&moved), 1920, 1080).unwrap();
        let (image, _) = control.frame().outgoing.unwrap();
        assert!(std::sync::Arc::ptr_eq(&image.unwrap(), &first));
        assert!(!std::sync::Arc::ptr_eq(
            &control.frame().configured.unwrap().unwrap(),
            &first
        ));

        // Turned off: no logo, and nothing extra to draw.
        control.set_configured(None, 1920, 1080).unwrap();
        assert!(matches!(control.frame().configured, Some(None)));
        assert!(!control.has_replacement());

        // A missing image is refused and keeps what is on air.
        let mut missing = logo_asset();
        missing.path.push_str(".missing");
        assert!(control.set_configured(Some(&missing), 1920, 1080).is_err());
        assert!(matches!(control.frame().configured, Some(None)));
    }

    #[test]
    fn a_replacement_shows_until_the_logo_is_shown_again() {
        let control = LogoControl::default();

        control
            .replace("Break/logo.png", &logo_asset(), 1920, 1080)
            .unwrap();
        assert!(control.has_replacement());
        assert_eq!(
            control.state().replacement.as_deref(),
            Some("Break/logo.png")
        );

        control.hide();
        assert!(control.state().hidden);
        // Hiding keeps the replacement for when the logo comes back.
        assert!(control.has_replacement());

        control.show();
        assert!(!control.state().hidden);
        assert!(!control.has_replacement());
        assert!(
            control
                .replace(
                    "x",
                    &LogoConfig {
                        path: "/missing.png".into(),
                        ..logo_asset()
                    },
                    1920,
                    1080
                )
                .is_err()
        );
    }

    #[test]
    fn the_logo_can_be_moved_and_put_back() {
        let control = LogoControl::default();

        assert!(control.move_to(Some("nonsense"), 1920, 1080).is_err());
        control.move_to(Some("12:H-h-12"), 1920, 1080).unwrap();
        assert_eq!(control.state().position.as_deref(), Some("12:H-h-12"));
        assert_eq!(control.frame().position.as_deref(), Some("12:H-h-12"));

        // A replacement goes to the same place, and moves with it.
        control.replace("a.png", &logo_asset(), 1920, 1080).unwrap();
        let replacement = control.frame().replacement.unwrap();
        assert_eq!(replacement.x, 12);
        assert_eq!(replacement.y, 1080 - replacement.height - 12);
        control.move_to(Some("W-w-12:12"), 1920, 1080).unwrap();
        let replacement = control.frame().replacement.unwrap();
        assert_eq!(replacement.x, 1920 - replacement.width - 12);

        control.move_to(None, 1920, 1080).unwrap();
        assert_eq!(control.state().position, None);
    }

    #[test]
    fn a_clip_places_the_logo_unless_it_was_moved_from_outside() {
        let control = LogoControl::default();

        control
            .set_clip_position(Some("50%:100%"), 1920, 1080)
            .unwrap();
        assert_eq!(control.frame().position.as_deref(), Some("50%:100%"));
        // Not a move from outside: the state shows none.
        assert_eq!(control.state().position, None);

        control.move_to(Some("12:12"), 1920, 1080).unwrap();
        assert_eq!(control.frame().position.as_deref(), Some("12:12"));
        control.move_to(None, 1920, 1080).unwrap();
        assert_eq!(control.frame().position.as_deref(), Some("50%:100%"));

        control.set_clip_position(None, 1920, 1080).unwrap();
        assert_eq!(control.frame().position, None);
        assert!(control.set_clip_position(Some("nope"), 1920, 1080).is_err());
    }

    #[test]
    fn logo_position_accepts_corners_and_centers() {
        assert_eq!(
            logo_position("W-w-12:12", 1920, 1080, 200, 100).unwrap(),
            (1708, 12)
        );
        assert_eq!(
            logo_position("W/2-w/2:H-h-24", 1920, 1080, 200, 100).unwrap(),
            (860, 956)
        );
        assert!(logo_position("W/0:12", 1920, 1080, 200, 100).is_err());
        // Shares of the room left around the logo.
        assert_eq!(
            logo_position("50%:100%", 1920, 1080, 200, 100).unwrap(),
            (860, 980)
        );
        assert_eq!(
            logo_position("25.5%:0%", 1920, 1080, 200, 100).unwrap(),
            (439, 0)
        );
        assert!(logo_position("120%:0%", 1920, 1080, 200, 100).is_err());
    }

    #[test]
    fn logo_scale_accepts_output_percentages() {
        assert_eq!(
            logo_dimensions(Some("12%:-1"), 400, 200, 1920, 1080).unwrap(),
            (230, 114)
        );
        assert_eq!(
            logo_dimensions(Some("-1:10%"), 400, 200, 1920, 1080).unwrap(),
            (216, 108)
        );
    }

    #[test]
    fn logo_scale_rejects_invalid_percentages() {
        assert!(logo_dimensions(Some("0%:-1"), 400, 200, 1920, 1080).is_err());
        assert!(logo_dimensions(Some("nan%:-1"), 400, 200, 1920, 1080).is_err());
    }
    #[test]
    fn scheduled_logo_visibility_restores_disabled_and_hidden_states_across_refresh() {
        let control = LogoControl::default();
        let spec = crate::GraphicSpec {
            picture: None,
            layers: vec![crate::GraphicLayerSpec {
                content: crate::GraphicContent::Box {
                    fill: crate::RgbaColor {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: 255,
                    },
                },
                x: 80.0,
                y: 5.0,
                width: 10.0,
                height: 10.0,
                opacity: 1.0,
                radius: 0.0,
                animation_in: crate::GraphicAnimation::Fade,
                animation_out: crate::GraphicAnimation::Fade,
                animation_ms: 400,
                delay_ms: 0,
            }],
        };
        control
            .set_model(Some(&spec), 0, None, false, 320, 180)
            .unwrap();
        assert!(!control.is_visible());
        control.set_scheduled_visible(true);
        let entered = control.lock().switched_at.unwrap();
        assert!(control.frame_at(entered).visibility < 0.01);
        assert!(
            (control
                .frame_at(entered + Duration::from_millis(200))
                .visibility
                - 0.5)
                .abs()
                < 0.01
        );
        control.set_scheduled_visible(true);
        assert!(control.is_visible());
        control
            .refresh_model(Some(&spec), 0, None, false, 320, 180)
            .unwrap();
        assert!(control.is_visible());
        // Simulate the fully entered interval without sleeping or changing a clock.
        control.lock().switched_at = None;
        control.set_scheduled_visible(false);
        let at = control.lock().switched_at.unwrap();
        assert!(!control.is_visible());
        let fading = control.frame_at(at + Duration::from_millis(200));
        assert!((fading.visibility - 0.5).abs() < 0.01);
        control.frame_at(at + Duration::from_millis(500));
        assert!(!control.state().hidden);
        control.set_scheduled_visible(true);
        control.lock().switched_at = None;
        control.set_scheduled_visible(false);
        control
            .set_model(Some(&spec), 0, None, true, 320, 180)
            .unwrap();
        control.show();
        control.frame_at(Instant::now() + Duration::from_secs(2));
        assert!(
            control.is_visible(),
            "a manual startup enable must supersede pending schedule cleanup"
        );
        control
            .set_model(Some(&spec), 0, None, true, 320, 180)
            .unwrap();
        control.hide();
        control.set_scheduled_visible(true);
        assert!(control.is_visible());
        control.set_scheduled_visible(false);
        assert!(!control.is_visible());
        assert!(control.state().hidden);
        control
            .set_model(Some(&spec), 0, None, false, 320, 180)
            .unwrap();
        control.set_scheduled_visible(true);
        control.hide();
        control.abandon_scheduled_visibility();
        assert!(!control.is_visible());
        assert!(control.state().hidden);
    }
}
