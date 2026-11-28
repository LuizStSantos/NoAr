//! What logo is on air, its overrides and its transitions.

use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use anyhow::Result;

use super::{
    overlay::{LogoModelUpdate, LogoOverlay},
    position::logo_position,
};
use crate::{
    compositor::graphics::{GraphicContent, GraphicSpec},
    utils::config::LogoConfig,
};

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
    pub(super) model_elapsed: Option<Duration>,
    pub(super) model_hidden: bool,
    pub(super) model_from: Vec<f64>,
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
            if !super::super::same_prepared(
                state.clip_image.as_ref().map(|(_, image)| image),
                clip.as_ref().map(|(_, image)| image),
            ) || !super::super::same_prepared(
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
mod tests;
