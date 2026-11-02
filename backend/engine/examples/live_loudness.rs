//! Offline reference runner for the engine's live loudness processor.
//!
//! ```text
//! cargo run -p noar-engine --example live_loudness -- INPUT_FILE --target-lufs -17 --max-gain-db 16
//! ```
//!
//! When present, the video stream is copied into an MP4 without re-encoding.
//! The first audio stream is decoded, normalized by [`LiveLoudnessProcessor`],
//! and encoded as 128 kbit/s Opus. Audio-only inputs produce an `.opus` file.
//! Reading, decoding, encoding and muxing all go through GStreamer. With
//! `--desktop`, audio and video are processed directly during playback,
//! buffering only the selected lookahead (requires a desktop feature).

use std::{
    collections::VecDeque,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, ValueEnum};
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::{AppSink, AppSrc};
use gstreamer_pbutils::Discoverer;
use noar_engine::{
    BufferedLoudnessAnalysis, LiveDynamicsProcessor, LiveLoudnessConfig, LiveLoudnessMeasurement,
    LiveLoudnessMetrics, LiveLoudnessProcessor, frame::AudioFrame,
};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
/// Samples pushed to the Opus encoder at a time: 20 ms.
const ENCODE_CHUNK: usize = 960;

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum AnalysisMode {
    /// Use the live short-term rider with 10 ms peak lookahead.
    #[default]
    ImmediateShortTerm,
    /// Buffer 500 ms and drive the rider from EBU R128 momentary loudness.
    Momentary500ms,
    /// Buffer 3 seconds and drive the rider from EBU R128 short-term loudness.
    ShortTerm3s,
    /// Experimental slow AGC, 50 ms compressor preview and true-peak limiter.
    LiveDynamics,
}

impl AnalysisMode {
    fn file_suffix(self) -> &'static str {
        match self {
            Self::ImmediateShortTerm => "immediate-short-term",
            Self::Momentary500ms => "momentary-500ms",
            Self::ShortTerm3s => "short-term-3s",
            Self::LiveDynamics => "live-dynamics",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    about = "Normalize an audio stream with the engine's live loudness processor",
    after_help = "All modes use the live 10 ms true-peak limiter. The immediate mode uses causal short-term analysis. Offline buffering preserves audio/video alignment. The buffered rider modes preserve source timing. live-dynamics adds a stereo-linked 6:1 soft-knee compressor with 50 ms preview, slow AGC, and a -55 dBFS pause detector. It needs about 70 ms of audio preview, not three seconds.\n\nFor unusually quiet source material, a target of -17 LUFS may require more than the conservative default of --max-gain-db 8; for example use --max-gain-db 16."
)]
struct Arguments {
    /// Input audio or video file. Video inputs retain their original video stream.
    input: PathBuf,

    /// Play directly in a desktop window with audio, without rendering a file.
    #[cfg(feature = "desktop-base")]
    #[arg(long)]
    desktop: bool,

    /// Hide the audio statistics overlay during desktop playback.
    #[cfg(feature = "desktop-base")]
    #[arg(long)]
    no_audio_stats: bool,

    /// Overwrite an existing output file.
    #[arg(short = 'y', long)]
    overwrite: bool,

    /// Loudness analysis/correction mode. Offline buffering preserves A/V alignment; every mode uses the live peak limiter. Options are: immediate-short-term, momentary500ms, short-term3s, live-dynamics.
    #[arg(long, value_enum, default_value_t = AnalysisMode::ImmediateShortTerm)]
    analysis_mode: AnalysisMode,

    /// Target loudness in LUFS. Higher values are louder; e.g. -17 is louder than -23.
    #[arg(short = 't', long, default_value_t = -23.0, allow_hyphen_values = true)]
    target_lufs: f64,

    /// No gain change inside this distance from the target, in LU. A wider band reduces gain movement.
    #[arg(long, default_value_t = 1.0)]
    dead_band_lu: f64,

    /// Largest upward gain the live rider may apply, in dB. Increase for very quiet sources; higher values also raise background noise.
    #[arg(long, default_value_t = 8.0)]
    max_gain_db: f64,

