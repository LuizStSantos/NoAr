use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use ffmpeg_next::{codec, frame, media, software::scaling, util::format::pixel::Pixel};

use crate::{
    compositor::overlay::{OverlayRef, blend_overlay, chroma_alpha},
    utils::{
        config::LogoConfig,
        helper::{even, open_media_input},
    },
};

#[derive(Clone)]
pub struct LogoOverlay {
    pub frame: frame::Video, // YUVA420P
    chroma_alpha: Vec<u8>,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub opacity: u8, // 0..255
}

impl LogoOverlay {
    pub fn load(config: &LogoConfig, output_width: u32, output_height: u32) -> Result<Self> {
        if !(0.0..=1.0).contains(&config.opacity) || !config.opacity.is_finite() {
            return Err(anyhow!("logo opacity must be between 0.0 and 1.0"));
        }

        let mut ictx = open_media_input(&config.path)
            .with_context(|| format!("failed to open logo {}", config.path))?;

        let stream = ictx
            .streams()
            .best(media::Type::Video)
            .ok_or_else(|| anyhow!("logo {} contains no video/image stream", config.path))?;

        let stream_index = stream.index();
        let ctx = codec::context::Context::from_parameters(stream.parameters())?;
        let mut decoder = ctx.decoder().video()?;

        let input_width = decoder.width();
        let input_height = decoder.height();

        let (width, height) = logo_dimensions(
            config.scale.as_deref(),
            input_width,
            input_height,
            output_width,
            output_height,
        )?;

        // Convert the logo directly to YUVA420P.
        // Plane 0 = Y
        // Plane 1 = U
        // Plane 2 = V
        // Plane 3 = Alpha
        let mut scaler = scaling::Context::get(
            decoder.format(),
            input_width,
            input_height,
            Pixel::YUVA420P,
            width,
            height,
            scaling::flag::Flags::BILINEAR,
        )?;

        let mut decoded = frame::Video::empty();
        let mut yuva = None;

        for (packet_stream, packet) in ictx.packets() {
            if packet_stream.index() != stream_index {
                continue;
            }

            decoder.send_packet(&packet)?;

            if decoder.receive_frame(&mut decoded).is_ok() {
                let mut scaled = frame::Video::empty();
                scaler.run(&decoded, &mut scaled)?;
                yuva = Some(scaled);
                break;
            }
        }

        if yuva.is_none() {
            decoder.send_eof()?;

            if decoder.receive_frame(&mut decoded).is_ok() {
                let mut scaled = frame::Video::empty();
                scaler.run(&decoded, &mut scaled)?;
                yuva = Some(scaled);
            }
        }

        let frame = yuva.ok_or_else(|| anyhow!("logo {} produced no frame", config.path))?;
        let (x, y) = logo_position(&config.position, output_width, output_height, width, height)?;

        Ok(Self {
            chroma_alpha: chroma_alpha(&frame, width, height),
            frame,
            x: even(x),
            y: even(y),
            width,
            height,
            opacity: (config.opacity * 255.0).round().clamp(0.0, 255.0) as u8,
        })
    }

    /// The same image placed at `position` (an `X:Y` expression) in an
    /// output of `width` x `height`.
    pub(crate) fn moved_to(&self, position: &str, width: u32, height: u32) -> Result<Self> {
        let (x, y) = logo_position(position, width, height, self.width, self.height)?;
        let mut moved = self.clone();

        moved.x = even(x);
        moved.y = even(y);

        Ok(moved)
    }

    fn as_overlay(&self) -> OverlayRef<'_> {
        OverlayRef {
            frame: &self.frame,
            chroma_alpha: &self.chroma_alpha,
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

pub fn blend_logo(target: &mut frame::Video, logo: &LogoOverlay, opacity_factor: f64) {
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
}

struct Outgoing {
    /// `None` is the configured logo.
    image: Option<Arc<LogoOverlay>>,
    since: Instant,
}

/// What the logo looks like in the frame being drawn.
#[derive(Default)]
pub(crate) struct LogoFrame {
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
    fn lock(&self) -> MutexGuard<'_, LogoOverride> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fade the logo out, whichever image it is.
    pub fn hide(&self) {
        self.lock().set_hidden(true);
    }

    /// Fade the configured logo back in, dropping any replacement, which
    /// cross-fades to it when it is on air.
    pub fn show(&self) {
        let mut state = self.lock();

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
        let mut overlay = LogoOverlay::load(config, width, height)?;
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

        let overlay = match LogoOverlay::load(config, width, height) {
            Ok(overlay) => overlay,
            Err(error) => {
                self.lock().clip_image = None;
                return Err(error);
            }
        };
        self.lock().clip_image = Some((config.path.clone(), Arc::new(overlay)));

        Ok(())
    }

    pub fn state(&self) -> LogoState {
        let state = self.lock();

        LogoState {
            hidden: state.hidden,
            replacement: state.replacement.as_ref().map(|(label, _)| label.clone()),
            position: state.position.clone(),
        }
    }

    pub(crate) fn has_replacement(&self) -> bool {
        let state = self.lock();

        state.replacement.is_some() || state.clip_image.is_some()
    }

    /// For the frame being drawn: how visible the logo is, the image
    /// replacing the configured one, and the one cross-fading away.
    pub(crate) fn frame(&self) -> LogoFrame {
        self.frame_at(Instant::now())
    }

    fn frame_at(&self, now: Instant) -> LogoFrame {
        let mut state = self.lock();
        let switch = state.transitions.switch().as_secs_f64();
        let outgoing = state.outgoing.as_ref().and_then(|outgoing| {
            let left = 1.0 - now.saturating_duration_since(outgoing.since).as_secs_f64() / switch;

            (left > 0.0).then(|| (outgoing.image.clone(), left))
        });

        if outgoing.is_none() {
            state.outgoing = None;
        }

        LogoFrame {
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
}
