use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

#[cfg(feature = "tokio")]
use std::{
    sync::mpsc,
    thread::{self, JoinHandle},
};

#[cfg(feature = "tokio")]
use anyhow::Context;
use anyhow::{Result, anyhow};

#[cfg(feature = "tokio")]
use tokio::sync::oneshot;

mod analysis;
mod audio_mixer;
mod benchmark;
mod compositor;
mod convert;
pub(crate) mod gstreamer;
mod input;
#[cfg(feature = "desktop-base")]
mod loudness_preview;
mod output;
mod playback_control;
mod playout;
mod preview_quality;
mod utils;

pub use analysis::audio_level::{AudioFrameCallback, AudioLevel, AudioLevelCallback};
pub use analysis::loudness::{LoudnessMeterControl, LoudnessMetrics};
use audio_mixer::program::{ProgramAudioOutput, ProgramAudioState};
pub use audio_mixer::{
    AudioEffectsControl, BufferedLoudnessAnalysis, LiveDynamicsProcessor, LiveLoudnessConfig,
    LiveLoudnessControl, LiveLoudnessMeasurement, LiveLoudnessMetrics, LiveLoudnessProcessor,
    LoudnessScope,
};
pub use compositor::logo::{
    LogoControl, LogoState, LogoTransition, LogoTransitions, MAX_LOGO_TRANSITION_SECONDS,
};
pub use compositor::lshape::{
    LSHAPE_LOGO_SECONDS, LSHAPE_MAX_SCALE, LSHAPE_MIN_SCALE, LSHAPE_SECONDS, LSHAPE_SWITCH_SECONDS,
    LShapeConfig, LShapeControl, LShapeCorner, LShapePlacement, lshape_phases,
};
pub use convert::{ConvertTarget, MediaShape, convert_to_output, probe_shape, video_matches};
use input::preload::Preloaded;
pub use input::preload::{PreloadSignal, Preloader};
#[cfg(all(feature = "desktop-base", feature = "tokio"))]
pub use output::desktop::thread::run_on_main_thread as run_desktop_on_main_thread;
pub use output::link::{LinkState, LinkStatus, OutputLink};
pub use output::resolved_variant_playlist_path;
pub use output::scte35::{
    BreakInfo, BreakSource, BreakState, ClipBreaks, DEFAULT_SCTE35_PID,
    DEFAULT_SCTE35_PREROLL_SECONDS, Scte35Config, SpliceControl, SpliceError, SpliceStatus,
};
use output::{FrameOutput, Output, PlaybackStopped};
pub use playback_control::{NavigationBlocked, PlaybackControl, PlaylistNavigation};
use playout::{PlaybackRestart, PlaybackSkipped, Timeline, write_fallback};
pub use preview_quality::{LightPreview, transcode_light_preview};
pub use utils::{
    clock,
    config::{
        AudioOptions, DesktopControlCallback, DesktopControlCommand, HlsSubtitle, HlsVariant,
        LogLevel, LogoConfig, OutputConfig, OutputSize, RecordingConfig, RecordingEncodeConfig,
        RgbaColor, StreamType, TextBackgroundConfig, TextConfig, TextOverlayState, TextPosition,
        TextScroll, TextWeight, VideoOptionChoice, VideoOptionKind, VideoOptionSpec,
        VideoOptionVisibility, VideoOptions, audio_codec_uses_bitrate, validate_audio_options,
        validate_output_metadata, validate_output_protocol_options, validate_video_options,
        video_codec_uses_bitrate, video_option_defaults, video_option_specs,
    },
    ffmpeg_capabilities::{
        FfmpegCapabilities, FfmpegCodec, FfmpegFeatureSet, FfmpegMediaType, FfmpegMuxer,
        FfmpegOutputTarget, ffmpeg_capabilities, validate_muxer_options,
    },
    helper::is_live_input,
    logging,
    media_info::{
        AudioStream as EngineAudioStream, MediaInfo, MediaProbe as EngineMediaProbe, ProbeFormat,
        SilenceDetection, VideoStream as EngineVideoStream, detect_audio_silence, print_media_info,
        probe_media, probe_media_info,
    },
};

pub fn available_font_families() -> Vec<String> {
    compositor::text::available_font_families()
}

