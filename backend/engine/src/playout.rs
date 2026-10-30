//! GStreamer decoding on the continuous NoAr scheduling timeline.

use std::{
    error::Error,
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use ffmpeg_next::{
    format, frame,
    util::{channel_layout::ChannelLayout, format::pixel::Pixel, format::sample::Sample},
};
use log::{debug, trace};

use crate::{
    LogoFade, MAX_LOGO_GAPS, PlaybackControl,
    benchmark::{self, Stage},
    compositor::{logo::*, text::TextOverlay},
    input::{
        gstreamer::{FrameEvent, Source},
        preload::{OpenedInput, Preloaded},
    },
    output::FrameOutput,
    utils::{
        config::{OutputConfig, TextOverlayState},
        ffmpeg::{make_video_frame_writable, reference_video_frame},
        helper::{is_live_input, is_network_url},
    },
};

const LOGO_FADE_SECONDS: f64 = 1.0;
const MEDIA_FADE_SECONDS: f64 = 1.0;
const MEDIA_DURATION_TOLERANCE_US: i64 = 500_000;
#[derive(Debug)]
pub(crate) struct PlaybackSkipped;

impl fmt::Display for PlaybackSkipped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("playback skipped")
    }
}

impl Error for PlaybackSkipped {}

#[derive(Debug)]
pub(crate) struct PlaybackRestart;

impl fmt::Display for PlaybackRestart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("playout restart requested")
    }
}

impl Error for PlaybackRestart {}

#[derive(Debug)]
pub(crate) struct LiveSourceLost(pub(crate) String);

impl fmt::Display for LiveSourceLost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LiveSourceLost {}

fn source_error(error: anyhow::Error, live: bool) -> anyhow::Error {
    if live {
        LiveSourceLost(format!("{error:#}")).into()
    } else {
        error
    }
}