    /// Largest attenuation the live rider may apply, in dB. This must be zero or negative; e.g. -12 allows reducing loud input by up to 12 dB.
    #[arg(long, default_value_t = -12.0, allow_hyphen_values = true)]
    max_attenuation_db: f64,

    /// Maximum upward gain change per second, in dB/s. Smaller values avoid pumping but take longer to lift quiet material.
    #[arg(long, default_value_t = 0.5)]
    gain_up_db_per_second: f64,

    /// Maximum downward gain change per second, in dB/s. A higher value reacts more quickly to unexpectedly loud input.
    #[arg(long, default_value_t = 2.0)]
    gain_down_db_per_second: f64,

    /// Silence gate for rider modes, in LUFS. live-dynamics uses a fixed -55 dBFS pause threshold.
    #[arg(long, default_value_t = -60.0, allow_hyphen_values = true)]
    silence_gate_lufs: f64,

    /// Final true-peak ceiling in dBTP, applied by the live lookahead limiter.
    #[arg(long, default_value_t = -1.0, allow_hyphen_values = true)]
    true_peak_ceiling_dbtp: f64,
}

impl Arguments {
    fn loudness_config(&self) -> Result<LiveLoudnessConfig> {
        let values = [
            self.target_lufs,
            self.dead_band_lu,
            self.max_gain_db,
            self.max_attenuation_db,
            self.gain_up_db_per_second,
            self.gain_down_db_per_second,
            self.silence_gate_lufs,
            self.true_peak_ceiling_dbtp,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            bail!("all loudness parameters must be finite numbers");
        }
        if self.dead_band_lu < 0.0
            || self.max_gain_db < 0.0
            || self.max_attenuation_db > 0.0
            || self.gain_up_db_per_second < 0.0
            || self.gain_down_db_per_second < 0.0
        {
            bail!(
                "gain limits and rates must be positive; max attenuation must be zero or negative"
            );
        }
        Ok(LiveLoudnessConfig {
            target_lufs: self.target_lufs,
            dead_band_lu: self.dead_band_lu,
            max_gain_db: self.max_gain_db,
            max_attenuation_db: self.max_attenuation_db,
            gain_up_db_per_second: self.gain_up_db_per_second,
            gain_down_db_per_second: self.gain_down_db_per_second,
            silence_gate_lufs: self.silence_gate_lufs,
            true_peak_ceiling_dbtp: self.true_peak_ceiling_dbtp,
            ..LiveLoudnessConfig::default()
        })
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let arguments = Arguments::parse();
    let loudness_config = arguments.loudness_config()?;
    gst::init().context("initializing GStreamer")?;
    #[cfg(feature = "desktop-base")]
    if arguments.desktop {
        return play_desktop(&arguments, loudness_config);
    }

    render_file(&arguments, loudness_config)
}

/// Decode the input, normalize its audio and write it next to the input.
fn render_file(arguments: &Arguments, loudness_config: LiveLoudnessConfig) -> Result<()> {
    let input = &arguments.input;
    if !input.is_file() {
        bail!("input file not found: {}", input.display());
    }

    let uri = gst::glib::filename_to_uri(input.canonicalize()?, None)?;
    let info = Discoverer::new(gst::ClockTime::from_seconds(30))?
        .discover_uri(&uri)
        .context("reading the input")?;
    ensure!(
        !info.audio_streams().is_empty(),
        "input has no audio stream"
    );
    let has_video = !info.video_streams().is_empty();
    let output = output_path(input, has_video, arguments.analysis_mode)?;
    if output.exists() && !arguments.overwrite {
        bail!("output file already exists: {}", output.display());
    }
    let mut progress = Progress::new(info.duration().map_or(0, |duration| {
        i64::try_from(duration.useconds()).unwrap_or(i64::MAX)
    }));

    let render = RenderPipeline::new(input, &output, has_video)?;
    let mut loudness = LoudnessPipeline::new(loudness_config, arguments.analysis_mode)?;
    let mut samples = [Vec::new(), Vec::new()];
    let mut next_sample = 0_u64;
    progress.print();
    render.pipeline.set_state(gst::State::Playing)?;

    loop {
        render.check_errors()?;

        let Some(sample) = render
            .decoded
            .try_pull_sample(gst::ClockTime::from_mseconds(100))
        else {
            // The audio sink joins the pipeline when the input's audio
            // stream appears, and reads as ended until it plays.
            if render.decoded.current_state() == gst::State::Playing && render.decoded.is_eos() {
                break;
            }
            continue;
        };
        let frame = decoded_frame(&sample)?;
        progress.report(
            sample
                .buffer()
                .and_then(gst::BufferRef::pts)
                .map(|pts| i64::try_from(pts.useconds()).unwrap_or(i64::MAX)),
        );
        loudness.process(frame, &mut samples);
        push_ready_audio(&render.encoded, &mut samples, &mut next_sample, false)?;
    }

    loudness.flush(&mut samples);
    push_ready_audio(&render.encoded, &mut samples, &mut next_sample, true)?;
    render.encoded.end_of_stream()?;
    let finished = render.wait_for_end();
    let _ = render.pipeline.set_state(gst::State::Null);
    finished?;

    progress.finish();
    println!("created: {}", output.display());
    println!("final metrics: {:#?}", loudness.metrics());

    Ok(())
}

/// One GStreamer pipeline: the input's video goes to the muxer as it is,
/// its audio comes out decoded at `decoded` and goes back in, normalized,
/// at `encoded`.
struct RenderPipeline {
    pipeline: gst::Pipeline,
    decoded: AppSink,
    encoded: AppSrc,
}

impl RenderPipeline {
    fn new(input: &Path, output: &Path, has_video: bool) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let source = gst::ElementFactory::make("filesrc")
            .property("location", input.to_string_lossy().as_ref())
            .build()?;
        let parse = gst::ElementFactory::make("parsebin").build()?;
        let mux = gst::ElementFactory::make(if has_video { "mp4mux" } else { "oggmux" }).build()?;
        let sink = gst::ElementFactory::make("filesink")
            .property("location", output.to_string_lossy().as_ref())
            .build()?;
        let encoded = AppSrc::builder()
            .caps(&raw_audio_caps())
            .format(gst::Format::Time)
            .build();
        let convert = gst::ElementFactory::make("audioconvert").build()?;
        let opus = gst::ElementFactory::make("opusenc")
            .property("bitrate", 128_000_i32)
            .build()?;
        let decoded = AppSink::builder()
            .caps(&raw_audio_caps())
            .sync(false)
            .build();