/// Sets the report interval for the optional `processing-bench` feature.
/// Has no effect when that feature is disabled.
pub fn set_processing_bench_interval(interval: Duration) {
    benchmark::set_report_interval(interval);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipResult {
    Played,
    Skipped,
    LiveEnded,
    Fallback { reason: String },
    Stopped,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogoFade {
    pub fade_in: bool,
    pub fade_out: bool,
    /// Keep the logo off for the whole clip (a commercial). The next clip that
    /// shows the logo fades it back in.
    pub hidden: bool,
    /// Stretches of the clip that keep the logo off, such as a commercial
    /// break edited into a programme.
    pub gaps: LogoGaps,
    /// Stretches of the clip shown in the L-shape.
    pub lshape: LogoGaps,
}

/// The most stretches without logo a clip can have.
pub const MAX_LOGO_GAPS: usize = 16;

/// Stretches of a clip without logo, in milliseconds of the source, sorted.
/// A fixed array keeps it `Copy`, like the rest of [`LogoFade`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogoGaps {
    spans: [(i64, i64); MAX_LOGO_GAPS],
    len: usize,
}

impl LogoGaps {
    /// From `(from, to)` seconds of the source. Empty, reversed and
    /// non-finite stretches are left out, overlapping ones merged, and only
    /// the first [`MAX_LOGO_GAPS`] kept.
    pub fn new(spans: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let mut valid: Vec<(i64, i64)> = spans
            .into_iter()
            .filter(|(from, to)| from.is_finite() && to.is_finite() && to > from && *to > 0.0)
            .map(|(from, to)| {
                (
                    (from.max(0.0) * 1000.0).round() as i64,
                    (to * 1000.0).round() as i64,
                )
            })
            .collect();
        let mut gaps = Self::default();

        valid.sort_unstable();

        for (from, to) in valid {
            match gaps.len.checked_sub(1).map(|last| &mut gaps.spans[last]) {
                Some(last) if from <= last.1 => last.1 = last.1.max(to),
                _ if gaps.len < MAX_LOGO_GAPS => {
                    gaps.spans[gaps.len] = (from, to);
                    gaps.len += 1;
                }
                _ => break,
            }
        }

        gaps
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The stretches as `(from, to)` milliseconds of the source.
    pub fn iter(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.spans[..self.len].iter().copied()
    }
}

pub struct Playout {
    config: OutputConfig,
    output: Output,
    program_audio: ProgramAudioState,
    timeline: Timeline,
    fallback_duration: f64,
    playback_control: PlaybackControl,
    preloader: Preloader,
}

#[derive(Clone)]
pub struct HlsHealth {
    last_muxed_at: Arc<Mutex<Instant>>,
}

impl HlsHealth {
    pub(crate) fn new() -> Self {
        Self {
            last_muxed_at: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub fn last_muxed_age(&self) -> Duration {
        self.last_muxed_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .elapsed()
    }

    pub(crate) fn mark_muxed(&self) {
        *self
            .last_muxed_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Instant::now();
    }
}

#[derive(Clone, Copy)]
struct PlayOptions<'a> {
    seek_seconds: Option<f64>,
    duration_seconds: Option<f64>,
    external_audio_path: Option<&'a str>,
    subtitles_media_path: Option<&'a str>,
    logo_fade: LogoFade,
}

#[cfg(feature = "tokio")]
pub struct AsyncPlayout {
    commands: mpsc::Sender<AsyncCommand>,
    completion: Option<WorkerCompletion>,
    playback_control: PlaybackControl,
    preloader: Preloader,
    hls_health: Option<HlsHealth>,
    /// The L-shape of the output, to set each clip's own graphic.
    lshape_control: LShapeControl,
    /// The logo of the output, to set each clip's own place.
    logo_control: LogoControl,
}

/// How to wait for the playout worker to finish.
#[cfg(feature = "tokio")]
enum WorkerCompletion {
    Thread(JoinHandle<()>),
}

#[cfg(feature = "tokio")]
impl AsyncPlayout {
    pub async fn open(
        output_url: impl Into<String>,
        config: OutputConfig,
        fallback_duration: f64,
    ) -> Result<Self> {
        let output_url = output_url.into();
        Self::open_with(move || Playout::open(&output_url, config, fallback_duration)).await
    }

    pub async fn open_stream(
        output_url: impl Into<String>,
        config: OutputConfig,
        fallback_duration: f64,
    ) -> Result<Self> {
        let output_url = output_url.into();
        Self::open_with(move || Playout::open_stream(&output_url, config, fallback_duration)).await
    }

    pub async fn open_hls(
        playlist: impl Into<String>,
        config: OutputConfig,
        fallback_duration: f64,
        hls_variants: Vec<HlsVariant>,
        hls_subtitle: Option<HlsSubtitle>,
        hls_segment_seconds: u32,
        hls_list_size: u32,
    ) -> Result<Self> {
        let playlist = playlist.into();
        let hls_health = HlsHealth::new();
        let worker_health = hls_health.clone();
        let mut playout = Self::open_with(move || {
            Playout::open_hls_with_health(
                &playlist,
                config,
                fallback_duration,
                &hls_variants,
                hls_subtitle,
                hls_segment_seconds,
                hls_list_size,
                worker_health,
            )
        })
        .await?;
        playout.hls_health = Some(hls_health);

        Ok(playout)
    }

    #[cfg(feature = "desktop-base")]
    pub async fn open_desktop(config: OutputConfig, fallback_duration: f64) -> Result<Self> {
        Self::open_with(move || Playout::open_desktop(config, fallback_duration)).await
    }

    async fn open_with<F>(open: F) -> Result<Self>
    where
        F: FnOnce() -> Result<Playout> + Send + 'static,
    {
        let (commands, command_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let playback_control = PlaybackControl::default();
        let worker_playback_control = playback_control.clone();
        let preloader = Preloader::default();
        let worker_preloader = preloader.clone();

        let worker = thread::spawn(move || match open() {
            Ok(mut playout) => {
                playout.playback_control = worker_playback_control;
                worker_preloader.configure(&playout.config);
                playout.preloader = worker_preloader;
                let _ = ready_tx.send(Ok((
                    playout.config.lshape_control.clone(),
                    playout.config.logo_control.clone(),
                )));
                run_async_playout_worker(playout, command_rx);
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        });

        // Own the cancellation guard before the await: opening can itself be cancelled.
        let mut playout = Self {
            commands,
            completion: Some(WorkerCompletion::Thread(worker)),
            playback_control,
            preloader,
            hls_health: None,
            lshape_control: LShapeControl::default(),
            logo_control: LogoControl::default(),
        };
        (playout.lshape_control, playout.logo_control) = ready_rx
            .await
            .context("playout worker stopped during open")??;
        Ok(playout)
    }

    /// The L-shape of the output: set the graphic of the clip about to play
    /// here, before playing it.
    pub fn lshape_control(&self) -> LShapeControl {
        self.lshape_control.clone()
    }

    /// The logo of the output: set where the clip about to play puts it,
    /// here, before playing it.
    pub fn logo_control(&self) -> LogoControl {
        self.logo_control.clone()
    }

    pub fn playback_control(&self) -> PlaybackControl {
        self.playback_control.clone()
    }

    pub fn hls_health(&self) -> Option<HlsHealth> {
        self.hls_health.clone()
    }

    /// Opens sources ahead of their turn; a play of the same path takes it.
    pub fn preloader(&self) -> Preloader {
        self.preloader.clone()
    }

    pub async fn play(&self, path: impl Into<String>) -> Result<ClipResult> {
        self.play_with_seek(path, None).await
    }

    pub async fn play_with_seek(
        &self,
        path: impl Into<String>,
        seek_seconds: Option<f64>,
    ) -> Result<ClipResult> {
        let path = path.into();
        let (response, result) = oneshot::channel();
        self.commands
            .send(AsyncCommand::Play {
                path,
                seek_seconds,
                duration_seconds: None,
                external_audio_path: None,
                subtitles_media_path: None,
                logo_fade: Box::default(),
                playout_rate: 1.0,
                response,
            })
            .map_err(|_| anyhow!("playout worker stopped"))?;

        result.await.context("playout worker stopped during play")?
    }

    pub async fn play_with_timing_and_logo_fade(
        &self,
        path: impl Into<String>,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        subtitles_media_path: Option<String>,
        logo_fade: LogoFade,
    ) -> Result<ClipResult> {
        self.play_with_timing_logo_fade_and_rate(
            path,
            seek_seconds,
            duration_seconds,
            subtitles_media_path,
            logo_fade,
            1.0,
        )
        .await
    }

    pub async fn play_with_timing_logo_fade_and_rate(
        &self,
        path: impl Into<String>,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        subtitles_media_path: Option<String>,
        logo_fade: LogoFade,
        playout_rate: f64,
    ) -> Result<ClipResult> {
        self.play_with_timing_logo_fade_rate_and_audio(
            path,
            seek_seconds,
            duration_seconds,
            None,
            subtitles_media_path,
            logo_fade,
            playout_rate,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn play_with_timing_logo_fade_rate_and_audio(
        &self,
        path: impl Into<String>,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        external_audio_path: Option<String>,
        subtitles_media_path: Option<String>,
        logo_fade: LogoFade,
        playout_rate: f64,
    ) -> Result<ClipResult> {
        let path = path.into();
        let (response, result) = oneshot::channel();
        self.commands
            .send(AsyncCommand::Play {
                path,
                seek_seconds,
                duration_seconds,
                external_audio_path,
                subtitles_media_path,
                logo_fade: Box::new(logo_fade),
                playout_rate,
                response,
            })
            .map_err(|_| anyhow!("playout worker stopped"))?;

        result.await.context("playout worker stopped during play")?
    }

    pub async fn finish(mut self) -> Result<()> {
        self.playback_control.request_shutdown();
        let (response, result) = oneshot::channel();
        self.commands
            .send(AsyncCommand::Finish { response })
            .map_err(|_| anyhow!("playout worker stopped"))?;

        let finish_result = result
            .await
            .context("playout worker stopped during finish")?;

        if let Some(WorkerCompletion::Thread(worker)) = self.completion.take()
            && worker.join().is_err()
            && finish_result.is_ok()
        {
            return Err(anyhow!("playout worker panicked during finish"));
        }

        finish_result
    }
}

#[cfg(feature = "tokio")]
impl Drop for AsyncPlayout {
    fn drop(&mut self) {
        if self.completion.is_some() {
            self.playback_control.request_shutdown();
            let (response, _) = oneshot::channel();
            let _ = self.commands.send(AsyncCommand::Finish { response });
        }
    }
}

#[cfg(feature = "tokio")]
enum AsyncCommand {
    Play {
        path: String,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        external_audio_path: Option<String>,
        subtitles_media_path: Option<String>,
        /// Boxed: its stretches without logo would make every command large.
        logo_fade: Box<LogoFade>,
        playout_rate: f64,
        response: oneshot::Sender<Result<ClipResult>>,
    },
    Finish {
        response: oneshot::Sender<Result<()>>,
    },
}

#[cfg(feature = "tokio")]
fn run_async_playout_worker(mut playout: Playout, commands: mpsc::Receiver<AsyncCommand>) {
    let mut finish_response = None;

    while let Ok(command) = commands.recv() {
        if playout.playback_control.is_shutdown() && !matches!(command, AsyncCommand::Finish { .. })
        {
            continue;
        }

        match command {
            AsyncCommand::Play {
                path,
                seek_seconds,
                duration_seconds,
                external_audio_path,
                subtitles_media_path,
                logo_fade,
                playout_rate,
                response,
            } => {
                let result = playout.play_timed(
                    &path,
                    seek_seconds,
                    duration_seconds,
                    external_audio_path.as_deref(),
                    subtitles_media_path.as_deref(),
                    *logo_fade,
                    playout_rate,
                );
                let _ = response.send(result);
                // A closed desktop window reports `Stopped`, but the owner
                // still sends `Finish` immediately afterwards. Keep this
                // worker alive so that command can explicitly release the
                // window and its WGPU resources before process shutdown.
            }
            AsyncCommand::Finish { response } => {
                finish_response = Some(response);
                break;
            }
        }
    }
    // Finalize the persistent output after cancellation or disconnection.
    let channel_id = playout.config.channel_id.unwrap_or_default();
    let result = playout.finish();

    if let Some(response) = finish_response {
        let _ = response.send(result);
    } else if let Err(error) = result {
        log::warn!(channel = channel_id; "failed to finalize abandoned playout: {error:#}");
    }
}

impl Playout {
    pub fn open(output_url: &str, config: OutputConfig, fallback_duration: f64) -> Result<Self> {
        Self::validate_fallback_duration(fallback_duration)?;
        init_media(&config)?;
        let output = Output::open(output_url, &config)?;

        Ok(Self::with_output(config, output, fallback_duration))
    }

    pub fn open_stream(
        output_url: &str,
        config: OutputConfig,
        fallback_duration: f64,
    ) -> Result<Self> {
        Self::validate_fallback_duration(fallback_duration)?;
        init_media(&config)?;
        let output = Output::open_stream(output_url, &config)?;

        Ok(Self::with_output(config, output, fallback_duration))
    }

    #[cfg(feature = "desktop-base")]
    pub fn open_desktop(config: OutputConfig, fallback_duration: f64) -> Result<Self> {
        Self::validate_fallback_duration(fallback_duration)?;
        init_media(&config)?;
        let output = Output::open_desktop(&config)?;

        Ok(Self::with_output(config, output, fallback_duration))
    }

    pub fn open_hls(
        playlist: &str,
        config: OutputConfig,
        fallback_duration: f64,
        hls_variants: &[HlsVariant],
        hls_subtitle: Option<HlsSubtitle>,
        hls_segment_seconds: u32,
        hls_list_size: u32,
    ) -> Result<Self> {
        Self::open_hls_with_health(
            playlist,
            config,
            fallback_duration,
            hls_variants,
            hls_subtitle,
            hls_segment_seconds,
            hls_list_size,
            HlsHealth::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn open_hls_with_health(
        playlist: &str,
        config: OutputConfig,
        fallback_duration: f64,
        hls_variants: &[HlsVariant],
        hls_subtitle: Option<HlsSubtitle>,
        hls_segment_seconds: u32,
        hls_list_size: u32,
        hls_health: HlsHealth,
    ) -> Result<Self> {
        Self::validate_fallback_duration(fallback_duration)?;
        init_media(&config)?;
        let output = Output::open_hls(
            playlist,
            &config,
            hls_variants,
            hls_subtitle,
            hls_segment_seconds,
            hls_list_size,
            hls_health,
        )?;

        Ok(Self::with_output(config, output, fallback_duration))
    }

    fn validate_fallback_duration(fallback_duration: f64) -> Result<()> {
        if !fallback_duration.is_finite() || fallback_duration <= 0.0 {
            return Err(anyhow!("fallback duration must be a positive number"));
        }

        Ok(())
    }

    fn with_output(config: OutputConfig, output: Output, fallback_duration: f64) -> Self {
        #[cfg(feature = "desktop-base")]
        if !output.is_desktop() {
            benchmark::start(config.channel_id);
        }
        #[cfg(not(feature = "desktop-base"))]
        benchmark::start(config.channel_id);

        let preloader = Preloader::default();
        preloader.configure(&config);

        Self {
            config,
            output,
            program_audio: ProgramAudioState::default(),
            timeline: Timeline::new(),
            fallback_duration,
            playback_control: PlaybackControl::default(),
            preloader,
        }
    }

    pub fn play(&mut self, path: &str) -> Result<ClipResult> {
        self.play_with_seek(path, None)
    }

    /// Play directly through the desktop output with experimental loudness lookahead.
    /// Only the requested audio/video preview is buffered; no intermediate file is created.
    /// Enabling `dynamics` selects the example's experimental AGC/compressor chain
    /// and adds its short audio/video preview independently of `lookahead`.
    #[cfg(feature = "desktop-base")]
    pub fn play_with_loudness_preview(
        &mut self,
        path: &str,
        loudness: LiveLoudnessConfig,
        measurement: LiveLoudnessMeasurement,
        lookahead: Duration,
        dynamics: bool,
        show_stats: bool,
    ) -> Result<(ClipResult, LiveLoudnessMetrics)> {
        let config = self.config.clone();
        let playback_control = self.playback_control.clone();
        let fallback_duration = self.fallback_duration;
        let mut timeline = self.timeline;
        let path = path.to_owned();
        let benchmark = benchmark::start(config.channel_id);
        let processor = LiveLoudnessProcessor::new(config.sample_rate, loudness)?;
        let dynamics = dynamics
            .then(|| LiveDynamicsProcessor::new(config.sample_rate, loudness))
            .transpose()?;
        let stats = show_stats
            .then(|| loudness_preview::AudioStatsOverlay::new(config.sample_rate))
            .transpose()?;
        let operation = self.output.run_desktop(benchmark, move |output| {
            let mut preview = loudness_preview::LoudnessPreview::new(
                output,
                processor,
                measurement,
                lookahead,
                config.sample_rate,
                config.video_time_base,
            );
            preview.set_dynamics(dynamics);
            preview.set_stats(stats);
            let result = play_to_output(
                &path,
                &config,
                &mut timeline,
                &mut preview,
                fallback_duration,
                &playback_control,
                PlayOptions {
                    seek_seconds: None,
                    duration_seconds: None,
                    external_audio_path: None,
                    subtitles_media_path: None,
                    logo_fade: LogoFade::default(),
                },
                None,
            );
            let result = result.and_then(|result| {
                if !matches!(result, ClipResult::Stopped | ClipResult::Skipped) {
                    preview.finish()?;
                }

                Ok((result, preview.metrics()))
            });

            (result, timeline)
        });

        match operation {
            Ok((result, timeline)) => {
                self.timeline = timeline;

                result
            }
            Err(error) if error.downcast_ref::<PlaybackStopped>().is_some() => {
                Ok((ClipResult::Stopped, LiveLoudnessMetrics::default()))
            }
            Err(error) => Err(error),
        }
    }

    pub fn play_with_seek(&mut self, path: &str, seek_seconds: Option<f64>) -> Result<ClipResult> {
        self.play_timed(
            path,
            seek_seconds,
            None,
            None,
            Some(path),
            LogoFade::default(),
            1.0,
        )
    }

    pub fn play_with_timing_and_logo_fade(
        &mut self,
        path: &str,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        logo_fade: LogoFade,
    ) -> Result<ClipResult> {
        self.play_timed(
            path,
            seek_seconds,
            duration_seconds,
            None,
            Some(path),
            logo_fade,
            1.0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn play_timed(
        &mut self,
        path: &str,
        seek_seconds: Option<f64>,
        duration_seconds: Option<f64>,
        external_audio_path: Option<&str>,
        subtitles_media_path: Option<&str>,
        logo_fade: LogoFade,
        playout_rate: f64,
    ) -> Result<ClipResult> {
        let external_audio_path = external_audio_path.map(str::to_string);
        let subtitles_media_path = subtitles_media_path.map(str::to_string);
        let preloaded = self.preloader.take(path);
        self.output.set_playout_rate(playout_rate);

        #[cfg(feature = "desktop-base")]
        if self.output.is_desktop() {
            let config = self.config.clone();
            let fallback_duration = self.fallback_duration;
            let playback_control = self.playback_control.clone();
            let mut timeline = self.timeline;
            let path = path.to_string();
            let mut program_audio = std::mem::take(&mut self.program_audio);
            let benchmark = benchmark::start(config.channel_id);

            let operation = self.output.run_desktop(benchmark, move |output| {
                let result = play_program_clip(
                    output,
                    &mut program_audio,
                    &path,
                    &config,
                    &mut timeline,
                    fallback_duration,
                    &playback_control,
                    PlayOptions {
                        seek_seconds,
                        duration_seconds,
                        external_audio_path: external_audio_path.as_deref(),
                        subtitles_media_path: subtitles_media_path.as_deref(),
                        logo_fade,
                    },
                    preloaded,
                );

                (result, timeline, program_audio)
            });

            return match operation {
                Ok((result, timeline, program_audio)) => {
                    self.program_audio = program_audio;
                    self.timeline = timeline;
                    result
                }
                Err(error) if error.downcast_ref::<PlaybackStopped>().is_some() => {
                    Ok(ClipResult::Stopped)
                }
                Err(error) => Err(error),
            };
        }

        let following_break = self
            .config
            .splice_control
            .clip_started(self.timeline.video_pts(), duration_seconds);
        let result = play_program_clip(
            &mut self.output,
            &mut self.program_audio,
            path,
            &self.config,
            &mut self.timeline,
            self.fallback_duration,
            &self.playback_control,
            PlayOptions {
                seek_seconds,
                duration_seconds,
                external_audio_path: external_audio_path.as_deref(),
                subtitles_media_path: subtitles_media_path.as_deref(),
                logo_fade,
            },
            preloaded,
        );
        self.config
            .splice_control
            .clip_finished(following_break, matches!(&result, Ok(ClipResult::Played)));

        result
    }

    pub fn finish(self) -> Result<()> {
        self.preloader.clear();
        let result = self.output.finish();
        benchmark::finish();
        result
    }
}

fn init_media(config: &OutputConfig) -> Result<()> {
    compositor::text::init();
    gstreamer::init()?;
    logging::init(
        config.ffmpeg_log_level,
        config.ingest_log_level,
        &config.ffmpeg_ignore_lines,
        config.channel_id,
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn play_program_clip<O: FrameOutput>(
    output: &mut O,
    state: &mut ProgramAudioState,
    path: &str,
    config: &OutputConfig,
    timeline: &mut Timeline,
    fallback_duration: f64,
    playback_control: &PlaybackControl,
    options: PlayOptions<'_>,
    preloaded: Option<Preloaded>,
) -> Result<ClipResult> {
    let mut output = ProgramAudioOutput::new(output, state, config)?;
    let result = play_to_output(
        path,
        config,
        timeline,
        &mut output,
        fallback_duration,
        playback_control,
        options,
        preloaded,
    );

    if result
        .as_ref()
        .is_ok_and(|result| !matches!(result, ClipResult::Stopped))
    {
        output.finish()?;
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn play_to_output<O: FrameOutput>(
    path: &str,
    config: &OutputConfig,
    timeline: &mut Timeline,
    output: &mut O,
    fallback_duration: f64,
    playback_control: &PlaybackControl,
    options: PlayOptions<'_>,
    preloaded: Option<Preloaded>,
) -> Result<ClipResult> {
    let start = (timeline.video_pts(), timeline.audio_pts());

    match playout::play_clip_with_external_audio(
        path,
        options.external_audio_path,
        config,
        timeline,
        output,
        options.seek_seconds,
        options.duration_seconds,
        options.subtitles_media_path,
        options.logo_fade,
        playback_control,
        preloaded,
    ) {
        Ok(()) => Ok(ClipResult::Played),
        Err(error) if error.downcast_ref::<PlaybackRestart>().is_some() => Err(error),
        Err(error) if error.downcast_ref::<PlaybackSkipped>().is_some() => Ok(ClipResult::Skipped),
        Err(error) if error.downcast_ref::<PlaybackStopped>().is_some() => Ok(ClipResult::Stopped),
        Err(error) if error.is::<playout::LiveSourceLost>() => Ok(ClipResult::Fallback {
            reason: error.to_string(),
        }),
        Err(error) => {
            let reason = format!("{error:#}");
            let slot = options
                .duration_seconds
                .filter(|duration| duration.is_finite() && *duration > 0.0)
                .unwrap_or(fallback_duration);
            let played = played_seconds(config, start, timeline);
            let duration = (slot - played).max(0.0);
            match write_fallback(path, config, timeline, output, duration, playback_control) {
                Ok(()) => {
                    timeline.finish_logo_fade(options.logo_fade);

                    Ok(ClipResult::Fallback { reason })
                }
                Err(error) => classify_fallback_error(error, path),
            }
        }
    }
}

/// How far a clip got since its `start` (video and audio positions), in
/// seconds: the track that went furthest counts, so the fallback after a clip
/// failing midway only fills what is left of its slot.
fn played_seconds(config: &OutputConfig, start: (i64, i64), timeline: &Timeline) -> f64 {
    let video = (timeline.video_pts() - start.0).max(0) as f64 / f64::from(config.fps.max(1));
    let audio =
        (timeline.audio_pts() - start.1).max(0) as f64 / f64::from(config.sample_rate.max(1));

    video.max(audio)
}

fn classify_fallback_error(error: anyhow::Error, path: &str) -> Result<ClipResult> {
    if error.downcast_ref::<PlaybackRestart>().is_some() {
        Err(error)
    } else if error.downcast_ref::<PlaybackSkipped>().is_some() {
        Ok(ClipResult::Skipped)
    } else if error.downcast_ref::<PlaybackStopped>().is_some() {
        Ok(ClipResult::Stopped)
    } else {
        Err(error.context(format!("failed to generate fallback for {path}")))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "tokio")]
    use std::{env, process};

    use super::{
        ClipResult, PlaybackControl, PlaybackRestart, PlaybackSkipped, Timeline,
        classify_fallback_error, played_seconds,
    };

    #[test]
    fn failed_live_connection_returns_control_without_filling_the_event_slot() {
        let directory =
            std::env::temp_dir().join(format!("noar-live-retry-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let config =
            super::OutputConfig::new(64, 48, 25, 48_000).with_stream_type(super::StreamType::Srt);
        let mut playout =
            super::Playout::open_stream(directory.join("output.ts").to_str().unwrap(), config, 1.0)
                .unwrap();
        // A rejected client address fails before any network connection. The
        // scheduler must get control instead of airing the entire ten-minute slot.
        let result = playout
            .play_timed(
                "srt://127.0.0.1:8890?mode=listener",
                None,
                Some(600.0),
                None,
                None,
                super::LogoFade::default(),
                1.0,
            )
            .unwrap();
        assert!(matches!(result, ClipResult::Fallback { .. }));
        assert!(playout.timeline.video_pts() < 25);
        let before = playout.timeline.video_pts();
        let clip = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix/av_sync.mp4");
        let result = playout
            .play_timed(
                clip.to_str().unwrap(),
                None,
                Some(1.0),
                None,
                None,
                super::LogoFade::default(),
                1.0,
            )
            .unwrap();
        assert_eq!(result, ClipResult::Played);
        assert_eq!(playout.timeline.video_pts(), before + 25);
        playout.finish().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn clips_take_exactly_their_duration_on_the_output() {
        let dir = std::env::temp_dir().join(format!("noar_clip_length_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix/av_sync.mp4")
            .to_string_lossy()
            .into_owned();
        let cfg =
            super::OutputConfig::new(320, 240, 25, 48_000).with_stream_type(super::StreamType::Udp);
        let mut playout =
            super::Playout::open_stream(dir.join("out.ts").to_str().unwrap(), cfg, 1.0).unwrap();

        // The last audio frame of a clip used to run past its end, and the
        // video padded a frame to cover it: a break announced ahead then
        // started a frame early.
        for end in [25, 50] {
            playout
                .play_timed(
                    &clip,
                    None,
                    Some(1.0),
                    None,
                    None,
                    super::LogoFade::default(),
                    1.0,
                )
                .unwrap();
            assert_eq!(playout.timeline.video_pts(), end);
        }

        playout.finish().unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn preloaded_clip_trim_after_a_previous_clip_completes_its_slot() {
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let directory =
                std::env::temp_dir().join(format!("noar-trim-preloaded-{}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests_assets/storage/media_mix/av_sync.mp4");
            let config = super::OutputConfig::new(320, 180, 25, 48_000)
                .with_stream_type(super::StreamType::Srt);
            let mut playout = super::Playout::open_stream(
                directory.join("output.ts").to_str().unwrap(),
                config,
                1.0,
            )
            .unwrap();
            for (seek, length, expected) in [(Some(2.88), 9.12, 228), (Some(7.7), 4.3, 336)] {
                playout.preloader.preload(source.to_str().unwrap());
                let result = playout
                    .play_timed(
                        source.to_str().unwrap(),
                        seek,
                        Some(length),
                        None,
                        None,
                        super::LogoFade::default(),
                        1.0,
                    )
                    .unwrap();
                assert_eq!(result, ClipResult::Played);
                assert_eq!(playout.timeline.video_pts(), expected);
            }
            playout.finish().unwrap();
            std::fs::remove_dir_all(directory).unwrap();
            sent.send(()).unwrap();
        });
        received
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("trimmed preloaded clips must complete promptly without wall-clock pacing");
    }

    /// A copy of a test clip with its middle overwritten by garbage.
    fn damaged_clip(dir: &std::path::Path) -> String {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix/av_sync.mp4");
        let mut bytes = std::fs::read(source).unwrap();
        let (start, end) = (bytes.len() * 2 / 5, bytes.len() * 3 / 5);

        for offset in (start..end).step_by(997) {
            for byte in &mut bytes[offset..(offset + 61).min(end)] {
                *byte ^= 0xA5;
            }
        }

        let path = dir.join("damaged.mp4");
        std::fs::write(&path, bytes).unwrap();

        path.to_string_lossy().into_owned()
    }

    #[test]
    fn damaged_data_in_a_clip_keeps_its_slot_length() {
        let dir = std::env::temp_dir().join(format!("noar_damaged_clip_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = damaged_clip(&dir);
        let cfg =
            super::OutputConfig::new(320, 240, 25, 48_000).with_stream_type(super::StreamType::Udp);
        let mut playout =
            super::Playout::open_stream(dir.join("out.ts").to_str().unwrap(), cfg, 1.0).unwrap();

        let result = playout
            .play_timed(
                &clip,
                None,
                Some(20.0),
                None,
                None,
                super::LogoFade::default(),
                1.0,
            )
            .unwrap();

        // Damaged packets are skipped like the FFmpeg command line does, and
        // whatever happens the clip's slot keeps its length: 20 s at 25 fps.
        assert_eq!(playout.timeline.video_pts(), 500);
        assert_eq!(result, ClipResult::Played, "{result:?}");

        playout.finish().unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_clip_failing_midway_counts_what_already_went_out() {
        let cfg = super::OutputConfig::new(320, 240, 25, 48_000);

        // 8 s of video and 8.5 s of audio out since the clip started at 100 / 4800.
        let timeline = Timeline::at(100 + 200, 4_800 + 408_000);
        assert_eq!(played_seconds(&cfg, (100, 4_800), &timeline), 8.5);

        // Nothing out yet, or a timeline behind the start, counts as nothing.
        assert_eq!(
            played_seconds(&cfg, (300, 9_600), &Timeline::at(300, 9_600)),
            0.0
        );
        assert_eq!(
            played_seconds(&cfg, (300, 9_600), &Timeline::at(200, 4_800)),
            0.0
        );
    }

    #[test]
    fn restart_during_fallback_remains_a_playout_restart() {
        let result =
            classify_fallback_error(anyhow::Error::new(PlaybackRestart), "unavailable.mp4");

        assert!(result.unwrap_err().is::<PlaybackRestart>());
    }

    #[test]
    fn playlist_reset_during_fallback_reinitializes_playout() {
        let result =
            classify_fallback_error(anyhow::Error::new(PlaybackSkipped), "unavailable.mp4");

        assert_eq!(result.unwrap(), ClipResult::Skipped);
    }

    #[test]
    fn restart_request_is_independent_from_skip_request() {
        let control = PlaybackControl::default();
        control.restart_playout();

        assert!(control.take_restart());
        assert!(!control.take_skip_current());
        assert!(!control.take_restart());
    }

    #[cfg(feature = "tokio")]
    #[test]
    fn dropping_async_playout_discards_queued_work_and_finishes_worker() {
        use super::*;
        let directory = env::temp_dir().join(format!("noar-drop-worker-{}", process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("output.mkv");
        let playback_control = PlaybackControl::default();
        let worker_control = playback_control.clone();
        let (commands, command_rx) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let worker = thread::spawn(move || {
            // Wait until the caller has dropped its owner with queued work.
            gate.recv().unwrap();
            let mut playout = Playout::open(
                path.to_str().unwrap(),
                OutputConfig::new(32, 32, 25, 48_000),
                1.0,
            )
            .unwrap();
            playout.playback_control = worker_control;
            run_async_playout_worker(playout, command_rx);
            done.send(()).unwrap();
        });
        let owner = AsyncPlayout {
            commands,
            completion: Some(WorkerCompletion::Thread(worker)),
            playback_control: playback_control.clone(),
            preloader: Preloader::default(),
            hls_health: None,
            lshape_control: LShapeControl::default(),
            logo_control: LogoControl::default(),
        };
        let mut responses = Vec::new();

        for _ in 0..2 {
            let (response, result) = oneshot::channel();
            owner
                .commands
                .send(AsyncCommand::Play {
                    path: String::new(),
                    seek_seconds: None,
                    duration_seconds: Some(3600.0),
                    external_audio_path: None,
                    subtitles_media_path: None,
                    logo_fade: Box::default(),
                    playout_rate: 1.0,
                    response,
                })
                .unwrap();
            responses.push(result);
        }
        drop(owner);
        assert!(playback_control.is_shutdown());
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();

        for mut response in responses {
            assert!(matches!(
                response.try_recv(),
                Err(oneshot::error::TryRecvError::Closed)
            ));
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