pub(crate) fn check_playback_control(playback_control: &PlaybackControl) -> Result<()> {
    if playback_control.is_shutdown() || playback_control.take_restart() {
        return Err(PlaybackRestart.into());
    }

    if playback_control.take_skip_current() || playback_control.take_navigation() {
        return Err(PlaybackSkipped.into());
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) struct Timeline {
    video_pts: i64,
    audio_pts: i64,
    text_pts: i64,
    logo_opacity: f64,
    /// The last clip kept the logo off; the next one fades it back in.
    logo_hidden: bool,
    // Live ingest preserves source PTS until the receiver maps both tracks
    // onto the continuous playout timeline. File playback always uses the
    // normal monotonically increasing timeline values.
}

impl Timeline {
    pub(crate) fn new() -> Self {
        Self {
            video_pts: 0,
            audio_pts: 0,
            text_pts: 0,
            logo_opacity: 1.0,
            logo_hidden: false,
        }
    }

    pub(crate) fn video_pts(&self) -> i64 {
        self.video_pts
    }

    pub(crate) fn audio_pts(&self) -> i64 {
        self.audio_pts
    }

    #[cfg(test)]
    pub(crate) fn at(video_pts: i64, audio_pts: i64) -> Self {
        Self {
            video_pts,
            audio_pts,
            ..Self::new()
        }
    }

    /// After a clip that kept the logo off, the next clip that shows it fades
    /// it back in, whatever its own plan says.
    pub(crate) fn resume_logo(&self, fade: LogoFade) -> LogoFade {
        if self.logo_hidden && !fade.hidden {
            LogoFade {
                fade_in: true,
                ..fade
            }
        } else {
            fade
        }
    }

    pub(crate) fn finish_logo_fade(&mut self, fade: LogoFade) {
        self.logo_hidden = fade.hidden;

        if fade.hidden || fade.fade_out {
            self.logo_opacity = 0.0;
        } else if fade.fade_in {
            self.logo_opacity = 1.0;
        }
    }
}

#[derive(Clone, Copy, Default)]
struct MediaFadePlan {
    video_end_pts: Option<i64>,
    audio_end_pts: Option<i64>,
    video_frames: i64,
    audio_samples: i64,
}

impl MediaFadePlan {
    fn for_trimmed_clip(
        requested_duration_us: Option<i64>,
        seek_us: i64,
        source_duration_us: Option<i64>,
        timeline: &Timeline,
        cfg: &OutputConfig,
    ) -> Self {
        let Some(requested_duration_us) = requested_duration_us else {
            return Self::default();
        };
        let Some(source_duration_us) = source_duration_us else {
            return Self::default();
        };

        let scheduled_out_us = seek_us.saturating_add(requested_duration_us);

        if (scheduled_out_us - source_duration_us).abs() <= MEDIA_DURATION_TOLERANCE_US {
            return Self::default();
        }

        Self {
            video_end_pts: Some(
                timeline.video_pts
                    + div_ceil(
                        i128::from(requested_duration_us) * i128::from(cfg.fps),
                        1_000_000,
                    ) as i64,
            ),
            audio_end_pts: Some(
                timeline.audio_pts
                    + div_ceil(
                        i128::from(requested_duration_us) * i128::from(cfg.sample_rate),
                        1_000_000,
                    ) as i64,
            ),
            video_frames: (MEDIA_FADE_SECONDS * f64::from(cfg.fps)).round().max(1.0) as i64,
            audio_samples: (MEDIA_FADE_SECONDS * f64::from(cfg.sample_rate))
                .round()
                .max(1.0) as i64,
        }
    }

    fn video_opacity_at(self, pts: i64) -> f32 {
        self.video_end_pts.map_or(1.0, |end_pts| {
            ((end_pts - pts).max(0) as f64 / self.video_frames as f64).clamp(0.0, 1.0) as f32
        })
    }

    fn audio_gain_at(self, pts: i64) -> f32 {
        self.audio_end_pts.map_or(1.0, |end_pts| {
            ((end_pts - pts).max(0) as f64 / self.audio_samples as f64).clamp(0.0, 1.0) as f32
        })
    }

    fn for_external_audio_trim(
        requested_duration_us: Option<i64>,
        seek_us: i64,
        source_duration_us: Option<i64>,
        timeline: &Timeline,
        cfg: &OutputConfig,
    ) -> Self {
        let (Some(requested_duration_us), Some(source_duration_us)) =
            (requested_duration_us, source_duration_us)
        else {
            return Self::default();
        };

        let scheduled_out_us = seek_us.saturating_add(requested_duration_us);

        if scheduled_out_us.saturating_add(MEDIA_DURATION_TOLERANCE_US) >= source_duration_us {
            return Self::default();
        }

        Self {
            audio_end_pts: Some(
                timeline.audio_pts
                    + div_ceil(
                        i128::from(requested_duration_us) * i128::from(cfg.sample_rate),
                        1_000_000,
                    ) as i64,
            ),
            audio_samples: (MEDIA_FADE_SECONDS * f64::from(cfg.sample_rate))
                .round()
                .max(1.0) as i64,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LogoFadePlan {
    fade_in: bool,
    fade_out: bool,
    hidden: bool,
    start_pts: i64,
    end_pts: Option<i64>,
    /// Frames the logo takes to come up and to go off; 0 is a cut.
    frames_in: i64,
    frames_out: i64,
    /// Stretches without logo, in output video PTS.
    gaps: [(i64, i64); MAX_LOGO_GAPS],
    gap_count: usize,
    /// Stretches in the L-shape, in output video PTS, and the frames it takes
    /// to go in or out.
    lshape: [(i64, i64); MAX_LOGO_GAPS],
    lshape_count: usize,
    lshape_frames: i64,
}

impl LogoFadePlan {
    /// `seek_seconds` is where in the source the clip starts, so stretches
    /// without logo, given in source time, land on the right frames.
    fn new(
        start_pts: i64,
        duration_seconds: Option<f64>,
        seek_seconds: Option<f64>,
        cfg: &OutputConfig,
        fade: LogoFade,
    ) -> Self {
        let fps = f64::from(cfg.fps);
        let end_pts = duration_seconds
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .map(|duration| start_pts + (duration * fps).ceil() as i64);
        let seek_ms = seek_seconds
            .filter(|seek| seek.is_finite() && *seek > 0.0)
            .map_or(0, |seek| (seek * 1000.0).round() as i64);
        let to_pts = |ms: i64| start_pts + ((ms - seek_ms) as f64 * fps / 1000.0).round() as i64;
        let mut gaps = [(0, 0); MAX_LOGO_GAPS];
        let mut gap_count = 0;

        for (from, to) in fade.gaps.iter().filter(|(_, to)| *to > seek_ms) {
            gaps[gap_count] = (to_pts(from), to_pts(to));
            gap_count += 1;
        }

        let mut lshape = [(0, 0); MAX_LOGO_GAPS];
        let mut lshape_count = 0;

        for (from, to) in fade.lshape.iter().filter(|(_, to)| *to > seek_ms) {
            lshape[lshape_count] = (to_pts(from), to_pts(to));
            lshape_count += 1;
        }

        let transitions = cfg.logo_control.transitions();
        let frames = (transitions.fade_seconds(LOGO_FADE_SECONDS) * fps)
            .round()
            .max(1.0) as i64;
        let frames_for = |transition| match transition {
            LogoTransition::Fade => frames,
            LogoTransition::Cut => 0,
        };

        Self {
            fade_in: fade.fade_in,
            fade_out: fade.fade_out,
            hidden: fade.hidden,
            start_pts,
            end_pts,
            frames_in: frames_for(transitions.show),
            frames_out: frames_for(transitions.hide),
            gaps,
            gap_count,
            lshape,
            lshape_count,
            lshape_frames: (crate::LSHAPE_SWITCH_SECONDS * fps).round() as i64,
        }
    }

    /// How far into the L-shape the stretches marked in the clip put `pts`.
    fn lshape_level(self, pts: i64) -> f64 {
        crate::compositor::lshape::marked_level(
            &self.lshape[..self.lshape_count],
            pts,
            self.lshape_frames,
        )
    }

    /// How much of the logo a stretch without logo lets through at `pts`:
    /// 0 inside one, fading over the logo fade length on both sides. Kept
    /// apart from the clip's own fade, which the next clip carries on from.
    fn gap_factor(self, pts: i64) -> f64 {
        self.gaps[..self.gap_count]
            .iter()
            .map(|&(from, to)| {
                if pts < from {
                    ramp(from - pts, self.frames_out)
                } else if pts <= to {
                    0.0
                } else {
                    ramp(pts - to, self.frames_in)
                }
            })
            .fold(1.0, f64::min)
    }

    fn opacity_at(self, pts: i64, current_opacity: f64) -> f64 {
        if self.hidden {
            return 0.0;
        }

        let mut opacity = current_opacity;

        if self.fade_in {
            // A cut shows the logo from the first frame.
            opacity = if self.frames_in == 0 {
                1.0
            } else {
                ramp(pts - self.start_pts, self.frames_in)
            };
        }

        if self.fade_out
            && let Some(end_pts) = self.end_pts
        {
            opacity = opacity.min(ramp(end_pts - pts, self.frames_out));
        }

        opacity
    }

    fn finish(self, timeline: &mut Timeline) {
        timeline.finish_logo_fade(LogoFade {
            fade_in: self.fade_in,
            fade_out: self.fade_out,
            hidden: self.hidden,
            ..LogoFade::default()
        });
    }
}

/// How far a fade over `frames` frames got after `distance` frames, between
/// 0 and 1. Over 0 frames (a cut) it is 1 as soon as `distance` is positive.
fn ramp(distance: i64, frames: i64) -> f64 {
    if frames <= 0 {
        if distance > 0 { 1.0 } else { 0.0 }
    } else {
        (distance.max(0) as f64 / frames as f64).clamp(0.0, 1.0)
    }
}

/// Decode through GStreamer and feed the persistent output in media order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn play_clip_with_external_audio<O: FrameOutput>(
    path: &str,
    external_audio_path: Option<&str>,
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    seek_seconds: Option<f64>,
    duration_seconds: Option<f64>,
    subtitles_media_path: Option<&str>,
    logo_fade: LogoFade,
    playback_control: &PlaybackControl,
    preloaded: Option<Preloaded>,
) -> Result<()> {
    ensure!(
        seek_seconds.is_none_or(|v| v.is_finite() && v >= 0.0),
        "seek must be finite and nonnegative"
    );
    ensure!(
        duration_seconds.is_none_or(|v| v.is_finite() && v > 0.0),
        "duration must be finite and positive"
    );
    let live = is_live_input(path);
    let seek = if live { None } else { seek_seconds };
    let clip_start = *timeline;
    let origin_video = timeline.video_pts;
    let origin_audio = timeline.audio_pts;
    let opening_started = Instant::now();
    let mut opened = open_input(path, cfg, timeline, output, playback_control, preloaded)?;
    debug!(channel = cfg.channel_id.unwrap_or_default();
        "GStreamer source opened in {:?}; output video={}, audio={}", opening_started.elapsed(), timeline.video_pts, timeline.audio_pts);
    let info = opened.input.info();
    let duration =
        duration_seconds.or_else(|| info.duration.map(|v| (v - seek.unwrap_or(0.0)).max(0.0)));
    let end_video = duration.map(|v| origin_video + (v * f64::from(cfg.fps)).ceil() as i64);
    let end_audio = duration.map(|v| origin_audio + (v * f64::from(cfg.sample_rate)).ceil() as i64);
    let fade = timeline.resume_logo(logo_fade);
    let logo = LogoFadePlan::new(origin_video, duration, seek, cfg, fade);
    let media_fade = if live {
        MediaFadePlan::default()
    } else {
        MediaFadePlan::for_trimmed_clip(
            duration.map(seconds_to_microseconds),
            seconds_to_microseconds(seek.unwrap_or(0.0)),
            info.duration.map(seconds_to_microseconds),
            &clip_start,
            cfg,
        )
    };
    let mut compositor = VideoCompositor::new(path, cfg, timeline, end_video)?;
    let mut external = external_audio_path
        .map(|path| ExternalAudioInput::open(path, cfg, &clip_start, seek, duration))
        .transpose()?;
    let mut first_iteration = true;
    let mut last_picture = None;
    let mut decoded_frames = 0;
    output.clear_vtt_subtitles()?;
    if let Some(media) = subtitles_media_path {
        output.write_vtt_subtitles(
            media,
            origin_video.saturating_mul(1000) / i64::from(cfg.fps),
            (seek.unwrap_or(0.0) * 1000.0).round() as i64,
        )?;
    }
    output.set_video_end(end_video)?;

    let result = (|| {
        loop {
            check_playback_control(playback_control)?;
            let start_video = timeline.video_pts;
            let start_audio = timeline.audio_pts;
            let source_started = Instant::now();
            opened
                .input
                .start(if first_iteration { seek } else { None })
                .map_err(|error| source_error(error, live))?;
            debug!(channel = cfg.channel_id.unwrap_or_default();
                "GStreamer source started in {:?}; seek={:?}", source_started.elapsed(), if first_iteration { seek } else { None });
            let source_origin = if live {
                None
            } else {
                Some(if first_iteration {
                    seek.unwrap_or(0.0)
                } else {
                    0.0
                })
            };
            let mut source_origin = source_origin;
            let mut last_media = Instant::now();
            let mut last_hold = Instant::now();
            let mut video_frames = 0;

            loop {
                check_playback_control(playback_control)?;
                if end_video.is_some_and(|end| timeline.video_pts >= end)
                    && end_audio.is_some_and(|end| timeline.audio_pts >= end)
                {
                    break;
                }
                let event = match opened.preroll.pop_front() {
                    Some(event) => Some(event),
                    None => opened
                        .input
                        .next(Duration::from_millis(10))
                        .map_err(|error| source_error(error, live))?,
                };
                let Some(event) = event else {
                    if opened.input.is_eos() {
                        break;
                    }
                    if live && last_media.elapsed() > Duration::from_secs(2) {
                        return Err(
                            LiveSourceLost("live source stopped supplying media".into()).into()
                        );
                    }
                    if live
                        && last_hold.elapsed() >= Duration::from_secs_f64(1.0 / f64::from(cfg.fps))
                    {
                        if end_video.is_none_or(|end| timeline.video_pts < end) {
                            let picture = last_picture
                                .as_ref()
                                .or(compositor.last_composited_frame.as_ref());
                            write_padding_video_frames(cfg, timeline, output, 1, picture)?;
                            synchronize_silence_to_video(cfg, timeline, output, playback_control)?;
                        }
                        last_hold = Instant::now();
                    }
                    continue;
                };
                last_media = Instant::now();
                last_hold = Instant::now();

                match event {
                    FrameEvent::Video(mut picture) => {
                        if end_video.is_some_and(|end| timeline.video_pts >= end) {
                            continue;
                        }
                        let source = picture.pts().unwrap_or(0) as f64 / f64::from(cfg.fps);
                        let origin = *source_origin.get_or_insert(source);
                        let target = start_video
                            + ((source - origin).max(0.0) * f64::from(cfg.fps)).round() as i64;
                        let gap = target
                            .saturating_sub(timeline.video_pts)
                            .min(i64::from(cfg.fps));
                        if gap > 0 {
                            let count = end_video
                                .map_or(gap, |end| gap.min(end.saturating_sub(timeline.video_pts)));
                            write_padding_video_frames(
                                cfg,
                                timeline,
                                output,
                                count,
                                compositor.last_composited_frame.as_ref(),
                            )?;
                            // A missing video frame does not imply missing
                            // audio. Let the source's pending audio arrive
                            // instead of replacing its opening samples.
                            if external.is_none() && (!info.has_audio || opened.input.audio_ended())
                            {
                                synchronize_silence_to_video(
                                    cfg,
                                    timeline,
                                    output,
                                    playback_control,
                                )?;
                            }
                        }
                        if target + 1 < timeline.video_pts || (live && target < timeline.video_pts)
                        {
                            continue;
                        }
                        last_picture = Some(reference_video_frame(&picture)?);
                        picture.set_pts(Some(timeline.video_pts));
                        encode_composited_frame(
                            picture,
                            &mut compositor,
                            timeline,
                            media_fade,
                            logo,
                            output,
                            &mut decoded_frames,
                        )?;
                        video_frames += 1;
                        if let Some(audio) = external.as_mut() {
                            audio.until_video(
                                cfg,
                                timeline,
                                output,
                                end_audio,
                                playback_control,
                            )?;
                        } else if !info.has_audio || opened.input.audio_ended() {
                            synchronize_silence_to_video(cfg, timeline, output, playback_control)?;
                        }
                    }
                    FrameEvent::Audio(audio) => {
                        if external.is_some() {
                            continue;
                        }
                        if end_audio.is_some_and(|end| timeline.audio_pts >= end) {
                            continue;
                        }
                        let source = audio.pts().unwrap_or(0) as f64 / f64::from(cfg.sample_rate);
                        let origin = *source_origin.get_or_insert(source);
                        let target = start_audio
                            + ((source - origin).max(0.0) * f64::from(cfg.sample_rate)).round()
                                as i64;
                        let gap = target
                            .saturating_sub(timeline.audio_pts)
                            .min(i64::from(cfg.sample_rate));
                        if gap > 0 {
                            let count = end_audio
                                .map_or(gap, |end| gap.min(end.saturating_sub(timeline.audio_pts)));
                            write_silence(cfg, timeline, output, count)?;
                        }
                        let skip = usize::try_from(timeline.audio_pts.saturating_sub(target))
                            .unwrap_or(usize::MAX)
                            .min(audio.samples());
                        send_audio(
                            &audio, skip, cfg, timeline, output, end_audio, media_fade, live,
                        )?;
                        if !info.has_video || opened.input.video_ended() {
                            let target = div_ceil(
                                i128::from(timeline.audio_pts) * i128::from(cfg.fps),
                                i128::from(cfg.sample_rate),
                            ) as i64;
                            while timeline.video_pts < target
                                && end_video.is_none_or(|end| timeline.video_pts < end)
                            {
                                let frame = last_picture
                                    .as_ref()
                                    .map(reference_video_frame)
                                    .transpose()?
                                    .unwrap_or_else(|| black_video_frame_for_config(cfg));
                                encode_composited_frame(
                                    frame,
                                    &mut compositor,
                                    timeline,
                                    media_fade,
                                    logo,
                                    output,
                                    &mut decoded_frames,
                                )?;
                            }
                        }
                    }
                }
                if live && external.is_none() {
                    synchronize_live_tracks(
                        cfg,
                        timeline,
                        output,
                        end_video,
                        end_audio,
                        compositor.last_composited_frame.as_ref(),
                    )?;
                }
            }

            if live || duration_seconds.is_none() {
                break;
            }
            let remaining = end_video
                .unwrap_or(timeline.video_pts)
                .saturating_sub(timeline.video_pts);
            if remaining <= 0 {
                break;
            }
            if video_frames <= 1 && last_picture.is_some() {
                while end_video.is_some_and(|end| timeline.video_pts < end) {
                    check_playback_control(playback_control)?;
                    let picture = reference_video_frame(
                        last_picture.as_ref().context("still picture missing")?,
                    )?;
                    encode_composited_frame(
                        picture,
                        &mut compositor,
                        timeline,
                        media_fade,
                        logo,
                        output,
                        &mut decoded_frames,
                    )?;
                    if let Some(audio) = external.as_mut() {
                        audio.until_video(cfg, timeline, output, end_audio, playback_control)?;
                    } else {
                        synchronize_silence_to_video(cfg, timeline, output, playback_control)?;
                    }
                }
                break;
            }
            ensure!(
                timeline.video_pts > start_video || timeline.audio_pts > start_audio,
                "source did not advance the output timeline"
            );
            if remaining < i64::from(cfg.fps) * 3 {
                write_padding_video_frames(
                    cfg,
                    timeline,
                    output,
                    remaining,
                    compositor.last_composited_frame.as_ref(),
                )?;
                synchronize_silence_to_video(cfg, timeline, output, playback_control)?;
                break;
            }
            opened = open_input(path, cfg, timeline, output, playback_control, None)?;
            first_iteration = false;
        }
        if let Some(audio) = external.as_mut() {
            audio.until_video(cfg, timeline, output, end_audio, playback_control)?;
        }
        synchronize_timeline(
            cfg,
            timeline,
            output,
            compositor.last_composited_frame.as_ref(),
        )?;
        output.video_decoded()?;
        output.video_finished()?;
        Ok(())
    })();
    if result
        .as_ref()
        .is_err_and(anyhow::Error::is::<LiveSourceLost>)
    {
        // Give the scheduler control immediately so its waiting/retry policy
        // can run without filling the rest of the live event with black.
        synchronize_timeline(
            cfg,
            timeline,
            output,
            compositor.last_composited_frame.as_ref(),
        )?;
    }
    if result.is_ok() {
        logo.finish(timeline);
    }
    if result
        .as_ref()
        .is_err_and(|error| error.downcast_ref::<PlaybackSkipped>().is_some())
    {
        synchronize_after_skip(
            cfg,
            timeline,
            output,
            compositor.last_composited_frame.as_ref(),
        )?;
    }

    result
}

fn seconds_to_microseconds(seconds: f64) -> i64 {
    (seconds * 1_000_000.0).round().max(0.0) as i64
}

#[allow(clippy::too_many_arguments)]
fn send_audio<O: FrameOutput>(
    source: &frame::Audio,
    skip: usize,
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    limit: Option<i64>,
    fade: MediaFadePlan,
    live: bool,
) -> Result<()> {
    let count = source
        .samples()
        .saturating_sub(skip)
        .min(limit.map_or(usize::MAX, |end| {
            end.saturating_sub(timeline.audio_pts).max(0) as usize
        }));
    if count == 0 {
        return Ok(());
    }
    let mut audio = frame::Audio::new(
        Sample::F32(format::sample::Type::Planar),
        count,
        ChannelLayout::STEREO,
    );
    audio.set_rate(cfg.sample_rate);
    audio.set_pts(Some(timeline.audio_pts));
    for plane in 0..2 {
        audio
            .plane_mut::<f32>(plane)
            .copy_from_slice(&source.plane::<f32>(plane)[skip..skip + count]);
    }
    if let Some(callback) = cfg.audio_frame_callback.as_ref() {
        callback.emit(&audio);
    }
    apply_audio_fade(&mut audio, timeline.audio_pts, fade);
    if live {
        output.encode_live_audio(&audio)?;
    } else {
        output.encode_audio_with_gain_hold(
            &audio,
            fade.audio_gain_at(timeline.audio_pts + count as i64) < 1.0,
        )?;
    }
    timeline.audio_pts += count as i64;

    Ok(())
}

struct ExternalAudioInput {
    source: Source,
    pending: Option<frame::Audio>,
    offset: usize,
    eof: bool,
    fade: MediaFadePlan,
}

impl ExternalAudioInput {
    fn open(
        path: &str,
        cfg: &OutputConfig,
        timeline: &Timeline,
        seek: Option<f64>,
        duration: Option<f64>,
    ) -> Result<Self> {
        let mut source = Source::open(path, cfg)?;
        ensure!(
            source.info().has_audio,
            "external audio source has no audio"
        );
        let fade = MediaFadePlan::for_external_audio_trim(
            duration.map(seconds_to_microseconds),
            seconds_to_microseconds(seek.unwrap_or(0.0)),
            source.info().duration.map(seconds_to_microseconds),
            timeline,
            cfg,
        );
        source.start(seek)?;
        Ok(Self {
            source,
            pending: None,
            offset: 0,
            eof: false,
            fade,
        })
    }

    fn until_video<O: FrameOutput>(
        &mut self,
        cfg: &OutputConfig,
        timeline: &mut Timeline,
        output: &mut O,
        end: Option<i64>,
        control: &PlaybackControl,
    ) -> Result<()> {
        let target = div_ceil(
            i128::from(timeline.video_pts) * i128::from(cfg.sample_rate),
            i128::from(cfg.fps),
        ) as i64;
        let target = end.map_or(target, |end| target.min(end));
        while timeline.audio_pts < target {
            check_playback_control(control)?;
            if self.pending.is_none() && !self.eof {
                match self.source.next(Duration::from_millis(100))? {
                    Some(FrameEvent::Audio(audio)) => {
                        self.pending = Some(audio);
                        self.offset = 0;
                    }
                    Some(FrameEvent::Video(_)) => continue,
                    None if self.source.is_eos() => self.eof = true,
                    None => bail!("external audio source stalled"),
                }
            }
            if let Some(audio) = self.pending.as_ref() {
                let before = timeline.audio_pts;
                send_audio(
                    audio,
                    self.offset,
                    cfg,
                    timeline,
                    output,
                    Some(target),
                    self.fade,
                    false,
                )?;
                self.offset += timeline.audio_pts.saturating_sub(before) as usize;
                if self.offset >= audio.samples() {
                    self.pending = None;
                }
            } else {
                write_silence(cfg, timeline, output, target - timeline.audio_pts)?;
            }
        }
        Ok(())
    }
}

struct VideoCompositor {
    output_width: u32,
    output_height: u32,
    output_fps: u32,
    logo: Option<LogoOverlay>,
    moved_logo: Option<(String, usize, LogoOverlay)>,
    logo_control: LogoControl,
    lshape_control: crate::LShapeControl,
    lshape_scaler: crate::compositor::lshape::LShapeScaler,
    text: Option<TextOverlay>,
    runtime_text: RuntimeTextOverlay,
    last_composited_frame: Option<frame::Video>,
}

impl VideoCompositor {
    fn new(label: &str, cfg: &OutputConfig, timeline: &Timeline, end: Option<i64>) -> Result<Self> {
        Ok(Self {
            output_width: cfg.width,
            output_height: cfg.height,
            output_fps: cfg.fps,
            logo: cfg
                .logo
                .as_ref()
                .map(|logo| LogoOverlay::load(logo, cfg.width, cfg.height))
                .transpose()?,
            moved_logo: None,
            logo_control: cfg.logo_control.clone(),
            lshape_control: cfg.lshape_control.clone(),
            lshape_scaler: Default::default(),
            text: cfg
                .text
                .as_ref()
                .map(|text| {
                    TextOverlay::load(
                        text,
                        label,
                        cfg.width,
                        cfg.height,
                        cfg.fps,
                        timeline.video_pts,
                        0,
                        end,
                    )
                })
                .transpose()?
                .flatten(),
            runtime_text: RuntimeTextOverlay::new(
                label,
                cfg,
                timeline.video_pts,
                timeline.text_pts,
                "runtime text overlay",
            )?,
            last_composited_frame: None,
        })
    }
}

fn open_input<O: FrameOutput>(
    path: &str,
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    control: &PlaybackControl,
    preloaded: Option<Preloaded>,
) -> Result<OpenedInput> {
    let from_preload = preloaded.is_some();
    let mut opening = OpeningCancellation(preloaded.as_ref().map(Preloaded::cancellation));
    let receiver = match preloaded {
        Some(preloaded) => preloaded.hand_over(),
        None if !is_network_url(path) => return Ok(Source::open(path, cfg)?.into()),
        None => {
            let (sender, receiver) = mpsc::sync_channel(1);
            let path = path.to_owned();
            let config = cfg.clone();
            let cancelled = Arc::new(AtomicBool::new(false));
            opening.0 = Some(Arc::clone(&cancelled));
            thread::Builder::new()
                .name("gst-open-input".to_owned())
                .spawn(move || {
                    let result = Source::open_with_options_and_cancel(
                        &path,
                        &config,
                        &Default::default(),
                        &Default::default(),
                        cancelled,
                    )
                    .map(OpenedInput::from)
                    .map_err(|error| format!("opening source: {error:#}"));
                    let _ = sender.send(result);
                })?;
            receiver
        }
    };
    match hold_until_open(path, cfg, timeline, output, control, &receiver)? {
        Ok(input) => {
            opening.0 = None;

            Ok(input)
        }
        Err(error) if from_preload => {
            debug!("Preload failed ({error}); opening source directly");
            drop(opening);
            open_input(path, cfg, timeline, output, control, None)
        }
        Err(error) => Err(source_error(anyhow!(error), is_live_input(path))),
    }
}

/// A skipped/cancelled wait must also stop its detached source-opening worker.
struct OpeningCancellation(Option<Arc<AtomicBool>>);

impl Drop for OpeningCancellation {
    fn drop(&mut self) {
        if let Some(cancelled) = &self.0 {
            cancelled.store(true, Ordering::Release);
        }
    }
}
/// Wait for `receiver`, writing the last picture with silence once per frame
/// time, so a quick source shows nothing held.
fn hold_until_open<O: FrameOutput>(
    path: &str,
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    playback_control: &PlaybackControl,
    receiver: &Receiver<Result<OpenedInput, String>>,
) -> Result<Result<OpenedInput, String>> {
    let frame_time = Duration::from_secs_f64(1.0 / f64::from(cfg.fps.max(1)));
    let started = Instant::now();
    let mut held = None;
    let mut frames = 0_u32;

    loop {
        let due = started + frame_time * (frames + 1);

        match receiver.recv_timeout(due.saturating_duration_since(Instant::now())) {
            Ok(opened) => {
                if frames > 0 {
                    debug!(channel = cfg.channel_id.unwrap_or_default();
                        "Held the picture for {frames} frame(s) while {path} connected"
                    );
                }

                return Ok(opened);
            }
            Err(RecvTimeoutError::Timeout) => {
                check_playback_control(playback_control)?;

                let picture = held.get_or_insert_with(|| {
                    output
                        .held_video_frame()
                        .unwrap_or_else(|| black_video_frame_for_config(cfg))
                });
                let mut frame = reference_video_frame(picture)?;

                frame.set_pts(Some(timeline.video_pts));
                output.encode_video(&frame)?;
                timeline.video_pts += 1;
                synchronize_silence_to_video(cfg, timeline, output, playback_control)?;
                frames += 1;
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Ok(Err(format!("opening {path} stopped unexpectedly")));
            }
        }
    }
}

struct RuntimeTextOverlay {
    state: TextOverlayState,
    revision: u64,
    overlay: Option<TextOverlay>,
    label: String,
    width: u32,
    height: u32,
    fps: u32,
    channel_id: i32,
    context: &'static str,
}

impl RuntimeTextOverlay {
    fn new(
        label: &str,
        cfg: &OutputConfig,
        start_pts: i64,
        scroll_pts: i64,
        context: &'static str,
    ) -> Result<Self> {
        let snapshot = cfg.text_overlay_state.snapshot_at(scroll_pts);
        let overlay = snapshot
            .config
            .as_ref()
            .map(|text| {
                TextOverlay::load(
                    text,
                    label,
                    cfg.width,
                    cfg.height,
                    cfg.fps,
                    start_pts,
                    snapshot.start_pts.unwrap_or(scroll_pts),
                    None,
                )
            })
            .transpose()?
            .flatten();

        Ok(Self {
            state: cfg.text_overlay_state.clone(),
            revision: snapshot.revision,
            overlay,
            label: label.to_string(),
            width: cfg.width,
            height: cfg.height,
            fps: cfg.fps,
            channel_id: cfg.channel_id.unwrap_or_default(),
            context,
        })
    }

    fn update(&mut self, pts: i64, scroll_pts: i64) {
        let snapshot = self.state.snapshot_at(scroll_pts);

        if snapshot.revision == self.revision {
            return;
        }

        self.revision = snapshot.revision;
        self.overlay = snapshot.config.and_then(|config| {
            TextOverlay::load(
                &config,
                &self.label,
                self.width,
                self.height,
                self.fps,
                pts,
                snapshot.start_pts.unwrap_or(scroll_pts),
                None,
            )
            .map_err(|error| {
                debug!(channel = self.channel_id; "failed to render {}: {error:#}", self.context);
                error
            })
            .ok()
            .flatten()
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn encode_composited_frame<O: FrameOutput>(
    mut frame: frame::Video,
    video: &mut VideoCompositor,
    timeline: &mut Timeline,
    media_fade_plan: MediaFadePlan,
    logo_fade_plan: LogoFadePlan,
    output: &mut O,
    decoded_frames: &mut i64,
) -> Result<()> {
    video
        .runtime_text
        .update(timeline.video_pts, timeline.text_pts);
    // The L-shape first: the texts go over it, and the logo gives way to it,
    // going off before the picture shrinks and back after it grew back.
    let mut logo_off = 0.0;

    if let Some((graphic, progress)) = video
        .lshape_control
        .frame(logo_fade_plan.lshape_level(timeline.video_pts))
    {
        let (gone, picture) = crate::compositor::lshape::lshape_phases(progress);
        logo_off = gone;

        if let Err(error) = crate::compositor::lshape::apply_lshape(
            &mut frame,
            &graphic,
            picture,
            &mut video.lshape_scaler,
        ) {
            debug!("L-shape not drawn: {error}");
        }
    }

    let video_opacity = media_fade_plan.video_opacity_at(timeline.video_pts);
    let changes_pixels = video_frame_needs_write(
        video_opacity,
        video.logo.is_some() || video.logo_control.has_replacement(),
        video.text.is_some(),
        video.runtime_text.overlay.is_some(),
    );

    if changes_pixels {
        make_video_frame_writable(&mut frame)?;
    }
    apply_video_fade(&mut frame, video_opacity);
    apply_overlays(
        &mut frame,
        video,
        timeline,
        logo_fade_plan,
        logo_off,
        output,
    );
    frame.set_pts(Some(timeline.video_pts));
    output.encode_video(&frame)?;
    let output_position_ms = frame
        .pts()
        .unwrap_or(timeline.video_pts)
        .saturating_mul(1_000)
        / i64::from(video.output_fps);
    output.advance_vtt_subtitles(output_position_ms)?;
    video.last_composited_frame = Some(frame);
    timeline.video_pts += 1;
    *decoded_frames += 1;
    Ok(())
}

fn video_frame_needs_write(
    opacity: f32,
    has_logo: bool,
    has_static_text: bool,
    has_runtime_text: bool,
) -> bool {
    opacity < 1.0 || has_logo || has_static_text || has_runtime_text
}

fn apply_overlays(
    frame: &mut frame::Video,
    video: &mut VideoCompositor,
    timeline: &mut Timeline,
    logo_fade_plan: LogoFadePlan,
    logo_off: f64,
    output: &mut impl FrameOutput,
) {
    let opacity = logo_fade_plan.opacity_at(timeline.video_pts, timeline.logo_opacity);
    timeline.logo_opacity = opacity;

    // Hidden or swapped from outside: the clip's own fade still applies, so
    // a commercial keeps any logo off.
    let logo = video.logo_control.frame();
    // The logo is off for the L-shape: it goes before the picture shrinks
    // and comes back after the picture grew back.
    let opacity = opacity
        * logo.visibility
        * logo_fade_plan.gap_factor(timeline.video_pts)
        * (1.0 - logo_off.clamp(0.0, 1.0));
    let mut incoming = 1.0;

    // The clip's own logo takes the configured one's place.
    let base = logo.clip_image.as_deref().or(video.logo.as_ref());
    let base_id = base.map(|overlay| overlay as *const LogoOverlay as usize);

    // Moved from outside: that logo drawn at the new place, made once for
    // each place and image.
    match (&logo.position, base) {
        (Some(position), Some(base))
            if video
                .moved_logo
                .as_ref()
                .is_none_or(|(at, id, _)| at != position || Some(*id) != base_id) =>
        {
            video.moved_logo = base
                .moved_to(position, video.output_width, video.output_height)
                .ok()
                .map(|moved| (position.clone(), base_id.unwrap_or_default(), moved));
        }
        (None, _) | (_, None) => video.moved_logo = None,
        _ => {}
    }
    let configured = video
        .moved_logo
        .as_ref()
        .map(|(_, _, moved)| moved)
        .or(base);

    // Another image took the logo's place: the old one fades as the new one
    // comes up.
    if let Some((image, left)) = &logo.outgoing {
        if let Some(old) = image.as_deref().or(configured) {
            draw_logo(frame, old, opacity * left, output);
        }
        incoming = 1.0 - left;
    }

    if let Some(current) = logo.replacement.as_deref().or(configured) {
        draw_logo(frame, current, opacity * incoming, output);
    }

    if let Some(text) = &mut video.text {
        let (width, height) = text.dimensions();
        benchmark::measure_overlay(Stage::TextStatic, width, height, || {
            text.blend(frame, timeline.video_pts, timeline.text_pts);
        });
    }

    if let Some(text) = &mut video.runtime_text.overlay {
        let (width, height) = text.dimensions();
        benchmark::measure_overlay(Stage::TextRuntime, width, height, || {
            text.blend(frame, timeline.video_pts, timeline.text_pts);
        });
    }

    timeline.text_pts += 1;
}

fn draw_logo(
    frame: &mut frame::Video,
    logo: &LogoOverlay,
    opacity: f64,
    output: &mut impl FrameOutput,
) {
    if output.benchmarks_logo_overlay() {
        benchmark::measure_overlay(Stage::LogoOverlay, logo.width, logo.height, || {
            output.apply_logo_overlay(frame, logo, opacity);
        });
    } else {
        output.apply_logo_overlay(frame, logo, opacity);
    }
}

/// Fades the decoded video toward limited-range YUV black before overlays are
/// blended, so logos and text remain fully visible during a clipped end.
fn apply_video_fade(frame: &mut frame::Video, opacity: f32) {
    if opacity >= 1.0 {
        return;
    }

    for plane in 0..frame.planes() {
        let target = if plane == 0 { 16.0 } else { 128.0 };
        let width = if plane == 0 {
            frame.width()
        } else {
            frame.width() / 2
        } as usize;
        let height = if plane == 0 {
            frame.height()
        } else {
            frame.height() / 2
        } as usize;
        let stride = frame.stride(plane);
        let data = frame.data_mut(plane);

        for row in data.chunks_mut(stride).take(height) {
            for sample in &mut row[..width] {
                *sample = (*sample as f32 * opacity + target * (1.0 - opacity)).round() as u8;
            }
        }
    }
}

fn apply_audio_fade(frame: &mut frame::Audio, start_pts: i64, fade: MediaFadePlan) {
    if fade.audio_end_pts.is_none() {
        return;
    }

    for plane in 0..frame.planes() {
        for (sample_index, sample) in frame.plane_mut::<f32>(plane).iter_mut().enumerate() {
            *sample *= fade.audio_gain_at(start_pts + sample_index as i64);
        }
    }
}

pub(crate) fn write_fallback<O: FrameOutput>(
    label: &str,
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    duration: f64,
    playback_control: &PlaybackControl,
) -> Result<()> {
    let video_end = timeline.video_pts + (duration * f64::from(cfg.fps)).ceil() as i64;
    let audio_end = timeline.audio_pts + (duration * f64::from(cfg.sample_rate)).ceil() as i64;
    let mut overlays = FallbackOverlays::new(
        label,
        cfg,
        timeline.video_pts,
        timeline.text_pts,
        Some(video_end),
    )?;

    while timeline.video_pts < video_end || timeline.audio_pts < audio_end {
        check_playback_control(playback_control)?;
        let video_time = timeline.video_pts as f64 / f64::from(cfg.fps);
        let audio_time = timeline.audio_pts as f64 / f64::from(cfg.sample_rate);

        if timeline.video_pts < video_end
            && (timeline.audio_pts >= audio_end || video_time <= audio_time)
        {
            write_black_frames(cfg, timeline, output, 1, Some(&mut overlays))?;
        } else {
            let remaining = (audio_end - timeline.audio_pts) as usize;
            let samples = remaining.min(output.audio_frame_size().max(1));
            write_silence_frame(cfg, timeline, output, samples)?;
        }
    }

    synchronize_timeline(cfg, timeline, output, None)
}

fn synchronize_timeline<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    last_video_frame: Option<&frame::Video>,
) -> Result<()> {
    let (video_frames, audio_samples) = padding_to_sync(
        timeline.video_pts,
        timeline.audio_pts,
        cfg.fps,
        cfg.sample_rate,
    )?;

    if video_frames > 0 {
        trace!(
            channel = cfg.channel_id.unwrap_or_default();
            "padding video with {video_frames} frame(s) ({:.6} s) to synchronize the timeline",
            video_frames as f64 / f64::from(cfg.fps)
        );
    }
    write_padding_video_frames(cfg, timeline, output, video_frames, last_video_frame)?;

    if audio_samples > 0 {
        trace!(
            channel = cfg.channel_id.unwrap_or_default();
            "padding audio with {audio_samples} silent sample(s) ({:.6} s) to synchronize the timeline",
            audio_samples as f64 / f64::from(cfg.sample_rate)
        );
    }
    write_silence(cfg, timeline, output, audio_samples)
}

/// Keep audio packets interleaved with video for video-only inputs. Deferring
/// all silence until EOF puts its timestamps behind already-muxed video and
/// can make a realtime muxer wait indefinitely for an interleavable packet.
fn synchronize_silence_to_video<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    playback_control: &PlaybackControl,
) -> Result<()> {
    check_playback_control(playback_control)?;
    let target_audio_pts = div_ceil(
        i128::from(timeline.video_pts) * i128::from(cfg.sample_rate),
        i128::from(cfg.fps),
    ) as i64;
    let samples = target_audio_pts.saturating_sub(timeline.audio_pts);

    if samples > 0 {
        write_silence(cfg, timeline, output, samples)?;
    }
    check_playback_control(playback_control)
}

/// A live input may keep one track active while the other waits for data.
/// Bound their separation so the persistent muxer can continue interleaving.
/// Late source frames are trimmed against the timeline already sent.
fn synchronize_live_tracks<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    end_video: Option<i64>,
    end_audio: Option<i64>,
    picture: Option<&frame::Video>,
) -> Result<()> {
    // Allow one video frame and 100 ms of audio jitter before synthesizing
    // missing media. Healthy, interleaved source tracks need no padding.
    let video_target = (i128::from(timeline.audio_pts) * i128::from(cfg.fps)
        / i128::from(cfg.sample_rate)) as i64
        - 1;
    let video_target = end_video.map_or(video_target, |end| video_target.min(end));
    let video_frames = video_target.saturating_sub(timeline.video_pts).max(0);
    write_padding_video_frames(cfg, timeline, output, video_frames, picture)?;

    let audio_target = (i128::from(timeline.video_pts) * i128::from(cfg.sample_rate)
        / i128::from(cfg.fps)) as i64
        - i64::from(cfg.sample_rate / 10);
    let audio_target = end_audio.map_or(audio_target, |end| audio_target.min(end));
    let samples = audio_target.saturating_sub(timeline.audio_pts).max(0);

    write_silence(cfg, timeline, output, samples)
}

fn synchronize_after_skip<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    last_video_frame: Option<&frame::Video>,
) -> Result<()> {
    let (video_frames, audio_samples) = padding_to_sync(
        timeline.video_pts,
        timeline.audio_pts,
        cfg.fps,
        cfg.sample_rate,
    )?;
    let target_video_pts = timeline.video_pts + video_frames;
    let target_audio_pts = timeline.audio_pts + audio_samples;

    if output.reset_after_skip(target_video_pts, target_audio_pts)? {
        timeline.video_pts = target_video_pts;
        timeline.audio_pts = target_audio_pts;

        return Ok(());
    }

    synchronize_timeline(cfg, timeline, output, last_video_frame)
}

fn padding_to_sync(
    video_pts: i64,
    audio_pts: i64,
    fps: u32,
    sample_rate: u32,
) -> Result<(i64, i64)> {
    if fps == 0 || sample_rate == 0 {
        return Err(anyhow!("fps and sample rate must be greater than zero"));
    }

    let fps = i128::from(fps);
    let sample_rate = i128::from(sample_rate);
    let mut video_end = i128::from(video_pts);
    let audio_end = i128::from(audio_pts);
    let mut video_padding = 0_i128;
    let mut audio_padding = 0_i128;

    if video_end * sample_rate < audio_end * fps {
        let target = div_ceil(audio_end * fps, sample_rate);
        video_padding = target - video_end;
        video_end = target;
    }

    if audio_end * fps < video_end * sample_rate {
        let target = div_ceil(video_end * sample_rate, fps);
        audio_padding = target - audio_end;
    }

    let video_padding =
        i64::try_from(video_padding).map_err(|_| anyhow!("video padding exceeds i64"))?;
    let audio_padding =
        i64::try_from(audio_padding).map_err(|_| anyhow!("audio padding exceeds i64"))?;

    Ok((video_padding, audio_padding))
}

fn div_ceil(numerator: i128, denominator: i128) -> i128 {
    (numerator + denominator - 1) / denominator
}

fn write_black_frames<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    frames: i64,
    mut overlays: Option<&mut FallbackOverlays>,
) -> Result<()> {
    for _ in 0..frames {
        let mut black = black_video_frame_for_config(cfg);

        if let Some(overlays) = overlays.as_mut() {
            overlays.apply(&mut black, timeline);
        }
        black.set_pts(Some(timeline.video_pts));
        output.encode_video(&black)?;
        timeline.video_pts += 1;
    }

    Ok(())
}

struct FallbackOverlays {
    text: Option<TextOverlay>,
    runtime_text: RuntimeTextOverlay,
}

impl FallbackOverlays {
    fn new(
        label: &str,
        cfg: &OutputConfig,
        fade_start_pts: i64,
        scroll_pts: i64,
        end_pts: Option<i64>,
    ) -> Result<Self> {
        let runtime_text = RuntimeTextOverlay::new(
            label,
            cfg,
            fade_start_pts,
            scroll_pts,
            "fallback runtime text overlay",
        )?;

        Ok(Self {
            text: cfg
                .text
                .as_ref()
                .map(|text| {
                    TextOverlay::load(
                        text,
                        label,
                        cfg.width,
                        cfg.height,
                        cfg.fps,
                        fade_start_pts,
                        0,
                        end_pts,
                    )
                })
                .transpose()?
                .flatten(),
            runtime_text,
        })
    }

    fn apply(&mut self, frame: &mut frame::Video, timeline: &mut Timeline) {
        if let Some(text) = &mut self.text {
            text.blend(frame, timeline.video_pts, timeline.text_pts);
        }
        self.runtime_text
            .update(timeline.video_pts, timeline.text_pts);
        if let Some(text) = &mut self.runtime_text.overlay {
            text.blend(frame, timeline.video_pts, timeline.text_pts);
        }

        timeline.text_pts += 1;
    }
}

fn write_padding_video_frames<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    frames: i64,
    last_video_frame: Option<&frame::Video>,
) -> Result<()> {
    if let Some(last_video_frame) = last_video_frame {
        for _ in 0..frames {
            // The frame is already fully composited. Padding only changes its
            // header PTS, so all duplicates can safely share the pixel buffer.
            let mut frame = reference_video_frame(last_video_frame)?;
            frame.set_pts(Some(timeline.video_pts));
            output.encode_video(&frame)?;
            timeline.video_pts += 1;
        }

        Ok(())
    } else {
        write_black_frames(cfg, timeline, output, frames, None)
    }
}

fn black_video_frame_for_config(cfg: &OutputConfig) -> frame::Video {
    black_video_frame(cfg.width, cfg.height)
}

pub(crate) fn black_video_frame(width: u32, height: u32) -> frame::Video {
    let mut frame = frame::Video::new(Pixel::YUV420P, width, height);
    fill_plane(&mut frame, 0, 16);
    fill_plane(&mut frame, 1, 128);
    fill_plane(&mut frame, 2, 128);
    frame
}

fn fill_plane(frame: &mut frame::Video, plane: usize, value: u8) {
    let height = if plane == 0 {
        frame.height()
    } else {
        frame.height() / 2
    } as usize;
    let width = if plane == 0 {
        frame.width()
    } else {
        frame.width() / 2
    } as usize;
    let stride = frame.stride(plane);
    let data = frame.data_mut(plane);

    for y in 0..height {
        let start = y * stride;
        data[start..start + width].fill(value);
    }
}

fn write_silence_frame<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    samples: usize,
) -> Result<()> {
    let mut frame = frame::Audio::new(
        Sample::F32(format::sample::Type::Planar),
        samples,
        ChannelLayout::STEREO,
    );
    frame.set_rate(cfg.sample_rate);
    frame.set_pts(Some(timeline.audio_pts));

    for plane in 0..frame.planes() {
        frame.plane_mut::<f32>(plane).fill(0.0);
    }
    output.encode_audio(&frame)?;
    timeline.audio_pts += samples as i64;

    Ok(())
}

