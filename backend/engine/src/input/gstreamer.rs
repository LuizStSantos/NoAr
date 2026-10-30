//! GStreamer source decoding and bounded raw-frame delivery.
//!
//! AVFrame is retained only as the CPU compositor's in-memory interchange
//! format. All input, decoding, scaling, frame-rate conversion and resampling
//! happen in GStreamer before a sample crosses this boundary.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use ::gstreamer::{self as gst, prelude::*};
use anyhow::{Context, Result, anyhow, bail, ensure};
use ffmpeg_next::{
    frame,
    util::{channel_layout::ChannelLayout, format::pixel::Pixel, format::sample::Sample},
};
use gstreamer_app::AppSink;
use gstreamer_video::{VideoFrameExt, VideoFrameRef, VideoInfo};
use url::{Host, Url};

use crate::{
    benchmark::{self, Stage},
    utils::{config::OutputConfig, helper::is_network_url},
};

const OPEN_TIMEOUT: Duration = Duration::from_secs(12);
const POLL_INTERVAL: Duration = Duration::from_millis(2);

#[derive(Debug, Clone, Default)]
pub(crate) struct SourceInfo {
    pub(crate) has_video: bool,
    pub(crate) has_audio: bool,
    pub(crate) duration: Option<f64>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: f64,
}

pub(crate) enum FrameEvent {
    Video(frame::Video),
    Audio(frame::Audio),
}

pub(crate) struct Source {
    pipeline: gst::Pipeline,
    video: AppSink,
    audio: AppSink,
    metadata: Arc<Mutex<SourceInfo>>,
    error: Arc<Mutex<Option<String>>>,
    abort: Arc<AtomicBool>,
    pending_video: Option<gst::Sample>,
    pending_audio: Option<gst::Sample>,
    fps: u32,
    sample_rate: u32,
    eos: bool,
    first_samples: [bool; 2],
    selected_streams: Option<(bool, bool)>,
}

impl Source {
    pub(crate) fn open(path: &str, config: &OutputConfig) -> Result<Self> {
        Self::open_with_options(path, config, &BTreeMap::new(), &BTreeMap::new())
    }

    pub(crate) fn open_with_options(
        path: &str,
        config: &OutputConfig,
        protocol_options: &BTreeMap<String, String>,
        demuxer_options: &BTreeMap<String, String>,
    ) -> Result<Self> {
        Self::open_with_options_and_cancel(
            path,
            config,
            protocol_options,
            demuxer_options,
            Arc::new(AtomicBool::new(false)),
        )
    }