        pipeline.add_many([
            &source,
            &parse,
            &mux,
            &sink,
            encoded.upcast_ref(),
            &convert,
            &opus,
        ])?;
        source.link(&parse)?;
        gst::Element::link_many([encoded.upcast_ref(), &convert, &opus, &mux, &sink])?;

        let audio_linked = Arc::new(AtomicBool::new(false));
        let video_linked = Arc::new(AtomicBool::new(false));
        let pipeline_weak = pipeline.downgrade();
        let decoded_sink = decoded.clone();
        parse.connect_pad_added(move |_, pad| {
            let Some(pipeline) = pipeline_weak.upgrade() else {
                return;
            };
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            let kind = caps
                .structure(0)
                .map(|structure| structure.name().to_string())
                .unwrap_or_default();
            let result = if kind.starts_with("audio/") && !audio_linked.swap(true, Ordering::SeqCst)
            {
                link_audio_branch(&pipeline, pad, &decoded_sink)
            } else if kind.starts_with("video/")
                && has_video
                && !video_linked.swap(true, Ordering::SeqCst)
            {
                link_video_branch(&pipeline, pad, &mux)
            } else {
                // Other streams (a second audio track, subtitles) are dropped.
                link_to_fakesink(&pipeline, pad)
            };

            if let Err(error) = result {
                log::error!("linking the {kind} stream: {error:#}");
            }
        });

        Ok(Self {
            pipeline,
            decoded,
            encoded,
        })
    }

