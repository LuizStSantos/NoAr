//! Persistent native encoding with isolated mux/transport destinations.
//!
//! Encoded samples are shared with preview and copy recording. Each muxer owns
//! a pipeline, so an optional recording or viewer cannot interrupt the air.

use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use ::gstreamer::{self as gst, prelude::*};
use anyhow::{Context, Result, anyhow, bail, ensure};
use ffmpeg_next::{Rational, frame};
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};

use super::{
    gst_hls::HlsPlaylist,
    link::{LinkFormat, NetworkSink},
    pipeline::{self, configure_muxer, make, muxer_factory},
    recording_native::{self, RecordingMonitor},
    scte35::{SpliceControl, TsInjector},
    vtt,
};
use crate::{
    HlsHealth,
    analysis::{audio_level::AudioLevelMeter, loudness::LoudnessMeter},
    audio_mixer::AudioEffectChain,
    utils::{
        clock::PlayoutClock,
        config::{HlsSubtitle, HlsVariant, OutputConfig},
        ffmpeg::{make_audio_frame_writable, reference_audio_frame, reference_video_frame},
    },
};

const MAX_ENCODED_QUEUE: u64 = 8 * 1024 * 1024;

fn inject_hls_buffer(buffer: &mut gst::BufferRef, injector: &mut TsInjector) -> Result<()> {
    let bytes = injector.process(buffer.map_readable()?.as_slice());
    buffer.remove_all_memory();
    buffer.append_memory(gst::Memory::from_mut_slice(bytes));

    Ok(())
}

#[derive(Clone)]
pub(super) enum EncodedFormat {
    Auto,
    #[cfg(feature = "desktop-base")]
    Recording(crate::RecordingConfig),
    Stream {
        muxer: String,
    },
    Hls {
        variants: Vec<HlsVariant>,
        subtitle: Option<HlsSubtitle>,
        segment_seconds: u32,
        list_size: u32,
    },
}

struct Destination {
    pipeline: gst::Pipeline,
    video: AppSrc,
    audio: AppSrc,
    video_parser: gst::Element,
    audio_parser: gst::Element,
    optional: bool,
    label: &'static str,
    disabled: Mutex<Option<String>>,
    hls: Option<Arc<Mutex<HlsPlaylist>>>,
    hls_split: Option<(gst::Element, SpliceControl)>,
    splice_input: Option<SpliceControl>,
    recording: Mutex<Option<RecordingMonitor>>,
    eos: AtomicBool,
}

impl Destination {
    fn new(optional: bool, label: &'static str, cfg: &OutputConfig) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let video = AppSrc::builder()
            .format(gst::Format::Time)
            .handle_segment_change(true)
            .max_bytes(MAX_ENCODED_QUEUE)
            .build();
        let audio = AppSrc::builder()
            .format(gst::Format::Time)
            .handle_segment_change(true)
            .max_bytes(MAX_ENCODED_QUEUE)
            .build();
        // Destinations negotiate their own framing: Annex B for MPEG-TS,
        // AVC headers for formats such as MP4 and FLV.
        let video_parser = pipeline::parser_for_codec(&cfg.video_codec, true)?;
        let audio_parser = pipeline::parser_for_codec(&cfg.audio_codec, false)?;
        if video_parser.find_property("config-interval").is_some() {
            video_parser.set_property("config-interval", -1_i32);
        }
        pipeline.add_many([
            video.upcast_ref::<gst::Element>(),
            audio.upcast_ref::<gst::Element>(),
            &video_parser,
            &audio_parser,
        ])?;
        video.link(&video_parser)?;
        audio.link(&audio_parser)?;

        Ok(Self {
            pipeline,
            video,
            audio,
            video_parser,
            audio_parser,
            optional,
            label,
            disabled: Mutex::new(None),
            hls: None,
            hls_split: None,
            splice_input: None,
            recording: Mutex::new(None),
            eos: AtomicBool::new(false),
        })
    }