    pub(crate) fn open_with_options_and_cancel(
        path: &str,
        config: &OutputConfig,
        protocol_options: &BTreeMap<String, String>,
        demuxer_options: &BTreeMap<String, String>,
        abort: Arc<AtomicBool>,
    ) -> Result<Self> {
        crate::gstreamer::init()?;
        ensure!(
            config.width > 0 && config.height > 0 && config.fps > 0 && config.sample_rate > 0,
            "source normalization dimensions, frame rate and sample rate must be positive"
        );
        ensure!(
            demuxer_options.is_empty(),
            "FFmpeg demuxer options are not supported by the GStreamer engine; clear demuxer options before starting this input"
        );

        let (uri, protocol_options) = source_uri(path, protocol_options)?;
        let pipeline = gst::Pipeline::new();
        let decoder = gst::ElementFactory::make("uridecodebin3")
            .property("uri", &uri)
            .build()
            .context("GStreamer uridecodebin3 is unavailable")?;
        let video_bin = gst::parse::bin_from_description(
            &format!(
                "queue max-size-buffers=4 max-size-bytes=0 max-size-time=0 ! \
                 videoconvert ! videoscale add-borders=true ! \
                 videorate skip-to-first=true ! \
                 video/x-raw,format=I420,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! \
                 appsink name=video-samples sync=false async=false max-buffers=2 wait-on-eos=false",
                config.width, config.height, config.fps,
            ),
            true,
        )?;
        let audio_bin = normalized_audio_bin(config.sample_rate)?;
        let video = video_bin
            .by_name("video-samples")
            .context("video sample sink is missing")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow!("video sample sink has the wrong type"))?;
        let audio = audio_bin
            .by_name("audio-samples")
            .context("audio sample sink is missing")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow!("audio sample sink has the wrong type"))?;
        pipeline.add_many([&decoder, video_bin.upcast_ref(), audio_bin.upcast_ref()])?;

        let metadata = Arc::new(Mutex::new(SourceInfo::default()));
        let error = Arc::new(Mutex::new(None));
        let complete = Arc::new(AtomicBool::new(false));
        let source_metadata = Arc::clone(&metadata);
        let source_error = Arc::clone(&error);
        let video_sink = video_bin
            .static_pad("sink")
            .context("video input pad missing")?;
        let audio_sink = audio_bin
            .static_pad("sink")
            .context("audio input pad missing")?;
        decoder.connect_pad_added(move |_, pad| {
            let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
            let Some(structure) = caps.structure(0) else {
                return;
            };
            let name = structure.name();
            let mut info = source_metadata
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let target = if name.starts_with("video/x-raw") && !video_sink.is_linked() {
                info.has_video = true;
                info.width = structure.get::<i32>("width").unwrap_or_default().max(0) as u32;
                info.height = structure.get::<i32>("height").unwrap_or_default().max(0) as u32;
                info.fps = structure
                    .get::<gst::Fraction>("framerate")
                    .map_or(0.0, |rate| {
                        f64::from(rate.numer()) / f64::from(rate.denom().max(1))
                    });
                Some(&video_sink)
            } else if name.starts_with("audio/x-raw") && !audio_sink.is_linked() {
                info.has_audio = true;
                Some(&audio_sink)
            } else {
                None
            };
            if let Some(target) = target
                && let Err(link_error) = pad.link(target)
            {
                *source_error.lock().unwrap_or_else(PoisonError::into_inner) = Some(format!(
                    "cannot attach decoded GStreamer stream: {link_error:?}"
                ));
            }
        });
        let source_complete = Arc::clone(&complete);
        decoder.connect_no_more_pads(move |_| source_complete.store(true, Ordering::Release));
        let options = protocol_options;
        let options_error = Arc::clone(&error);
        decoder.connect("source-setup", false, move |values| {
            if let Ok(source) = values[1].get::<gst::Element>()
                && let Err(error) = configure_source(&source, &options)
            {
                *options_error.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(error.to_string());
            }
            None
        });

        let mut source = Self {
            pipeline,
            video,
            audio,
            metadata,
            error,
            abort,
            pending_video: None,
            pending_audio: None,
            fps: config.fps,
            sample_rate: config.sample_rate,
            eos: false,
            first_samples: [true; 2],
            selected_streams: None,
        };
        // Negotiate seekable files while paused. Starting them in PLAYING and
        // immediately pausing a full appsink can race the first flushing seek.
        let initial_state = if is_network_url(path) {
            gst::State::Playing
        } else {
            gst::State::Paused
        };
        source.pipeline.set_state(initial_state)?;
        let deadline = Instant::now() + OPEN_TIMEOUT;

        loop {
            source.check_bus()?;
            ensure!(
                !source.abort.load(Ordering::Acquire),
                "source opening cancelled"
            );
            let info = source.info();
            let selected_ready = source.selected_streams.is_some_and(|(video, audio)| {
                (video || audio) && (!video || info.has_video) && (!audio || info.has_audio)
            });
            if (info.has_video && info.has_audio)
                || selected_ready
                || (complete.load(Ordering::Acquire) && (info.has_video || info.has_audio))
            {
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "GStreamer source opening deadline: video={}, audio={}, discovery_complete={}",
                info.has_video,
                info.has_audio,
                complete.load(Ordering::Acquire)
            );
            thread::sleep(Duration::from_millis(10));
        }

        if let Some(duration) = source.pipeline.query_duration::<gst::ClockTime>() {
            source
                .metadata
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .duration =
                Some(duration.nseconds() as f64 / gst::ClockTime::SECOND.nseconds() as f64);
        }
        log::debug!("GStreamer decoded streams: {:?}", source.info());

        Ok(source)
    }

    pub(crate) fn info(&self) -> SourceInfo {
        self.metadata
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn start(&mut self, seek_seconds: Option<f64>) -> Result<()> {
        self.check_bus()?;
        ensure!(
            seek_seconds.is_none_or(|seconds| seconds.is_finite() && seconds >= 0.0),
            "seek position must be finite and nonnegative"
        );

        if let Some(seconds) = seek_seconds.filter(|seconds| *seconds > 0.0) {
            ensure!(seconds.is_finite(), "seek position must be finite");
            self.pipeline.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds((seconds * 1_000_000_000.0).round() as u64),
            )?;
            self.pending_video = None;
            self.pending_audio = None;
            self.eos = false;
        }
        self.pipeline.set_state(gst::State::Playing)?;

        Ok(())
    }

    pub(crate) fn pause(&self) -> Result<()> {
        self.pipeline.set_state(gst::State::Paused)?;

        Ok(())
    }

    pub(crate) fn next(&mut self, timeout: Duration) -> Result<Option<FrameEvent>> {
        let deadline = Instant::now() + timeout;

        loop {
            self.check_bus()?;
            ensure!(
                !self.abort.load(Ordering::Acquire),
                "source reading cancelled"
            );
            if self.pending_video.is_none() {
                self.pending_video = self.video.try_pull_sample(gst::ClockTime::ZERO);
            }
            if self.pending_audio.is_none() {
                self.pending_audio = self.audio.try_pull_sample(gst::ClockTime::ZERO);
            }
            for pending in [&mut self.pending_video, &mut self.pending_audio] {
                if pending.as_ref().is_some_and(|sample| {
                    sample.buffer().and_then(gst::BufferRef::pts).is_some()
                        && sample_timestamp(sample).is_none()
                }) {
                    // Decoder preroll outside the selected segment has no
                    // stream timestamp and must not reset the media origin.
                    *pending = None;
                }
            }
            let video_first = match (&self.pending_video, &self.pending_audio) {
                (Some(video), Some(audio)) => sample_pts(video) <= sample_pts(audio),
                (Some(_), None) => true,
                _ => false,
            };

            if video_first {
                let sample = self.pending_video.take().context("video sample missing")?;
                self.log_first_sample(&sample, 0);

                return Ok(Some(FrameEvent::Video(benchmark::measure_success(
                    Stage::InputVideo,
                    || video_frame(&sample, self.fps),
                )?)));
            }
            if let Some(sample) = self.pending_audio.take() {
                self.log_first_sample(&sample, 1);
                return Ok(Some(FrameEvent::Audio(benchmark::measure_success(
                    Stage::InputAudio,
                    || audio_frame(&sample, self.sample_rate),
                )?)));
            }
            if self.is_eos() || Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub(crate) fn video_ended(&self) -> bool {
        self.pending_video.is_none() && self.video.is_eos()
    }

    fn log_first_sample(&mut self, sample: &gst::Sample, track: usize) {
        if std::mem::take(&mut self.first_samples[track]) {
            log::debug!(
                "GStreamer first {} sample: PTS {:?}, segment {:?}",
                if track == 0 { "video" } else { "audio" },
                sample.buffer().and_then(gst::BufferRef::pts),
                sample.segment()
            );
        }
    }

    pub(crate) fn audio_ended(&self) -> bool {
        self.pending_audio.is_none() && self.audio.is_eos()
    }

    pub(crate) fn is_eos(&self) -> bool {
        let info = self.info();
        self.pending_video.is_none()
            && self.pending_audio.is_none()
            && (self.eos
                || ((!info.has_video || self.video.is_eos())
                    && (!info.has_audio || self.audio.is_eos())))
    }

    fn check_bus(&mut self) -> Result<()> {
        if let Some(error) = self
            .error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            bail!(error);
        }
        let bus = self
            .pipeline
            .bus()
            .context("GStreamer source bus is missing")?;

        while let Some(message) = bus.pop() {
            match message.view() {
                gst::MessageView::Error(error) => bail!("GStreamer source: {}", error.error()),
                gst::MessageView::Eos(_) => self.eos = true,
                gst::MessageView::Latency(_) => {
                    self.pipeline.recalculate_latency()?;
                }
                gst::MessageView::StreamsSelected(selected) => {
                    let mut video = false;
                    let mut audio = false;
                    for stream in selected.streams() {
                        video |= stream.stream_type().contains(gst::StreamType::VIDEO);
                        audio |= stream.stream_type().contains(gst::StreamType::AUDIO);
                    }
                    self.selected_streams = Some((video, audio));
                }
                _ => {}
            }
        }

        Ok(())
    }
}