    fn check_errors(&self) -> Result<()> {
        let Some(bus) = self.pipeline.bus() else {
            return Ok(());
        };

        if let Some(message) = bus.pop_filtered(&[gst::MessageType::Error])
            && let gst::MessageView::Error(error) = message.view()
        {
            bail!("GStreamer: {} ({:?})", error.error(), error.debug());
        }

        Ok(())
    }

    fn wait_for_end(&self) -> Result<()> {
        let bus = self.pipeline.bus().context("pipeline has no bus")?;
        let message = bus
            .timed_pop_filtered(
                gst::ClockTime::NONE,
                &[gst::MessageType::Eos, gst::MessageType::Error],
            )
            .context("pipeline ended without a message")?;

        if let gst::MessageView::Error(error) = message.view() {
            bail!("GStreamer: {} ({:?})", error.error(), error.debug());
        }

        Ok(())
    }
}

/// Planar f32 stereo at the processor's rate, as GStreamer delivers it
/// interleaved.
fn raw_audio_caps() -> gst::Caps {
    gst::Caps::builder("audio/x-raw")
        .field("format", "F32LE")
        .field("layout", "interleaved")
        .field("rate", SAMPLE_RATE as i32)
        .field("channels", CHANNELS as i32)
        .build()
}

fn link_audio_branch(pipeline: &gst::Pipeline, pad: &gst::Pad, sink: &AppSink) -> Result<()> {
    let decode = gst::ElementFactory::make("decodebin").build()?;
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let resample = gst::ElementFactory::make("audioresample").build()?;
    pipeline.add_many([&decode, &convert, &resample, sink.upcast_ref()])?;
    gst::Element::link_many([&convert, &resample, sink.upcast_ref()])?;

    let target = convert
        .static_pad("sink")
        .context("audio converter has no input")?;
    decode.connect_pad_added(move |_, decoded| {
        if !target.is_linked() {
            let _ = decoded.link(&target);
        }
    });
    pad.link(&decode.static_pad("sink").context("decoder has no input")?)?;

    for element in [sink.upcast_ref(), &resample, &convert, &decode] {
        element.sync_state_with_parent()?;
    }

    Ok(())
}

fn link_video_branch(pipeline: &gst::Pipeline, pad: &gst::Pad, mux: &gst::Element) -> Result<()> {
    // Video is only queued here while the audio waits in the loudness
    // lookahead, so the queue has no size limit.
    let queue = gst::ElementFactory::make("queue")
        .property("max-size-buffers", 0_u32)
        .property("max-size-bytes", 0_u32)
        .property("max-size-time", 0_u64)
        .build()?;
    pipeline.add(&queue)?;
    pad.link(&queue.static_pad("sink").context("queue has no input")?)?;
    let target = mux
        .request_pad_simple("video_%u")
        .context("the muxer takes no video")?;
    queue
        .static_pad("src")
        .context("queue has no output")?
        .link(&target)?;
    queue.sync_state_with_parent()?;

    Ok(())
}

fn link_to_fakesink(pipeline: &gst::Pipeline, pad: &gst::Pad) -> Result<()> {
    let sink = gst::ElementFactory::make("fakesink")
        .property("sync", false)
        .build()?;
    pipeline.add(&sink)?;
    pad.link(&sink.static_pad("sink").context("fakesink has no input")?)?;
    sink.sync_state_with_parent()?;

    Ok(())
}

/// A decoded interleaved sample as a planar frame.
fn decoded_frame(sample: &gst::Sample) -> Result<AudioFrame> {
    let buffer = sample.buffer().context("decoded audio has no buffer")?;
    let mapped = buffer.map_readable()?;
    let bytes_per_sample = CHANNELS * 4;
    ensure!(
        mapped.len().is_multiple_of(bytes_per_sample),
        "decoded audio has a partial sample"
    );
    let mut frame = AudioFrame::stereo(mapped.len() / bytes_per_sample);
    frame.set_rate(SAMPLE_RATE);

    for channel in 0..CHANNELS {
        for (index, value) in frame.plane_mut(channel).iter_mut().enumerate() {
            let start = index * bytes_per_sample + channel * 4;
            *value = f32::from_le_bytes(mapped[start..start + 4].try_into()?);
        }
    }

    Ok(frame)
}