    fn disable(&self, reason: String) {
        let mut disabled = self.disabled.lock().unwrap_or_else(PoisonError::into_inner);
        if disabled.is_none() {
            log::warn!("{} stopped: {reason}", self.label);
            *disabled = Some(reason);
        }
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    fn push(&self, sample: &gst::Sample, video: bool) -> Result<()> {
        if self
            .disabled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
        {
            return Ok(());
        }
        let source = if video { &self.video } else { &self.audio };
        if video
            && let Some(buffer) = sample.buffer()
            && let Some(pts) = buffer.pts()
            && let Some(segment) = sample
                .segment()
                .and_then(|s| s.downcast_ref::<gst::ClockTime>())
            && let Some(running) = segment.to_running_time(pts)
        {
            if let Some(control) = &self.splice_input {
                control.video_packet_muxed((running.nseconds() / 100_000 * 9) as i64);
            }
            if let Some((split, control)) = &self.hls_split
                && control.is_splice_time(running.nseconds())
            {
                // This sample has not entered splitmuxsink yet, so the cut is
                // queued before the requested IDR can pass the splitter.
                split.emit_by_name::<()>("split-at-running-time", &[&running.nseconds()]);
            }
        }
        ensure!(
            source.current_level_bytes() < MAX_ENCODED_QUEUE,
            "{} muxer stopped consuming encoded data",
            self.label
        );
        source
            .push_sample(sample)
            .with_context(|| format!("feeding {} muxer", self.label))?;

        Ok(())
    }

    fn poll(&self) -> Result<()> {
        if let Some(error) = self
            .disabled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            return if self.optional {
                Ok(())
            } else {
                Err(anyhow!(error.clone()))
            };
        }
        let mut monitor = self
            .recording
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(monitor) = monitor.as_mut() {
            monitor.check()?;
        }
        drop(monitor);
        let bus = self.pipeline.bus().context("destination has no bus")?;
        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Eos(_) => self.eos.store(true, Ordering::Release),
                gst::MessageView::Error(error) => bail!("{}: {}", self.label, error.error()),
                gst::MessageView::Element(element) => {
                    if let Some(structure) = element.structure()
                        && structure.name() == "splitmuxsink-fragment-closed"
                        && let Some(hls) = self.hls.as_ref()
                    {
                        let location = structure.get::<String>("location")?;
                        let end = structure.get::<gst::ClockTime>("running-time")?;
                        hls.lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .closed(&location, end.nseconds())?;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn finish(&self) -> Result<()> {
        if self
            .disabled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
        {
            return Ok(());
        }
        self.video.end_of_stream()?;
        self.audio.end_of_stream()?;
        let timeout = std::time::Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Err(error) = self.poll() {
                break Err(error);
            }
            if self.eos.load(Ordering::Acquire) {
                break Ok(());
            }
            if std::time::Instant::now() >= timeout {
                break Err(anyhow!("{} did not finish within five seconds", self.label));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let _ = self.pipeline.set_state(gst::State::Null);

        result
    }

    fn stream(
        path: &str,
        cfg: &OutputConfig,
        muxer: &str,
        optional: bool,
        label: &'static str,
        splice: bool,
    ) -> Result<Arc<Self>> {
        let destination = Self::new(optional, label, cfg)?;
        let mux = make(muxer_factory(muxer)?)?;
        configure_muxer(&mux, muxer, &cfg.muxer_options)?;
        apply_metadata(&mux, muxer, &cfg.metadata_options)?;
        let sink = AppSink::builder()
            .sync(false)
            .async_(false)
            .max_buffers(16)
            .build();
        destination.pipeline.add_many([&mux, sink.upcast_ref()])?;
        destination.video_parser.link(&mux)?;
        destination.audio_parser.link(&mux)?;
        mux.link(&sink)?;
        let format = LinkFormat::for_muxer(muxer);
        let mut writer: Box<dyn Write + Send> = if path.contains("://") {
            Box::new(NetworkSink::start(
                path,
                format.context("network format must be MPEG-TS or FLV")?,
                &cfg.protocol_options,
                cfg.output_link.clone(),
                cfg.channel_id,
            )?)
        } else {
            if let Some(parent) = Path::new(path)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            Box::new(fs::File::create(path).with_context(|| format!("creating output {path}"))?)
        };
        let mut injector = splice.then(|| {
            TsInjector::new(
                cfg.splice_control.clone(),
                cfg.scte35
                    .as_ref()
                    .map_or(crate::DEFAULT_SCTE35_PID, |c| c.pid),
            )
        });
        sink.set_callbacks(
            AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                    let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                    let mapped = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                    let injected;
                    let bytes = if let Some(injector) = injector.as_mut() {
                        injected = injector.process(&mapped);
                        injected.as_slice()
                    } else {
                        mapped.as_slice()
                    };
                    writer.write_all(bytes).map_err(|_| gst::FlowError::Error)?;
                    Ok(gst::FlowSuccess::Ok)
                })
                .build(),
        );
        destination.pipeline.set_state(gst::State::Playing)?;

        Ok(Arc::new(destination))
    }

    fn hls(
        path: &str,
        cfg: &OutputConfig,
        segment_seconds: u32,
        list_size: u32,
        subtitle: Option<HlsSubtitle>,
        health: Option<HlsHealth>,
        splice: Option<(SpliceControl, bool)>,
    ) -> Result<Arc<Self>> {
        let mut destination = Self::new(false, "HLS output", cfg)?;
        let playlist = Arc::new(Mutex::new(HlsPlaylist::new(
            path,
            segment_seconds,
            list_size,
            subtitle,
            cfg.muxer_options
                .get("hls_flags")
                .is_some_and(|flags| flags.split('+').any(|f| f == "program_date_time")),
        )?));
        let split = make("splitmuxsink")?;
        let mux = make("mpegtsmux")?;
        apply_metadata(&mux, "mpegts", &cfg.metadata_options)?;
        if let Some((control, follower)) = splice {
            let pid = cfg.scte35.context("HLS SCTE-35 settings missing")?.pid;
            let injector = Mutex::new(TsInjector::new(control.clone(), pid));
            let errors = control.clone();
            mux.static_pad("src")
                .context("HLS muxer has no output")?
                .add_probe(
                    gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST,
                    move |_, info| {
                        let mut injector = injector.lock().unwrap_or_else(PoisonError::into_inner);
                        let result = if let Some(buffer) = info.buffer_mut() {
                            inject_hls_buffer(buffer.make_mut(), &mut injector)
                        } else if let Some(list) = info.buffer_list_mut() {
                            let list = list.make_mut();
                            (0..list.len()).try_for_each(|index| {
                                inject_hls_buffer(
                                    list.get_mut(index).context("missing HLS mux buffer")?,
                                    &mut injector,
                                )
                            })
                        } else {
                            Ok(())
                        };
                        if let Err(error) = result {
                            errors.fail(format!("HLS transport injection: {error:#}"));
                        }
                        gst::PadProbeReturn::Ok
                    },
                );
            if follower {
                destination.splice_input = Some(control);
            }
            playlist
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .set_splice(cfg.splice_control.clone());
            destination.hls_split = Some((split.clone(), cfg.splice_control.clone()));
        }
        if let Some(health) = health {
            mux.static_pad("src")
                .context("HLS muxer has no output")?
                .add_probe(
                    gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST,
                    move |_, _| {
                        health.mark_muxed();
                        gst::PadProbeReturn::Ok
                    },
                );
        }
        split.set_property("muxer", &mux);
        split.set_property("max-size-time", u64::from(segment_seconds) * 1_000_000_000);
        split.set_property("send-keyframe-requests", true);
        let state = Arc::clone(&playlist);
        split.connect("format-location", false, move |values| {
            let id = values[1].get::<u32>().unwrap_or_default();
            Some(
                state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .segment_path(id)
                    .to_value(),
            )
        });
        destination.pipeline.add(&split)?;
        destination
            .video_parser
            .link_pads(None, &split, Some("video"))?;
        destination
            .audio_parser
            .link_pads(None, &split, Some("audio_%u"))?;
        destination.hls = Some(playlist);
        destination.pipeline.set_state(gst::State::Playing)?;

        Ok(Arc::new(destination))
    }

    fn recording(
        config: &crate::RecordingConfig,
        encoding: &OutputConfig,
        optional: bool,
    ) -> Result<Arc<Self>> {
        let mut destination = Self::new(optional, "Recording", encoding)?;
        let monitor = recording_native::prepare(config)?;
        let split = make("splitmuxsink")?;
        let mux = make("matroskamux")?;
        split.set_property("muxer", &mux);
        split.set_property(
            "max-size-time",
            u64::from(config.segment_duration) * 1_000_000_000,
        );
        split.set_property("send-keyframe-requests", true);
        let settings = config.clone();
        split.connect("format-location", false, move |_| {
            Some(recording_native::next_segment(&settings).to_value())
        });
        destination.pipeline.add(&split)?;
        destination
            .video_parser
            .link_pads(None, &split, Some("video"))?;
        destination
            .audio_parser
            .link_pads(None, &split, Some("audio_%u"))?;
        destination.recording = Mutex::new(Some(monitor));
        destination.pipeline.set_state(gst::State::Playing)?;

        Ok(Arc::new(destination))
    }
}

impl Drop for Destination {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

struct Encoder {
    pipeline: gst::Pipeline,
    video: AppSrc,
    audio: AppSrc,
    video_encoder: gst::Element,
    destinations: Arc<Mutex<Vec<Arc<Destination>>>>,
    optional: bool,
    disabled: bool,
}

impl Encoder {
    fn new(
        cfg: &OutputConfig,
        input_width: u32,
        input_height: u32,
        optional: bool,
        key_frame_interval: Option<u32>,
    ) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let video = AppSrc::builder()
            .format(gst::Format::Time)
            .max_bytes(32 * 1024 * 1024)
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", input_width as i32)
                    .field("height", input_height as i32)
                    .field("framerate", gst::Fraction::new(cfg.fps as i32, 1))
                    .build(),
            )
            .build();
        let audio = AppSrc::builder()
            .format(gst::Format::Time)
            .max_bytes(4 * 1024 * 1024)
            .caps(
                &gst::Caps::builder("audio/x-raw")
                    .field("format", "F32LE")
                    .field("layout", "interleaved")
                    .field("rate", cfg.sample_rate as i32)
                    .field("channels", 2_i32)
                    .build(),
            )
            .build();
        let video_encoder = pipeline::video_encoder(
            &cfg.video_codec,
            &cfg.video_options,
            cfg.fps,
            cfg.width,
            cfg.height,
        )?;
        if let Some(interval) = key_frame_interval {
            let property = ["key-int-max", "keyframe-max-dist", "gop-size"]
                .into_iter()
                .find(|name| video_encoder.find_property(name).is_some())
                .context(
                    "the selected GStreamer video encoder cannot enforce HLS keyframe intervals",
                )?;
            pipeline::set_property(&video_encoder, property, &interval.to_string())?;
        }
        let audio_encoder =
            pipeline::audio_encoder(&cfg.audio_codec, &cfg.audio_options, cfg.audio_bitrate)?;
        let converter = make("videoconvert")?;
        let scale = make("videoscale")?;
        scale.set_property("add-borders", true);
        let dimensions = gst::Caps::builder("video/x-raw")
            .field("width", cfg.width as i32)
            .field("height", cfg.height as i32)
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .build();
        let caps = make("capsfilter")?;
        caps.set_property("caps", dimensions);
        let video_parser = pipeline::parser(&video_encoder, true)?;
        if video_parser.find_property("config-interval").is_some() {
            video_parser.set_property("config-interval", -1_i32);
        }
        let audio_converter = make("audioconvert")?;
        let audio_caps = make("capsfilter")?;
        if matches!(cfg.audio_codec.as_str(), "pcm_s16le" | "pcm_f32le") {
            audio_caps.set_property(
                "caps",
                gst::Caps::builder("audio/x-raw")
                    .field(
                        "format",
                        if cfg.audio_codec == "pcm_s16le" {
                            "S16LE"
                        } else {
                            "F32LE"
                        },
                    )
                    .field("layout", "interleaved")
                    .build(),
            );
        }
        let audio_parser = pipeline::parser(&audio_encoder, false)?;
        let video_sink = AppSink::builder()
            .sync(false)
            .async_(false)
            .max_buffers(8)
            .build();
        let audio_sink = AppSink::builder()
            .sync(false)
            .async_(false)
            .max_buffers(16)
            .build();
        pipeline.add_many([
            video.upcast_ref(),
            audio.upcast_ref(),
            &converter,
            &scale,
            &caps,
            &video_encoder,
            &video_parser,
            &audio_converter,
            &audio_caps,
            &audio_encoder,
            &audio_parser,
            video_sink.upcast_ref(),
            audio_sink.upcast_ref(),
        ])?;
        gst::Element::link_many([
            video.upcast_ref(),
            &converter,
            &scale,
            &caps,
            &video_encoder,
            &video_parser,
            video_sink.upcast_ref(),
        ])?;
        gst::Element::link_many([
            audio.upcast_ref(),
            &audio_converter,
            &audio_caps,
            &audio_encoder,
            &audio_parser,
            audio_sink.upcast_ref(),
        ])?;
        let destinations: Arc<Mutex<Vec<Arc<Destination>>>> = Arc::new(Mutex::new(Vec::new()));
        for (sink, is_video) in [(&video_sink, true), (&audio_sink, false)] {
            let destinations = Arc::clone(&destinations);
            sink.set_callbacks(
                AppSinkCallbacks::builder()
                    .new_sample(move |sink| {
                        let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                        let targets = destinations
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .clone();
                        for destination in targets {
                            if let Err(error) = destination.push(&sample, is_video) {
                                destination.disable(format!("{error:#}"));
                            }
                        }
                        Ok(gst::FlowSuccess::Ok)
                    })
                    .build(),
            );
        }
        pipeline.set_state(gst::State::Playing)?;

        Ok(Self {
            pipeline,
            video,
            audio,
            video_encoder,
            destinations,
            optional,
            disabled: false,
        })
    }

    fn attach(&self, destination: Arc<Destination>) {
        self.destinations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(destination);
    }

    fn poll(&mut self) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        let bus = self.pipeline.bus().context("encoder has no bus")?;
        while let Some(message) = bus.pop() {
            if let gst::MessageView::Error(error) = message.view() {
                bail!("GStreamer encoder: {}", error.error());
            }
        }
        for destination in self
            .destinations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            if let Err(error) = destination.poll() {
                if destination.optional {
                    destination.disable(format!("{error:#}"));
                } else {
                    return Err(error);
                }
            }
        }

        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        self.video.end_of_stream()?;
        self.audio.end_of_stream()?;
        let bus = self.pipeline.bus().context("encoder has no bus")?;
        let timeout = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(message) = bus.timed_pop_filtered(
                gst::ClockTime::from_mseconds(20),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            ) {
                match message.view() {
                    gst::MessageView::Eos(_) => break,
                    gst::MessageView::Error(error) => {
                        bail!("finishing GStreamer encoder: {}", error.error())
                    }
                    _ => {}
                }
            }
            ensure!(
                std::time::Instant::now() < timeout,
                "GStreamer encoder did not finish within five seconds"
            );
        }
        for destination in self
            .destinations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
        {
            let result = destination.finish();
            if destination.optional {
                if let Err(error) = result {
                    log::warn!("{} finalization: {error:#}", destination.label);
                }
            } else {
                result?;
            }
        }
        self.pipeline.set_state(gst::State::Null)?;

        Ok(())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