fn write_silence<O: FrameOutput>(
    cfg: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    mut samples: i64,
) -> Result<()> {
    if samples > 0 && output.pad_audio(samples)? {
        timeline.audio_pts += samples;

        return Ok(());
    }

    let frame_samples = output.audio_frame_size().max(1);

    while samples > 0 {
        let current_samples = samples.min(frame_samples as i64) as usize;
        write_silence_frame(cfg, timeline, output, current_samples)?;
        samples -= current_samples as i64;
    }

    Ok(())
}

#[cfg(test)]
mod live_interleave_tests {
    use super::*;

    #[derive(Default)]
    struct Capture {
        video: Vec<i64>,
        audio: Vec<(i64, usize)>,
    }

    impl FrameOutput for Capture {
        fn audio_frame_size(&self) -> usize {
            1024
        }

        fn encode_video(&mut self, frame: &frame::Video) -> Result<()> {
            self.video.push(frame.pts().unwrap());

            Ok(())
        }

        fn encode_audio(&mut self, frame: &frame::Audio) -> Result<()> {
            assert!(frame.plane::<f32>(0).iter().all(|sample| *sample == 0.0));
            self.audio.push((frame.pts().unwrap(), frame.samples()));

            Ok(())
        }
    }

    #[test]
    fn missing_live_video_holds_a_picture_while_audio_continues() {
        let config = OutputConfig::new(64, 48, 25, 48_000);
        let mut timeline = Timeline::at(0, 9600);
        let mut output = Capture::default();
        synchronize_live_tracks(&config, &mut timeline, &mut output, None, None, None).unwrap();

        assert_eq!(output.video, [0, 1, 2, 3]);
        assert_eq!(timeline.video_pts, 4);
        assert_eq!(timeline.audio_pts, 9600);
        assert!(output.audio.is_empty());
    }