/// Send normalized audio to the encoder in 20 ms chunks; at the end, the
/// rest as well.
fn push_ready_audio(
    encoded: &AppSrc,
    samples: &mut [Vec<f32>; CHANNELS],
    next_sample: &mut u64,
    finish: bool,
) -> Result<()> {
    while samples[0].len() >= ENCODE_CHUNK || (finish && !samples[0].is_empty()) {
        let count = samples[0].len().min(ENCODE_CHUNK);
        let mut bytes = Vec::with_capacity(count * CHANNELS * 4);

        for index in 0..count {
            for channel in samples.iter() {
                bytes.extend_from_slice(&channel[index].to_le_bytes());
            }
        }

        for channel in samples.iter_mut() {
            channel.drain(..count);
        }

        let mut buffer = gst::Buffer::from_mut_slice(bytes);
        let writable = buffer.get_mut().context("new buffer is shared")?;
        writable.set_pts(samples_to_time(*next_sample));
        writable.set_duration(samples_to_time(count as u64));
        *next_sample += count as u64;
        encoded.push_buffer(buffer)?;
    }

    Ok(())
}

fn samples_to_time(samples: u64) -> gst::ClockTime {
    gst::ClockTime::from_nseconds(samples * 1_000_000_000 / u64::from(SAMPLE_RATE))
}

#[cfg(feature = "desktop-base")]
fn play_desktop(arguments: &Arguments, loudness: LiveLoudnessConfig) -> Result<()> {
    use std::time::Duration;

    use noar_engine::{ClipResult, OutputConfig, Playout};

    let (measurement, lookahead) = match arguments.analysis_mode {
        AnalysisMode::ImmediateShortTerm => (LiveLoudnessMeasurement::ShortTerm, Duration::ZERO),
        AnalysisMode::Momentary500ms => (
            LiveLoudnessMeasurement::Momentary,
            Duration::from_millis(500),
        ),
        AnalysisMode::ShortTerm3s => (LiveLoudnessMeasurement::ShortTerm, Duration::from_secs(3)),
        AnalysisMode::LiveDynamics => (LiveLoudnessMeasurement::ShortTerm, Duration::ZERO),
    };
    let config = OutputConfig::new(1280, 720, 25, SAMPLE_RATE);
    let mut playout = Playout::open_desktop(config, 10.0)?;
    let (result, metrics) = playout.play_with_loudness_preview(
        arguments.input.to_string_lossy().as_ref(),
        loudness,
        measurement,
        lookahead,
        matches!(arguments.analysis_mode, AnalysisMode::LiveDynamics),
        !arguments.no_audio_stats,
    )?;

    if let ClipResult::Fallback { reason } = &result {
        bail!("desktop playback failed: {reason}");
    }

    playout.finish()?;
    if !matches!(result, ClipResult::Stopped) {
        println!("final metrics: {metrics:#?}");
    }

    Ok(())
}

struct Progress {
    duration_us: Option<i64>,
    latest_us: i64,
    last_reported_us: i64,
}

impl Progress {
    fn new(duration_us: i64) -> Self {
        Self {
            duration_us: (duration_us > 0).then_some(duration_us),
            latest_us: 0,
            last_reported_us: 0,
        }
    }

    fn report(&mut self, timestamp_us: Option<i64>) {
        let Some(timestamp_us) = timestamp_us else {
            return;
        };
        self.latest_us = self.latest_us.max(timestamp_us);
        if self.latest_us - self.last_reported_us >= 1_000_000 {
            self.print();
            self.last_reported_us = self.latest_us;
        }
    }

    fn finish(&mut self) {
        if let Some(duration_us) = self.duration_us {
            self.latest_us = duration_us;
        }
        self.print();
        println!();
    }

    fn print(&self) {
        let seconds = self.latest_us as f64 / 1_000_000.0;
        if let Some(duration_us) = self.duration_us {
            let percent = (self.latest_us as f64 / duration_us as f64 * 100.0).min(100.0);
            print!("\rprocessing: {seconds:.1} s ({percent:.0}%)");
        } else {
            print!("\rprocessing: {seconds:.1} s");
        }
        let _ = io::stdout().flush();
    }
}