pub(super) struct EncodedOutput {
    encoders: Vec<Encoder>,
    config: OutputConfig,
    audio_effects: AudioEffectChain,
    audio_level: AudioLevelMeter,
    loudness: LoudnessMeter,
    clock: PlayoutClock,
    pace: bool,
    last_video: Option<frame::Video>,
    last_audio_end: Option<i64>,
    pending_vtt: VecDeque<vtt::VttCue>,
    splice: bool,
}

impl EncodedOutput {
    #[cfg(feature = "desktop-base")]
    pub(super) fn open_recording(
        cfg: &OutputConfig,
        recording: &crate::RecordingConfig,
    ) -> Result<Self> {
        let mut config = cfg.clone();
        config.recording = None;
        config.preview_url = None;
        config.muxer_options.clear();
        config.metadata_options.clear();
        config.protocol_options.clear();
        if let Some(settings) = &recording.encode {
            config.video_codec = settings.video_codec.clone();
            config.video_options = settings.video_options.clone();
            config.audio_codec = settings.audio_codec.clone();
            config.audio_options = settings.audio_options.clone();
            config.audio_bitrate = settings.audio_bitrate;
        }
        Self::open(
            &recording.path,
            &config,
            EncodedFormat::Recording(recording.clone()),
        )
    }

