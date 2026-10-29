use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant, SystemTime},
};

use ::gstreamer as gst;
use anyhow::{Context, Result, anyhow, bail, ensure};
use gst::prelude::*;
use gst_pbutils::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_pbutils as gst_pbutils;
use log::{error, info};

const SILENCE_SAMPLE_RATE: u32 = 48_000;
const DISCOVERY_CACHE_LIMIT: usize = 128;

struct CachedDiscovery {
    path: PathBuf,
    size: u64,
    modified: SystemTime,
    info: gst_pbutils::DiscovererInfo,
}

static DISCOVERY_CACHE: Mutex<VecDeque<CachedDiscovery>> = Mutex::new(VecDeque::new());

#[derive(Debug, Clone, PartialEq)]
pub struct SilenceDetection {
    pub silent: bool,
    pub analyzed_seconds: f64,
}

pub struct MediaInfo {
    pub duration_seconds: Option<f64>,
    pub fps: Option<f64>,
    pub resolution: Option<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaProbe {
    pub format: ProbeFormat,
    pub audio: Vec<AudioStream>,
    pub video: Vec<VideoStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeFormat {
    pub duration: Option<f64>,
    pub nb_streams: i64,
    pub size: Option<i64>,
    pub bit_rate: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub channels: Option<i64>,
    pub codec_name: Option<String>,
    pub duration: Option<f64>,
    pub sample_rate: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub aspect_ratio: Option<String>,
    pub bit_rate: Option<i64>,
    pub codec_name: Option<String>,
    pub duration: Option<f64>,
    pub field_order: Option<String>,
    pub frame_rate: String,
    pub height: Option<i64>,
    pub nb_frames: Option<i64>,
    pub width: Option<i64>,
}

/// Convert an existing local path to a URI without interpolating it into a pipeline DSL.
pub(crate) fn media_uri(path: &str) -> Result<String> {
    if path.contains("://") {
        return Ok(path.to_owned());
    }

    let absolute = std::fs::canonicalize(path).with_context(|| format!("cannot open {path}"))?;
    Ok(gst::glib::filename_to_uri(absolute, None)?.to_string())
}

pub(crate) fn discover_media(path: &str) -> Result<gst_pbutils::DiscovererInfo> {
    crate::gstreamer::init()?;
    // Folder playout and day generation inspect the same files repeatedly.
    // Reuse immutable metadata, with bounded storage and file-change invalidation.
    // Network sources always require a fresh signal check.
    let file = (!path.contains("://"))
        .then(|| {
            let canonical = std::fs::canonicalize(path).ok()?;
            let metadata = std::fs::metadata(&canonical).ok()?;
            metadata
                .is_file()
                .then_some((canonical, metadata.len(), metadata.modified().ok()?))
        })
        .flatten();
    if let Some((canonical, size, modified)) = &file {
        let cache = DISCOVERY_CACHE
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(cached) = cache.iter().find(|entry| {
            entry.path == *canonical && entry.size == *size && entry.modified == *modified
        }) {
            return Ok(cached.info.clone());
        }
    }
    let discoverer = gst_pbutils::Discoverer::new(gst::ClockTime::from_seconds(15))?;
    let info = discoverer
        .discover_uri(&media_uri(path)?)
        .with_context(|| format!("discovering {path}"))?;
    ensure!(
        info.result() == gst_pbutils::DiscovererResult::Ok,
        "GStreamer could not discover {path}: {:?}",
        info.result()
    );
    if let Some((path, size, modified)) = file {
        let mut cache = DISCOVERY_CACHE
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cache.retain(|entry| entry.path != path);
        if cache.len() >= DISCOVERY_CACHE_LIMIT {
            cache.pop_front();
        }
        cache.push_back(CachedDiscovery {
            path,
            size,
            modified,
            info: info.clone(),
        });
    }
    Ok(info)
}

/// Keep the public probe's codec identifiers stable across media runtimes.
pub(crate) fn codec_name(caps: Option<&gst::CapsRef>) -> Option<String> {
    let structure = caps?.structure(0)?;
    let name = structure.name();
    let codec = match name.as_str() {
        "video/x-h264" => "h264",
        "video/x-h265" => "hevc",
        "video/x-vp8" => "vp8",
        "video/x-vp9" => "vp9",
        "video/x-av1" => "av1",
        "video/x-theora" => "theora",
        "video/x-prores" => "prores",
        "video/mpeg" => match structure.get::<i32>("mpegversion").ok() {
            Some(1) => "mpeg1video",
            Some(2) => "mpeg2video",
            Some(4) => "mpeg4",
            _ => "mpeg",
        },
        "image/jpeg" => "mjpeg",
        "image/png" => "png",
        "image/webp" => "webp",
        "audio/mpeg" => match structure.get::<i32>("mpegversion").ok() {
            Some(2 | 4) => "aac",
            _ => match structure.get::<i32>("layer").ok() {
                Some(1) => "mp1",
                Some(2) => "mp2",
                _ => "mp3",
            },
        },
        "audio/x-opus" => "opus",
        "audio/x-vorbis" => "vorbis",
        "audio/x-flac" => "flac",
        "audio/x-ac3" => "ac3",
        "audio/x-eac3" => "eac3",
        "audio/x-alaw" => "pcm_alaw",
        "audio/x-mulaw" => "pcm_mulaw",
        "audio/x-raw" => {
            return structure
                .get::<String>("format")
                .ok()
                .map(|format| format!("pcm_{}", format.to_ascii_lowercase()));
        }
        _ => return Some(name.to_string()),
    };
    Some(codec.to_owned())
}

pub fn print_media_info(path: &str) {
    match probe_media_info(path) {
        Ok(info) => info!(
            "playing: {path} (length: {}, fps: {}, resolution: {})",
            format_duration(info.duration_seconds),
            format_fps(info.fps),
            format_resolution(info.resolution)
        ),
        Err(error) => error!("playing: {path} (metadata unavailable: {error})"),
    }
}

pub fn probe_media_info(path: &str) -> Result<MediaInfo> {
    let info = discover_media(path)?;
    let video = info.video_streams().into_iter().next();
    Ok(MediaInfo {
        duration_seconds: info
            .duration()
            .filter(|duration| !duration.is_zero())
            .map(gstreamer::ClockTime::seconds_f64),
        fps: video.as_ref().and_then(|stream| {
            let rate = stream.framerate();
            (rate.numer() > 0).then(|| f64::from(rate.numer()) / f64::from(rate.denom()))
        }),
        resolution: video.map(|stream| (stream.width(), stream.height())),
    })
}

pub fn probe_media(path: &str) -> Result<MediaProbe> {
    let info = discover_media(path)?;
    let duration = info
        .duration()
        .filter(|duration| !duration.is_zero())
        .map(gstreamer::ClockTime::seconds_f64);
    let size = std::fs::metadata(Path::new(path))
        .ok()
        .map(|metadata| metadata.len() as i64);
    let format = ProbeFormat {
        duration,
        nb_streams: info.stream_list().len() as i64,
        size,
        bit_rate: size
            .zip(duration)
            .filter(|(_, seconds)| *seconds > 0.0)
            .map(|(bytes, seconds)| (bytes as f64 * 8.0 / seconds) as i64),
    };
    let audio = info
        .audio_streams()
        .into_iter()
        .map(|stream| AudioStream {
            channels: (stream.channels() > 0).then_some(i64::from(stream.channels())),
            codec_name: codec_name(stream.caps().as_deref()),
            duration,
            sample_rate: (stream.sample_rate() > 0).then_some(i64::from(stream.sample_rate())),
        })
        .collect();
    let video = info
        .video_streams()
        .into_iter()
        .map(|stream| {
            let rate = stream.framerate();
            VideoStream {
                aspect_ratio: Some(format!("{}:{}", stream.width(), stream.height())),
                bit_rate: (stream.bitrate() > 0).then_some(i64::from(stream.bitrate())),
                codec_name: codec_name(stream.caps().as_deref()),
                duration,
                field_order: Some(
                    if stream.is_interlaced() {
                        "tt"
                    } else {
                        "progressive"
                    }
                    .to_owned(),
                ),
                frame_rate: format!("{}/{}", rate.numer(), rate.denom()),
                height: Some(i64::from(stream.height())),
                nb_frames: None,
                width: Some(i64::from(stream.width())),
            }
        })
        .collect();
    Ok(MediaProbe {
        format,
        audio,
        video,
    })
}

pub fn detect_audio_silence(
    path: &str,
    seek_seconds: f64,
    duration_seconds: f64,
    threshold_db: f32,
    min_silence_seconds: f64,
) -> Result<SilenceDetection> {
    ensure!(
        seek_seconds.is_finite()
            && duration_seconds.is_finite()
            && threshold_db.is_finite()
            && min_silence_seconds.is_finite(),
        "silence detection settings must be finite"
    );
    ensure!(
        duration_seconds >= 0.0 && min_silence_seconds >= 0.0,
        "silence detection durations must not be negative"
    );
    ensure!(
        !probe_media(path)?.audio.is_empty(),
        "input has no audio stream"
    );
    let pipeline = gst::Pipeline::new();
    let _shutdown = PipelineShutdown(pipeline.clone());
    let source = gst::ElementFactory::make("uridecodebin")
        .property("uri", media_uri(path)?)
        .build()?;
    let queue = gst::ElementFactory::make("queue").build()?;
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let resample = gst::ElementFactory::make("audioresample").build()?;
    let sink = gst_app::AppSink::builder()
        .caps(
            &gst::Caps::builder("audio/x-raw")
                .field("format", "F32LE")
                .field("layout", "interleaved")
                .field("rate", SILENCE_SAMPLE_RATE as i32)
                .field("channels", 2_i32)
                .build(),
        )
        .max_buffers(4)
        .sync(false)
        .build();
    pipeline.add_many([&source, &queue, &convert, &resample, sink.upcast_ref()])?;
    gst::Element::link_many([&queue, &convert, &resample, sink.upcast_ref()])?;
    let audio_pad = queue.static_pad("sink").context("silence audio sink pad")?;
    source.connect_pad_added(move |_, pad| {
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        if caps
            .structure(0)
            .is_some_and(|structure| structure.name().starts_with("audio/"))
            && !audio_pad.is_linked()
        {
            let _ = pad.link(&audio_pad);
        }
    });
    pipeline.set_state(gst::State::Paused)?;
    let (state, _, _) = pipeline.state(gst::ClockTime::from_seconds(10));
    state.context("waiting for silence detector preroll")?;
    if seek_seconds > 0.0 {
        pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::ClockTime::from_nseconds(
                (seek_seconds * gst::ClockTime::SECOND.nseconds() as f64) as u64,
            ),
        )?;
    }
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("silence detector bus")?;
    let max_seconds = duration_seconds.min(min_silence_seconds);
    let max_samples = (max_seconds * f64::from(SILENCE_SAMPLE_RATE)).ceil() as usize;
    let threshold = 10.0_f32.powf(threshold_db / 20.0).abs();
    let mut analyzed_samples = 0;
    let mut loud = false;
    let mut last_sample = Instant::now();
    while analyzed_samples < max_samples && !loud {
        if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
            let buffer = sample.buffer().context("silence detector buffer")?;
            let map = buffer.map_readable()?;
            let remaining = max_samples.saturating_sub(analyzed_samples);
            for frame in map.as_slice().as_chunks::<8>().0.iter().take(remaining) {
                analyzed_samples += 1;
                loud = frame.as_chunks::<4>().0.iter().any(|bytes| {
                    let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    !value.is_finite() || value.abs() > threshold
                });
                if loud {
                    break;
                }
            }
            last_sample = Instant::now();
        } else if sink.is_eos() {
            break;
        } else if last_sample.elapsed() > Duration::from_secs(15) {
            bail!("audio source stopped producing samples during silence detection");
        }
        for message in bus.iter() {
            if let gst::MessageView::Error(error) = message.view() {
                return Err(anyhow!("silence detection failed: {}", error.error()));
            }
        }
    }
    let analyzed_seconds = analyzed_samples as f64 / f64::from(SILENCE_SAMPLE_RATE);
    Ok(SilenceDetection {
        silent: !loud && analyzed_seconds >= min_silence_seconds,
        analyzed_seconds,
    })
}