struct LoudnessPipeline {
    processor: LiveLoudnessProcessor,
    dynamics: Option<LiveDynamicsProcessor>,
    lookahead_samples: usize,
    buffered_samples: usize,
    pending: VecDeque<(AudioFrame, Option<BufferedLoudnessAnalysis>)>,
}

impl LoudnessPipeline {
    fn new(config: LiveLoudnessConfig, mode: AnalysisMode) -> Result<Self> {
        let mut processor = LiveLoudnessProcessor::new(SAMPLE_RATE, config)?;
        let lookahead_samples = match mode {
            AnalysisMode::ImmediateShortTerm => 0,
            AnalysisMode::Momentary500ms => {
                processor.set_measurement(LiveLoudnessMeasurement::Momentary);
                SAMPLE_RATE as usize / 2
            }
            AnalysisMode::ShortTerm3s => SAMPLE_RATE as usize * 3,
            AnalysisMode::LiveDynamics => 0,
        };
        let dynamics = matches!(mode, AnalysisMode::LiveDynamics)
            .then(|| LiveDynamicsProcessor::new(SAMPLE_RATE, config))
            .transpose()?;

        Ok(Self {
            processor,
            dynamics,
            lookahead_samples,
            buffered_samples: 0,
            pending: VecDeque::new(),
        })
    }

    fn process(&mut self, mut frame: AudioFrame, samples: &mut [Vec<f32>; CHANNELS]) {
        let analysis =
            (self.lookahead_samples > 0).then(|| self.processor.analyze_buffered(&mut frame));

        self.buffered_samples += frame.samples();
        self.pending.push_back((frame, analysis));
        let preview_samples = self.lookahead_samples.max(self.required_samples());

        while self.pending.front().is_some_and(|(frame, _)| {
            self.buffered_samples.saturating_sub(frame.samples()) >= preview_samples
        }) {
            self.emit_front(samples);
        }
    }

    fn emit_front(&mut self, samples: &mut [Vec<f32>; CHANNELS]) {
        let Some((mut frame, analysis)) = self.pending.pop_front() else {
            return;
        };
        self.buffered_samples -= frame.samples();
        let limit = self.required_samples();
        let mut future = Vec::with_capacity(limit);

        for (buffered, _) in &self.pending {
            for index in 0..buffered.samples() {
                if future.len() == limit {
                    break;
                }

                future.push([buffered.plane(0)[index], buffered.plane(1)[index]]);
            }

            if future.len() == limit {
                break;
            }
        }

        if let Some(dynamics) = self.dynamics.as_mut() {
            dynamics.process(&mut frame, &future);
        } else if let Some(analysis) = analysis {
            self.processor
                .apply_buffered_gain(&mut frame, &analysis, &future);
        } else {
            self.processor.process_with_lookahead(&mut frame, &future);
        }

        append_frame(samples, &frame);
    }

    fn flush(&mut self, samples: &mut [Vec<f32>; CHANNELS]) {
        while !self.pending.is_empty() {
            self.emit_front(samples);
        }
    }

    fn required_samples(&self) -> usize {
        self.dynamics.as_ref().map_or(
            self.processor.peak_lookahead_samples(),
            LiveDynamicsProcessor::lookahead_samples,
        )
    }

    fn metrics(&self) -> LiveLoudnessMetrics {
        self.dynamics
            .as_ref()
            .map_or_else(|| self.processor.metrics(), LiveDynamicsProcessor::metrics)
    }
}

fn append_frame(samples: &mut [Vec<f32>; CHANNELS], frame: &AudioFrame) {
    for (channel, buffer) in samples.iter_mut().enumerate() {
        buffer.extend_from_slice(frame.plane(channel));
    }
}