fn source_uri(
    path: &str,
    options: &BTreeMap<String, String>,
) -> Result<(String, BTreeMap<String, String>)> {
    if path
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("srt://"))
    {
        let normalized =
            crate::output::link::normalize_srt_uri(path, options).map_err(anyhow::Error::msg)?;
        let uri = Url::parse(&normalized)?;
        ensure!(
            uri.query_pairs()
                .all(|(name, value)| name != "mode" || value == "caller"),
            "NoAr accepts SRT client URLs only; publish to external MediaMTX and configure its read URL"
        );
        ensure!(
            !matches!(uri.host_str(), None | Some("0.0.0.0" | "[::]" | "::")),
            "SRT input must name the external media server"
        );

        // The normalized URI already contains every native SRT option. Applying
        // source-setup options again would convert latency units twice.
        return Ok((normalized, BTreeMap::new()));
    }
    ensure!(
        !options
            .get("listen")
            .is_some_and(|value| value != "0" && value != "false"),
        "input listener/server mode was removed; use an external media server's read URL"
    );
    let uri = if path.contains("://") {
        path.to_string()
    } else {
        gst::glib::filename_to_uri(Path::new(path).canonicalize()?, None)?.to_string()
    };

    Ok((
        uri,
        options
            .iter()
            .filter(|(key, _)| key.as_str() != "listen")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    ))
}

impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

fn normalized_audio_bin(sample_rate: u32) -> Result<gst::Bin> {
    // MPEG-TS PCR adjustment and millisecond FLV timestamps can jitter while
    // the audio samples themselves remain continuous. Normalize their clock
    // natively before the playout compares them with the output timeline.
    Ok(gst::parse::bin_from_description(
        &format!(
            "queue max-size-buffers=8 max-size-bytes=0 max-size-time=0 ! \
             audioconvert ! audioresample ! \
             audio/x-raw,format=F32LE,layout=interleaved,rate={sample_rate},channels=2 ! \
             audiorate skip-to-first=true tolerance=40000000 ! \
             appsink name=audio-samples sync=false async=false max-buffers=4 wait-on-eos=false"
        ),
        true,
    )?)
}

fn sample_pts(sample: &gst::Sample) -> u64 {
    sample_timestamp(sample).map_or(0, gst::ClockTime::nseconds)
}

fn sample_timestamp(sample: &gst::Sample) -> Option<gst::ClockTime> {
    let pts = sample.buffer()?.pts()?;
    // Adaptive inputs can change the raw timestamp origin per segment. Use
    // stream time to keep those segments on one media clock. Unlike running
    // time, stream time also preserves the position of a seeked file.
    match sample
        .segment()
        .and_then(|s| s.downcast_ref::<gst::ClockTime>())
    {
        Some(segment) => segment.to_stream_time(pts),
        None => Some(pts),
    }
}