    pub(super) fn open(path: &str, cfg: &OutputConfig, format: EncodedFormat) -> Result<Self> {
        Self::open_with_hls_health(path, cfg, format, None)
    }

    pub(super) fn open_with_hls_health(
        path: &str,
        cfg: &OutputConfig,
        format: EncodedFormat,
        health: Option<HlsHealth>,
    ) -> Result<Self> {
        crate::gstreamer::init()?;
        crate::validate_output_metadata(&cfg.metadata_options).map_err(anyhow::Error::msg)?;
        let mut primary_config = cfg.clone();
        if let EncodedFormat::Hls { variants, .. } = &format {
            super::hls::validate_variants(variants)?;
            if let Some(primary) = variants.first() {
                configure_rendition(&mut primary_config, primary);
            }
        }
        #[cfg(feature = "desktop-base")]
        if let EncodedFormat::Recording(recording) = &format
            && let Some(settings) = &recording.encode
        {
            primary_config.width = settings.width;
            primary_config.height = settings.height;
        }
        let hls_keyframes = match &format {
            EncodedFormat::Hls {
                segment_seconds, ..
            } => Some(
                cfg.fps
                    .saturating_mul(if segment_seconds % 2 == 0 { 2 } else { 1 }),
            ),
            _ => None,
        };
        let encoder = Encoder::new(&primary_config, cfg.width, cfg.height, false, hls_keyframes)?;
        let splice = cfg.scte35.is_some()
            && (matches!(&format, EncodedFormat::Hls { .. })
                || matches!(&format, EncodedFormat::Stream { muxer } if muxer == "mpegts"));
        if splice {
            cfg.splice_control.open_output(
                cfg.scte35.context("SCTE settings missing")?,
                cfg.fps,
                (
                    i64::from(cfg.video_time_base.0),
                    i64::from(cfg.video_time_base.1),
                ),
            );
        } else {
            cfg.splice_control
                .unavailable("SCTE-35 requires MPEG-TS over SRT/UDP or native HLS".to_owned());
        }
        let mut variants = Vec::new();
        match format {
            #[cfg(feature = "desktop-base")]
            EncodedFormat::Recording(recording) => {
                encoder.attach(Destination::recording(&recording, &primary_config, false)?);
            }
            EncodedFormat::Hls {
                variants: renditions,
                subtitle,
                segment_seconds,
                list_size,
            } => {
                super::hls::validate_variants(&renditions)?;
                let primary_path = renditions
                    .first()
                    .map(|variant| super::hls::resolved_variant_playlist_path(path, &variant.name))
                    .transpose()?;
                encoder.attach(Destination::hls(
                    primary_path.as_deref().unwrap_or(path),
                    &primary_config,
                    segment_seconds,
                    list_size,
                    subtitle.clone(),
                    health.clone(),
                    splice.then(|| (cfg.splice_control.clone(), false)),
                )?);
                let mut manifest = Vec::new();
                if let Some(primary) = renditions.first() {
                    manifest.push((
                        primary.name.clone(),
                        primary.width,
                        primary.height,
                        primary.video_bitrate + primary.audio_bitrate,
                    ));
                }
                for rendition in renditions.into_iter().skip(1) {
                    let mut config = cfg.clone();
                    configure_rendition(&mut config, &rendition);
                    let rendition_path =
                        super::hls::resolved_variant_playlist_path(path, &rendition.name)?;
                    let variant =
                        Encoder::new(&config, cfg.width, cfg.height, false, hls_keyframes)?;
                    variant.attach(Destination::hls(
                        &rendition_path,
                        &config,
                        segment_seconds,
                        list_size,
                        subtitle.clone(),
                        health.clone(),
                        splice.then(|| (cfg.splice_control.rendition(), true)),
                    )?);
                    manifest.push((
                        rendition.name,
                        config.width,
                        config.height,
                        rendition.video_bitrate + rendition.audio_bitrate,
                    ));
                    variants.push(variant);
                }
                HlsPlaylist::write_master(path, &manifest, subtitle.as_ref())?;
            }
            EncodedFormat::Stream { muxer } => {
                crate::validate_muxer_options(&muxer, &cfg.muxer_options)
                    .map_err(anyhow::Error::msg)?;
                encoder.attach(Destination::stream(
                    path,
                    cfg,
                    &muxer,
                    false,
                    "Program output",
                    splice,
                )?);
            }
            EncodedFormat::Auto => {
                let muxer = match Path::new(path)
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("ts")
                {
                    "mkv" => "matroska",
                    "webm" => "webm",
                    "mp4" | "mov" => "mp4",
                    "flv" => "flv",
                    _ => "mpegts",
                };
                encoder.attach(Destination::stream(
                    path,
                    cfg,
                    muxer,
                    false,
                    "Program output",
                    splice,
                )?);
            }
        }
        if let Some(url) = cfg.preview_url.as_ref() {
            let mut preview_cfg = cfg.clone();
            preview_cfg.protocol_options.clear();
            preview_cfg.muxer_options.clear();
            preview_cfg.metadata_options.clear();
            preview_cfg.output_link = crate::OutputLink::default();
            match Destination::stream(url, &preview_cfg, "mpegts", true, "Player preview", false) {
                Ok(destination) => encoder.attach(destination),
                Err(error) => log::warn!("Player preview is unavailable: {error:#}"),
            }
        }
        let mut encoders = vec![encoder];
        encoders.extend(variants);
        if let Some(recording) = cfg.recording.as_ref() {
            let mut recording_encoding = cfg.clone();
            if let Some(settings) = recording.encode.as_ref() {
                recording_encoding.video_codec = settings.video_codec.clone();
                recording_encoding.audio_codec = settings.audio_codec.clone();
            }
            let destination = Destination::recording(recording, &recording_encoding, true);
            match destination {
                Ok(destination) => {
                    if let Some(settings) = recording.encode.as_ref() {
                        let mut config = cfg.clone();
                        config.width = settings.width;
                        config.height = settings.height;
                        config.video_codec = settings.video_codec.clone();
                        config.video_options = settings.video_options.clone();
                        config.audio_codec = settings.audio_codec.clone();
                        config.audio_options = settings.audio_options.clone();
                        config.audio_bitrate = settings.audio_bitrate;
                        match Encoder::new(&config, cfg.width, cfg.height, true, None) {
                            Ok(encoder) => {
                                encoder.attach(destination);
                                encoders.push(encoder);
                            }
                            Err(error) => log::warn!("Recording encoder unavailable: {error:#}"),
                        }
                    } else {
                        let selected = encoders
                            .get(recording.video_stream_index)
                            .context("recording rendition is unavailable")?;
                        selected.attach(destination);
                    }
                }
                Err(error) => log::warn!("Recording unavailable: {error:#}"),
            }
        }

        Ok(Self {
            encoders,
            config: cfg.clone(),
            audio_effects: AudioEffectChain::new(cfg.audio_effects.clone(), cfg.sample_rate),
            audio_level: AudioLevelMeter::new(cfg.sample_rate, cfg.audio_level_callback.clone()),
            loudness: LoudnessMeter::new(cfg.sample_rate, cfg.loudness_meter_control.clone()),
            clock: PlayoutClock::new(),
            pace: path.contains("://") || path.ends_with(".m3u8"),
            last_video: None,
            last_audio_end: None,
            pending_vtt: VecDeque::new(),
            splice,
        })
    }