struct PipelineShutdown(gst::Pipeline);

impl Drop for PipelineShutdown {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn format_duration(duration_seconds: Option<f64>) -> String {
    let Some(duration_seconds) = duration_seconds else {
        return "unknown".to_string();
    };
    let total_millis = (duration_seconds * 1_000.0).round().max(0.0) as u64;
    let total_seconds = total_millis / 1_000;
    let millis = total_millis % 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
    } else {
        format!("{minutes:02}:{seconds:02}.{millis:03}")
    }
}

fn format_fps(fps: Option<f64>) -> String {
    fps.map(|fps| format!("{fps:.3}"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn format_resolution(resolution: Option<(u32, u32)>) -> String {
    resolution
        .map(|(width, height)| format!("{width}x{height}"))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::detect_audio_silence;

    #[test]
    fn repeated_probing_observes_replacement_media_at_the_same_path() {
        let directory =
            std::env::temp_dir().join(format!("noar-discovery-cache-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("clip.mp4");
        std::fs::copy(media_mix_asset("ad.mp4"), &path).unwrap();
        for _ in 0..3 {
            let probe = super::probe_media(path.to_str().unwrap()).unwrap();
            assert!((probe.format.duration.unwrap() - 25.0).abs() < 0.1);
        }
        std::fs::copy(media_mix_asset("av_sync.mp4"), &path).unwrap();
        let probe = super::probe_media(path.to_str().unwrap()).unwrap();
        assert!((probe.format.duration.unwrap() - 30.0).abs() < 0.1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn media_mix_asset(name: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn silence_detection_rejects_media_without_audio_stream() {
        let error = detect_audio_silence(&media_mix_asset("no_audio.mp4"), 0.0, 15.0, -30.0, 15.0)
            .expect_err("no_audio.mp4 must not be analyzed as silent audio");

        assert!(
            error.to_string().contains("input has no audio stream"),
            "{error:#}"
        );
    }
}