pub(crate) fn video_frame(sample: &gst::Sample, fps: u32) -> Result<frame::Video> {
    let buffer = sample
        .buffer()
        .context("decoded video sample has no buffer")?;
    let caps = sample.caps().context("decoded video sample has no caps")?;
    let info = VideoInfo::from_caps(caps)?;
    ensure!(
        info.format() == gstreamer_video::VideoFormat::I420,
        "decoded video is not I420"
    );
    let mapped = VideoFrameRef::from_buffer_ref_readable(buffer, &info)?;
    let mut frame = frame::Video::new(Pixel::YUV420P, info.width(), info.height());

    for plane in 0..3 {
        let source = mapped.plane_data(plane as u32)?;
        let source_stride = usize::try_from(mapped.plane_stride()[plane])?;
        let width = if plane == 0 {
            info.width()
        } else {
            info.width().div_ceil(2)
        } as usize;
        let height = if plane == 0 {
            info.height()
        } else {
            info.height().div_ceil(2)
        } as usize;
        let target_stride = frame.stride(plane);
        let target = frame.data_mut(plane);

        for row in 0..height {
            target[row * target_stride..row * target_stride + width]
                .copy_from_slice(&source[row * source_stride..row * source_stride + width]);
        }
    }
    frame.set_pts(
        sample_timestamp(sample)
            .map(|pts| (u128::from(pts.nseconds()) * u128::from(fps) / 1_000_000_000) as i64),
    );

    Ok(frame)
}

pub(crate) fn audio_frame(sample: &gst::Sample, sample_rate: u32) -> Result<frame::Audio> {
    let buffer = sample
        .buffer()
        .context("decoded audio sample has no buffer")?;
    let mapped = buffer.map_readable()?;
    ensure!(
        mapped.len().is_multiple_of(8),
        "decoded stereo f32 audio has a partial sample"
    );
    let samples = mapped.len() / 8;
    let mut frame = frame::Audio::new(
        Sample::F32(ffmpeg_next::format::sample::Type::Planar),
        samples,
        ChannelLayout::STEREO,
    );
    frame.set_rate(sample_rate);

    for channel in 0..2 {
        for (index, value) in frame.plane_mut::<f32>(channel).iter_mut().enumerate() {
            let start = index * 8 + channel * 4;
            *value = f32::from_le_bytes(mapped[start..start + 4].try_into()?);
        }
    }
    frame.set_pts(
        sample_timestamp(sample).map(|pts| {
            (u128::from(pts.nseconds()) * u128::from(sample_rate) / 1_000_000_000) as i64
        }),
    );

    Ok(frame)
}

fn configure_source(source: &gst::Element, options: &BTreeMap<String, String>) -> Result<()> {
    let factory = source
        .factory()
        .map(|factory| factory.name().to_string())
        .unwrap_or_default();
    if factory == "souphttpsrc" {
        let location = source.property::<String>("location");
        let exclusions = std::env::var("no_proxy")
            .or_else(|_| std::env::var("NO_PROXY"))
            .unwrap_or_default();
        if loopback_proxy_exception(&location, &exclusions) {
            // souphttpsrc copies http_proxy into a resolver with no ignore
            // list. Clearing that override restores GIO's system resolver,
            // which honors the explicit loopback no_proxy exception.
            source.set_property("proxy", "");
        }
    }
    if factory == "rtspsrc" {
        set_property(source, "protocols", "tcp")?;
        set_property(source, "latency", "120")?;
    }

    for (name, value) in options {
        let (property, value) = match name.as_str() {
            "mode" | "passphrase" | "pbkeylen" | "streamid" => (name.as_str(), value.clone()),
            "latency" if factory == "srtsrc" => {
                let microseconds: u64 = value.parse().context("invalid SRT latency")?;
                ("latency", microseconds.div_ceil(1_000).to_string())
            }
            "rtsp_transport" if factory == "rtspsrc" => ("protocols", value.clone()),
            "buffer_size" if factory == "udpsrc" => ("buffer-size", value.clone()),
            "timeout" | "rw_timeout" if factory == "souphttpsrc" || factory == "rtspsrc" => {
                let microseconds: u64 = value.parse().context("invalid input timeout")?;
                if factory == "souphttpsrc" {
                    ("timeout", microseconds.div_ceil(1_000_000).to_string())
                } else {
                    ("timeout", microseconds.to_string())
                }
            }
            "listen" if value == "0" => continue,
            "listen" => bail!(
                "GStreamer RTMP input requires the managed ingest relay, not an FFmpeg listen option"
            ),
            _ => bail!("input option {name} is not supported by GStreamer source {factory}"),
        };
        set_property(source, property, &value)?;
    }

    Ok(())
}