    pub(super) fn audio_frame_size(&self) -> usize {
        1024
    }

    pub(super) fn set_playout_rate(&mut self, rate: f64) {
        self.clock.set_rate(rate);
    }

    fn poll(&mut self) -> Result<()> {
        for encoder in &mut self.encoders {
            if let Err(error) = encoder.poll() {
                if encoder.optional {
                    log::warn!("Recording encoder stopped: {error:#}");
                    encoder.disabled = true;
                    let _ = encoder.pipeline.set_state(gst::State::Null);
                } else {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    pub(super) fn encode_video(&mut self, frame: &frame::Video) -> Result<()> {
        self.poll()?;
        let pts = frame.pts().unwrap_or(0);
        if self
            .last_video
            .as_ref()
            .and_then(|v| v.pts())
            .is_some_and(|previous| pts <= previous)
        {
            return Ok(());
        }
        if self.pace {
            self.clock
                .wait_until(Some(pts), self.config.video_time_base);
        }
        let mut buffer = packed_video_buffer(frame)?;
        let writable = buffer.get_mut().context("video buffer is shared")?;
        writable.set_pts(clock_time(pts, self.config.video_time_base));
        writable.set_duration(gst::ClockTime::from_nseconds(
            1_000_000_000 / u64::from(self.config.fps),
        ));
        if self.splice {
            self.config
                .splice_control
                .video_packet_muxed(pts.saturating_mul(90_000) / i64::from(self.config.fps));
            if self.config.splice_control.frame_needs_key_frame(pts) {
                for encoder in &self.encoders {
                    let event = gstreamer_video::UpstreamForceKeyUnitEvent::builder()
                        .running_time(clock_time(pts, self.config.video_time_base))
                        .all_headers(true)
                        .build();
                    if let Some(pad) = encoder.video_encoder.static_pad("src") {
                        pad.send_event(event);
                    }
                }
            }
        }
        for encoder in &self.encoders {
            if !encoder.disabled {
                ensure!(
                    encoder.video.current_level_bytes() < 32 * 1024 * 1024,
                    "video encoder stopped consuming frames"
                );
                encoder.video.push_buffer(buffer.clone())?;
            }
        }
        self.last_video = Some(reference_video_frame(frame)?);
        Ok(())
    }

    pub(super) fn held_video_frame(&self) -> Option<frame::Video> {
        self.last_video
            .as_ref()
            .and_then(|frame| reference_video_frame(frame).ok())
    }

    pub(super) fn encode_audio(&mut self, frame: &frame::Audio) -> Result<()> {
        self.audio(frame, true)
    }
    pub(super) fn encode_processed_audio(&mut self, frame: &frame::Audio) -> Result<()> {
        self.audio(frame, false)
    }

    fn audio(&mut self, input: &frame::Audio, effects: bool) -> Result<()> {
        self.poll()?;
        let pts = input.pts().unwrap_or(self.last_audio_end.unwrap_or(0));
        let skip = self
            .last_audio_end
            .unwrap_or(pts)
            .saturating_sub(pts)
            .max(0) as usize;
        if skip >= input.samples() {
            return Ok(());
        }
        if self.pace {
            self.clock
                .wait_until(Some(pts), self.config.audio_time_base);
        }
        let mut audio = reference_audio_frame(input)?;
        make_audio_frame_writable(&mut audio)?;
        if effects {
            self.audio_effects.process(&mut audio);
        }
        self.audio_level.process_frame(&audio);
        self.loudness.process_frame(&audio);
        let mut bytes = Vec::with_capacity((audio.samples() - skip) * 8);
        for index in skip..audio.samples() {
            for channel in 0..2 {
                let sample = audio.plane::<f32>(channel)[index];
                bytes.extend_from_slice(
                    &if sample.is_finite() { sample } else { 0.0 }.to_le_bytes(),
                );
            }
        }
        let mut buffer = gst::Buffer::from_mut_slice(bytes);
        let writable = buffer.get_mut().context("audio buffer is shared")?;
        writable.set_pts(clock_time(pts + skip as i64, self.config.audio_time_base));
        writable.set_duration(gst::ClockTime::from_nseconds(
            (audio.samples() - skip) as u64 * 1_000_000_000 / u64::from(self.config.sample_rate),
        ));
        for encoder in &self.encoders {
            if !encoder.disabled {
                ensure!(
                    encoder.audio.current_level_bytes() < 4 * 1024 * 1024,
                    "audio encoder stopped consuming frames"
                );
                encoder.audio.push_buffer(buffer.clone())?;
            }
        }
        self.last_audio_end = Some(pts + audio.samples() as i64);
        Ok(())
    }

    pub(super) fn write_vtt_subtitles(
        &mut self,
        media: &str,
        output_start: i64,
        source_start: i64,
    ) -> Result<()> {
        self.pending_vtt.clear();
        let path = vtt::sidecar_path(media);
        if path.is_file() {
            for mut cue in vtt::parse_file(&path)? {
                if cue.end_ms > source_start {
                    cue.start_ms = output_start + cue.start_ms.saturating_sub(source_start);
                    cue.end_ms = output_start + cue.end_ms.saturating_sub(source_start);
                    self.pending_vtt.push_back(cue);
                }
            }
        }
        Ok(())
    }

    pub(super) fn clear_vtt_subtitles(&mut self) {
        self.pending_vtt.clear();
    }

    pub(super) fn advance_vtt_subtitles(&mut self, position: i64) -> Result<()> {
        while self
            .pending_vtt
            .front()
            .is_some_and(|cue| cue.start_ms <= position)
        {
            let cue = self
                .pending_vtt
                .pop_front()
                .context("subtitle cue missing")?;
            for encoder in &self.encoders {
                for destination in encoder
                    .destinations
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .iter()
                {
                    if let Some(hls) = destination.hls.as_ref() {
                        hls.lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .add_cue(&cue);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<()> {
        for encoder in &mut self.encoders {
            if encoder.optional {
                if let Err(error) = encoder.finish() {
                    log::warn!("Recording finalization: {error:#}");
                }
            } else {
                encoder.finish()?;
            }
        }
        if self.splice {
            self.config.splice_control.close_output();
        }
        Ok(())
    }
}

fn clock_time(pts: i64, time_base: Rational) -> gst::ClockTime {
    gst::ClockTime::from_nseconds(
        (i128::from(pts.max(0)) * i128::from(time_base.0) * 1_000_000_000
            / i128::from(time_base.1.max(1))) as u64,
    )
}

/// AVFrame rows can have different padding from GStreamer's default I420
/// layout. Pack into the exact layout advertised by the appsrc caps.
fn packed_video_buffer(frame: &frame::Video) -> Result<gst::Buffer> {
    ensure!(
        frame.format() == ffmpeg_next::format::Pixel::YUV420P,
        "GStreamer output expects a normalized I420 frame"
    );
    let info = gstreamer_video::VideoInfo::builder(
        gstreamer_video::VideoFormat::I420,
        frame.width(),
        frame.height(),
    )
    .build()?;
    let mut data = vec![0_u8; info.size()];

    for plane in 0..3 {
        let width = frame.plane_width(plane) as usize;
        let height = frame.plane_height(plane) as usize;
        let target_stride = usize::try_from(info.stride()[plane])?;
        let target_offset = info.offset()[plane];
        ensure!(
            width <= target_stride,
            "video plane exceeds its GStreamer stride"
        );

        for row in 0..height {
            let source_offset = row * frame.stride(plane);
            let target_offset = target_offset + row * target_stride;
            data[target_offset..target_offset + width]
                .copy_from_slice(&frame.data(plane)[source_offset..source_offset + width]);
        }
    }

    Ok(gst::Buffer::from_mut_slice(data))
}

fn configure_rendition(config: &mut OutputConfig, rendition: &HlsVariant) {
    config.width = rendition.width;
    config.height = rendition.height;
    config.audio_bitrate = rendition.audio_bitrate;
    config.video_options.insert(
        "maxrate".to_owned(),
        (rendition.video_bitrate / 1000).max(1).to_string(),
    );
    config
        .video_options
        .insert("rate_control".to_owned(), "cbr".to_owned());
}

fn apply_metadata(
    mux: &gst::Element,
    format: &str,
    metadata: &BTreeMap<String, String>,
) -> Result<()> {
    if metadata.is_empty() {
        return Ok(());
    }
    if format == "mpegts" {
        return super::gst_metadata::set_service(mux, metadata);
    }
    let setter = mux
        .clone()
        .dynamic_cast::<gst::TagSetter>()
        .map_err(|_| anyhow!("muxer does not support metadata"))?;
    let mut tags = gst::TagList::new();
    for (name, value) in metadata {
        tags.get_mut().context("tag list is shared")?.add_generic(
            name.as_str(),
            value.as_str(),
            gst::TagMergeMode::Replace,
        )?;
    }
    setter.merge_tags(&tags, gst::TagMergeMode::Replace);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf, process::Command, sync::atomic::AtomicU64, time::Instant};

    use ffmpeg_next::{
        ChannelLayout,
        format::{Pixel, Sample, sample::Type},
    };

    #[cfg(feature = "desktop-base")]
    use crate::RecordingConfig;

    use super::*;

    fn directory(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = env::temp_dir().join(format!(
            "noar-native-output-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn video(width: u32, height: u32, pts: i64) -> frame::Video {
        let mut frame = frame::Video::new(Pixel::YUV420P, width, height);
        frame.set_pts(Some(pts));
        for plane in 0..3 {
            let stride = frame.stride(plane);
            let width = frame.plane_width(plane) as usize;
            let height = frame.plane_height(plane) as usize;
            for row in 0..height {
                frame.data_mut(plane)[row * stride..row * stride + width].fill(if plane == 0 {
                    40 + (pts % 170) as u8
                } else {
                    128
                });
            }
        }
        frame
    }

    fn encode_seconds(output: &mut EncodedOutput, width: u32, height: u32, seconds: i64) {
        for tick in 0..seconds * 25 {
            output.encode_video(&video(width, height, tick)).unwrap();
            let mut audio =
                frame::Audio::new(Sample::F32(Type::Planar), 1920, ChannelLayout::STEREO);
            audio.set_rate(48_000);
            audio.set_pts(Some(tick * 1920));
            for channel in 0..2 {
                for (index, sample) in audio.plane_mut::<f32>(channel).iter_mut().enumerate() {
                    let at = (tick * 1920 + index as i64) as f32 / 48_000.0;
                    *sample = (at * 440.0 * std::f32::consts::TAU).sin() * 0.2;
                }
            }
            output.encode_audio(&audio).unwrap();
        }
    }

    fn probe(path: &Path) -> String {
        let result = Command::new("ffprobe")
            .args(["-v", "error", "-show_streams", "-show_format"])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }

    #[test]
    fn i420_transfer_honors_native_plane_alignment() {
        crate::gstreamer::init().unwrap();
        let mut frame = video(34, 26, 0);
        for plane in 0..3 {
            let stride = frame.stride(plane);
            let width = frame.plane_width(plane) as usize;
            let height = frame.plane_height(plane) as usize;
            for row in 0..height {
                frame.data_mut(plane)[row * stride..row * stride + width]
                    .fill((plane * 50 + row) as u8);
            }
        }
        let buffer = packed_video_buffer(&frame).unwrap();
        let mapped = buffer.map_readable().unwrap();
        let info = gstreamer_video::VideoInfo::builder(gstreamer_video::VideoFormat::I420, 34, 26)
            .build()
            .unwrap();
        assert_eq!(buffer.size(), info.size());
        for plane in 0..3 {
            for row in 0..frame.plane_height(plane) as usize {
                let start = info.offset()[plane] + row * info.stride()[plane] as usize;
                assert!(
                    mapped[start..start + frame.plane_width(plane) as usize]
                        .iter()
                        .all(|value| *value == (plane * 50 + row) as u8)
                );
            }
        }
    }

    #[test]
    fn native_file_output_keeps_audio_and_video_on_the_same_timeline() {
        let directory = directory("file");
        let path = directory.join("channel.ts");
        let cfg = OutputConfig::new(34, 26, 25, 48_000);
        let mut output =
            EncodedOutput::open(path.to_str().unwrap(), &cfg, EncodedFormat::Auto).unwrap();
        encode_seconds(&mut output, 34, 26, 2);
        output.finish().unwrap();
        let result = probe(&path);
        assert!(result.contains("codec_type=video"), "{result}");
        assert!(result.contains("codec_type=audio"), "{result}");
        assert!(result.contains("width=34"), "{result}");
        let starts: Vec<f64> = result
            .lines()
            .filter_map(|line| line.strip_prefix("start_time=")?.parse().ok())
            .collect();
        assert!(starts.len() >= 2);
        assert!(
            (starts[0] - starts[1]).abs() < 0.1,
            "encoded segments lost their timing: {starts:?}"
        );
        assert!(
            result
                .lines()
                .filter_map(|line| line.strip_prefix("duration=")?.parse::<f64>().ok())
                .all(|duration| duration < 3.0),
            "{result}"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn native_hls_finalizes_playable_segments_and_vtt() {
        let directory = directory("hls");
        let path = directory.join("index.m3u8");
        let cfg = OutputConfig::new(64, 48, 25, 48_000);
        let subtitle = HlsSubtitle {
            name: "English".into(),
            language: "en".into(),
            default: true,
        };
        let health = HlsHealth::new();
        *health.last_muxed_at.lock().unwrap() = Instant::now() - Duration::from_secs(60);
        let mut output = EncodedOutput::open_with_hls_health(
            path.to_str().unwrap(),
            &cfg,
            EncodedFormat::Hls {
                variants: Vec::new(),
                subtitle: Some(subtitle),
                segment_seconds: 1,
                list_size: 5,
            },
            Some(health.clone()),
        )
        .unwrap();
        output.pace = false;
        let media = directory.join("source.mp4");
        fs::write(
            media.with_extension("vtt"),
            "WEBVTT\n\n00:00:00.100 --> 00:00:01.600\nNative caption\n",
        )
        .unwrap();
        output
            .write_vtt_subtitles(media.to_str().unwrap(), 0, 0)
            .unwrap();
        output.advance_vtt_subtitles(100).unwrap();
        encode_seconds(&mut output, 64, 48, 3);
        output.finish().unwrap();
        assert!(health.last_muxed_age() < Duration::from_secs(1));
        let playlist = fs::read_to_string(&path).unwrap();
        let segments: Vec<_> = playlist
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert!(!segments.is_empty(), "{playlist}");
        for uri in &segments {
            let result = probe(&directory.join(uri));
            assert!(result.contains("codec_type=video") && result.contains("codec_type=audio"));
        }
        assert!(directory.join("master.m3u8").is_file());
        let subtitles = fs::read_to_string(directory.join("index_vtt.m3u8")).unwrap();
        assert!(subtitles.contains(".vtt"));
        assert!(segments.iter().any(|uri| {
            fs::read_to_string(directory.join(uri).with_extension("vtt"))
                .is_ok_and(|text| text.contains("Native caption"))
        }));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[cfg(feature = "desktop-base")]
    fn standalone_recording_uses_its_encode_dimensions() {
        let directory = directory("desktop-recording");
        let cfg = OutputConfig::new(320, 180, 25, 48_000);
        let mut recording = RecordingConfig::new(directory.to_string_lossy(), 30);
        recording.encode = Some(crate::RecordingEncodeConfig {
            width: 160,
            height: 90,
            video_codec: cfg.video_codec.clone(),
            video_options: cfg.video_options.clone(),
            audio_codec: cfg.audio_codec.clone(),
            audio_options: cfg.audio_options.clone(),
            audio_bitrate: cfg.audio_bitrate,
        });
        let mut output = EncodedOutput::open_recording(&cfg, &recording).unwrap();
        output.pace = false;
        encode_seconds(&mut output, 320, 180, 1);
        output.finish().unwrap();
        let path = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|extension| extension == "mkv"))
            .unwrap();
        let result = probe(&path);

        assert!(result.contains("width=160"), "{result}");
        assert!(result.contains("height=90"), "{result}");
        assert!(result.contains("codec_type=audio"), "{result}");
        fs::remove_dir_all(directory).unwrap();
    }
}