    #[test]
    fn missing_live_audio_inserts_silence_without_exceeding_the_event_end() {
        let config = OutputConfig::new(64, 48, 25, 48_000);
        let mut timeline = Timeline::at(25, 0);
        let mut output = Capture::default();
        synchronize_live_tracks(
            &config,
            &mut timeline,
            &mut output,
            Some(25),
            Some(20_000),
            None,
        )
        .unwrap();

        assert!(output.video.is_empty());
        assert_eq!(timeline.audio_pts, 20_000);
        assert_eq!(output.audio.first().unwrap().0, 0);
        assert_eq!(
            output
                .audio
                .iter()
                .map(|(_, samples)| samples)
                .sum::<usize>(),
            20_000
        );
        assert!(
            output
                .audio
                .windows(2)
                .all(|frames| frames[0].0 + frames[0].1 as i64 == frames[1].0)
        );
    }

    #[test]
    fn healthy_live_tracks_are_not_padded() {
        let config = OutputConfig::new(64, 48, 25, 48_000);
        let mut timeline = Timeline::at(25, 48_000);
        let mut output = Capture::default();
        synchronize_live_tracks(&config, &mut timeline, &mut output, None, None, None).unwrap();

        assert!(output.video.is_empty());
        assert!(output.audio.is_empty());
    }
}