fn loopback_proxy_exception(location: &str, exclusions: &str) -> bool {
    let Ok(uri) = Url::parse(location) else {
        return false;
    };
    let loopback = match uri.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if !loopback {
        return false;
    }
    let host = uri.host_str().unwrap_or_default();

    exclusions.split(',').any(|entry| {
        let entry = entry.trim();
        entry == "*"
            || entry
                .trim_matches(['[', ']'])
                .eq_ignore_ascii_case(host.trim_matches(['[', ']']))
    })
}

fn set_property(element: &gst::Element, name: &str, text: &str) -> Result<()> {
    let specification = element
        .find_property(name)
        .with_context(|| format!("GStreamer input does not provide property {name}"))?;
    let value = gst::glib::Value::deserialize(text, specification.value_type())
        .with_context(|| format!("invalid value for GStreamer input property {name}"))?;
    element.set_property_from_value(name, &value);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn decoded_frames_use_stream_time_across_adaptive_timestamp_origins_and_file_seeks() {
        crate::gstreamer::init().unwrap();
        let info = VideoInfo::builder(gstreamer_video::VideoFormat::I420, 64, 64)
            .build()
            .unwrap();
        let caps = info.to_caps().unwrap();
        for (start, time, raw, expected) in [(66, 4, 66, 4), (3, 6, 3, 6), (9, 9, 10, 10)] {
            let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
            segment.set_start(gst::ClockTime::from_seconds(start));
            segment.set_time(gst::ClockTime::from_seconds(time));
            let mut buffer = gst::Buffer::with_size(info.size()).unwrap();
            buffer
                .get_mut()
                .unwrap()
                .set_pts(gst::ClockTime::from_seconds(raw));
            let video = gst::Sample::builder()
                .buffer(&buffer)
                .caps(&caps)
                .segment(&segment)
                .build();
            assert_eq!(
                video_frame(&video, 25).unwrap().pts(),
                Some(expected as i64 * 25)
            );
            let mut buffer = gst::Buffer::with_size(8 * 1024).unwrap();
            buffer
                .get_mut()
                .unwrap()
                .set_pts(gst::ClockTime::from_seconds(raw));
            let audio = gst::Sample::builder()
                .buffer(&buffer)
                .segment(&segment)
                .build();
            assert_eq!(
                audio_frame(&audio, 48_000).unwrap().pts(),
                Some(expected as i64 * 48_000)
            );
            assert_eq!(sample_pts(&audio), expected * 1_000_000_000);
        }
    }

    #[test]
    fn native_audio_normalization_preserves_samples_through_timestamp_jitter() {
        crate::gstreamer::init().unwrap();
        let pipeline = gst::Pipeline::new();
        let source = gstreamer_app::AppSrc::builder()
            .format(gst::Format::Time)
            .caps(
                &gst::Caps::builder("audio/x-raw")
                    .field("format", "F32LE")
                    .field("layout", "interleaved")
                    .field("rate", 48_000_i32)
                    .field("channels", 2_i32)
                    .build(),
            )
            .build();
        let normalization = normalized_audio_bin(48_000).unwrap();
        let sink = normalization
            .by_name("audio-samples")
            .unwrap()
            .downcast::<AppSink>()
            .unwrap();
        pipeline
            .add_many([
                source.upcast_ref::<gst::Element>(),
                normalization.upcast_ref(),
            ])
            .unwrap();
        source.link(&normalization).unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let mut expected = Vec::new();
        for packet in 0..48_u64 {
            let mut bytes = Vec::new();
            for sample in packet * 1024..(packet + 1) * 1024 {
                let value = ((sample % 251) as f32 / 251.0).to_le_bytes();
                bytes.extend_from_slice(&value);
                bytes.extend_from_slice(&value);
            }
            expected.extend_from_slice(&bytes);
            let mut buffer = gst::Buffer::from_mut_slice(bytes);
            let jitter = if packet % 3 == 0 {
                -5_000_000_i64
            } else {
                5_000_000
            };
            let rounded = packet * 1024 * 1000 / 48_000 * 1_000_000;
            buffer
                .get_mut()
                .unwrap()
                .set_pts(gst::ClockTime::from_nseconds(
                    (1_000_000_000 + rounded as i64 + if packet == 0 { 0 } else { jitter }) as u64,
                ));
            source.push_buffer(buffer).unwrap();
        }
        source.end_of_stream().unwrap();
        let mut actual = Vec::new();
        while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(1)) {
            let buffer = sample.buffer().unwrap();
            let pts = buffer.pts().unwrap().nseconds();
            let next = 1_000_000_000 + actual.len() as u64 / 8 * 1_000_000_000 / 48_000;
            assert!(pts.abs_diff(next) <= 1);
            actual.extend_from_slice(buffer.map_readable().unwrap().as_slice());
        }
        pipeline.set_state(gst::State::Null).unwrap();

        assert_eq!(actual.len(), expected.len());
        assert!(
            actual == expected,
            "audio samples changed while normalizing timestamp jitter"
        );
    }

    #[test]
    fn stream_selection_opens_sources_with_only_one_track() {
        crate::gstreamer::init().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "noar-selected-streams-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        for (name, description, video) in [
            (
                "video.mp4",
                "videotestsrc num-buffers=5 ! video/x-raw,width=64,height=64,framerate=25/1 ! x264enc tune=zerolatency ! h264parse ! qtmux",
                true,
            ),
            (
                "audio.wav",
                "audiotestsrc num-buffers=5 ! audio/x-raw,rate=48000 ! wavenc",
                false,
            ),
        ] {
            let path = directory.join(name);
            let pipeline = gst::parse::launch(&format!(
                "{description} ! filesink location=\"{}\"",
                path.display()
            ))
            .unwrap();
            pipeline.set_state(gst::State::Playing).unwrap();
            let message = pipeline
                .bus()
                .unwrap()
                .timed_pop_filtered(
                    gst::ClockTime::from_seconds(5),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                )
                .unwrap();
            pipeline.set_state(gst::State::Null).unwrap();
            assert!(
                matches!(message.view(), gst::MessageView::Eos(_)),
                "{message:?}"
            );
            let config = OutputConfig {
                width: 64,
                height: 64,
                fps: 25,
                ..OutputConfig::default()
            };
            let mut source = Source::open(path.to_str().unwrap(), &config).unwrap();
            assert_eq!(
                (source.info().has_video, source.info().has_audio),
                (video, !video)
            );
            source.start(None).unwrap();
            let first = source.next(Duration::from_secs(1)).unwrap().unwrap();
            assert_eq!(matches!(first, FrameEvent::Video(_)), video);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explicit_proxy_exceptions_are_limited_to_exact_loopback_hosts() {
        assert!(loopback_proxy_exception(
            "http://127.0.0.1:21988/input/index.m3u8",
            "localhost, 127.0.0.1"
        ));
        assert!(loopback_proxy_exception(
            "http://localhost:8080/",
            "LOCALHOST"
        ));
        assert!(loopback_proxy_exception("http://[::1]:8080/", "::1"));
        assert!(!loopback_proxy_exception(
            "http://127.0.0.1:8080/",
            "localhost"
        ));
        assert!(!loopback_proxy_exception("http://127.0.0.1:8080/", ""));
        assert!(!loopback_proxy_exception(
            "https://localhost.example.org/",
            "localhost"
        ));
        assert!(!loopback_proxy_exception("https://example.org/", "*"));
    }
}