fn output_path(input: &Path, has_video: bool, analysis_mode: AnalysisMode) -> Result<PathBuf> {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .context("input file has no name")?
        .to_string_lossy();
    let extension = if has_video { "mp4" } else { "opus" };
    Ok(parent.join(format!(
        "{stem} # live_loudness # {}.{extension}",
        analysis_mode.file_suffix()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(samples: usize, value: f32) -> AudioFrame {
        let mut frame = AudioFrame::stereo(samples);
        frame.set_rate(SAMPLE_RATE);

        for channel in 0..CHANNELS {
            frame.plane_mut(channel).fill(value);
        }

        frame
    }

    #[test]
    fn all_modes_limit_future_peaks_and_drain_without_losing_samples() {
        for mode in [
            AnalysisMode::ImmediateShortTerm,
            AnalysisMode::Momentary500ms,
            AnalysisMode::ShortTerm3s,
            AnalysisMode::LiveDynamics,
        ] {
            let mut pipeline = LoudnessPipeline::new(LiveLoudnessConfig::default(), mode).unwrap();
            let mut output = [Vec::new(), Vec::new()];
            pipeline.process(audio(1_024, 0.1), &mut output);
            assert!(output[0].is_empty());
            let mut peak = audio(1_024, 0.1);

            for channel in 0..CHANNELS {
                peak.plane_mut(channel)[0] = 2.0;
            }

            pipeline.process(peak, &mut output);

            if pipeline.lookahead_samples > 0 {
                assert!(output[0].is_empty());
            }

            pipeline.flush(&mut output);
            assert_eq!(output[0].len(), 2_048);
            assert_eq!(output[0], output[1]);
            assert!(output[0][1_000] < 0.06);
            assert!(
                output[0]
                    .iter()
                    .all(|value| value.abs() <= 10.0_f32.powf(-1.0 / 20.0))
            );
            assert_eq!(pipeline.buffered_samples, 0);
        }
    }

    #[test]
    fn three_second_mode_analyzes_future_audio_before_emitting() {
        let mut pipeline =
            LoudnessPipeline::new(LiveLoudnessConfig::default(), AnalysisMode::ShortTerm3s)
                .unwrap();
        let mut output = [Vec::new(), Vec::new()];

        for _ in 0..30 {
            pipeline.process(audio(4_800, 0.1), &mut output);
        }

        assert!(output[0].is_empty());
        assert!(pipeline.metrics().short_term_lufs.is_some());
        pipeline.process(audio(4_800, 0.1), &mut output);
        assert_eq!(output[0].len(), 4_800);
        pipeline.flush(&mut output);
        assert_eq!(output[0].len(), 31 * 4_800);
    }
    #[test]
    fn three_second_buffer_keeps_gain_changes_on_the_source_timeline() {
        let mut immediate = LoudnessPipeline::new(
            LiveLoudnessConfig::default(),
            AnalysisMode::ImmediateShortTerm,
        )
        .unwrap();
        let mut delayed =
            LoudnessPipeline::new(LiveLoudnessConfig::default(), AnalysisMode::ShortTerm3s)
                .unwrap();
        let mut reference = [Vec::new(), Vec::new()];
        let mut output = [Vec::new(), Vec::new()];
        let rate = SAMPLE_RATE as usize;

        // Irregular packet boundaries cross the 100 ms measurement cadence.
        // Loud input at four seconds must not attenuate the quiet input at one second.
        for offset in (0..rate * 10).step_by(777) {
            let count = 777.min(rate * 10 - offset);
            let mut frame = audio(count, 0.0);

            for channel in 0..CHANNELS {
                for (index, sample) in frame.plane_mut(channel).iter_mut().enumerate() {
                    let position = offset + index;
                    let amplitude = if (rate * 4..rate * 6).contains(&position) {
                        0.4
                    } else {
                        0.01
                    };
                    *sample = (std::f64::consts::TAU * 997.0 * position as f64
                        / f64::from(SAMPLE_RATE))
                    .sin() as f32
                        * amplitude;
                }
            }

            immediate.process(frame.clone(), &mut reference);
            delayed.process(frame, &mut output);
        }

        immediate.flush(&mut reference);
        delayed.flush(&mut output);
        assert_eq!(output[0].len(), rate * 10);

        for (index, (&actual, &expected)) in output[0].iter().zip(&reference[0]).enumerate() {
            assert!(
                (actual - expected).abs() < 0.000001,
                "gain correction shifted at sample {index}: {actual} vs {expected}"
            );
        }
    }
}
