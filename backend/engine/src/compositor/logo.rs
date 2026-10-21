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
/// from outside.
const LOGO_SWITCH: Duration = Duration::from_millis(400);

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
}

impl LogoOverride {
    /// Visibility now, between 0 (hidden) and 1, fading after a switch.
    fn level(&self, now: Instant) -> f64 {
        let target = if self.hidden { 0.0 } else { 1.0 };

        match self.switched_at {
            Some(at) => {
                let progress =
                    now.saturating_duration_since(at).as_secs_f64() / LOGO_SWITCH.as_secs_f64();

                self.from + (target - self.from) * progress.min(1.0)
            }
            None => target,
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

    /// Fade the configured logo back in, dropping any replacement.
    pub fn show(&self) {
        let mut state = self.lock();

        state.replacement = None;
        state.set_hidden(false);
    }

    /// Show the image of `config` in place of the configured logo, sized and
    /// placed for an output of `width` x `height`. `label` names it in
    /// [`LogoState`].
    pub fn replace(&self, label: &str, config: &LogoConfig, width: u32, height: u32) -> Result<()> {
        let overlay = LogoOverlay::load(config, width, height)?;
        let mut state = self.lock();

        state.replacement = Some((label.to_string(), Arc::new(overlay)));
        state.set_hidden(false);

        Ok(())
    }

    pub fn state(&self) -> LogoState {
        let state = self.lock();

        LogoState {
            hidden: state.hidden,
            replacement: state.replacement.as_ref().map(|(label, _)| label.clone()),
        }
    }

    pub(crate) fn has_replacement(&self) -> bool {
        self.lock().replacement.is_some()
    }

    /// For the frame being drawn: how visible the logo is, and the image
    /// replacing the configured one.
    pub(crate) fn frame(&self) -> (f64, Option<Arc<LogoOverlay>>) {
        let state = self.lock();

        (
            state.level(Instant::now()),
            state
                .replacement
                .as_ref()
                .map(|(_, overlay)| overlay.clone()),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{LogoConfig, LogoControl, LogoOverride, logo_dimensions, logo_position};

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
