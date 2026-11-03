//! Convert media through the same mandatory native GStreamer runtime as playout.
//! Compatible H.264 video is parsed and remuxed; other video is fitted and
//! encoded. Audio is normalized to stereo AAC, preserving any initial silence.

use std::{
    path::Path,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use ::gstreamer as gst;
use anyhow::{Context, Result, anyhow, bail, ensure};
use gstreamer_pbutils::prelude::*;
use log::warn;

use crate::utils::media_info::{codec_name, discover_media, media_uri, probe_media};

/// What a file is converted to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConvertTarget {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub sample_rate: u32,
}

/// What a file already is, to tell whether it needs converting.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaShape {
    pub video_codec: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Frames per second, when the file says.
    pub fps: Option<f64>,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<u32>,
    pub audio_channels: Option<u16>,
}

impl MediaShape {
    /// The file already has the target's picture, frame rate and sound.
    pub fn matches(&self, target: &ConvertTarget) -> bool {
        self.video_codec.as_deref() == Some("h264")
            && self.width == target.width
            && self.height == target.height
            && self
                .fps
                .is_some_and(|fps| (fps - f64::from(target.fps)).abs() < 0.01)
            && self
                .audio_codec
                .as_deref()
                .is_none_or(|codec| codec == "aac")
            && self
                .sample_rate
                .is_none_or(|rate| rate == target.sample_rate)
            && self.audio_channels.is_none_or(|channels| channels == 2)
    }
}

/// Reads the encoded stream metadata with GStreamer, without decoding.
pub fn probe_shape(path: &Path) -> Result<MediaShape> {
    let probe = probe_media(path.to_string_lossy().as_ref())?;
    let video = probe.video.first();
    let audio = probe.audio.first();
    Ok(MediaShape {
        video_codec: video.and_then(|stream| stream.codec_name.clone()),
        width: video.and_then(|stream| stream.width).unwrap_or(0).max(0) as u32,
        height: video.and_then(|stream| stream.height).unwrap_or(0).max(0) as u32,
        fps: video.and_then(|stream| {
            let (numerator, denominator) = stream.frame_rate.split_once('/')?;
            let numerator = numerator.parse::<f64>().ok()?;
            let denominator = denominator.parse::<f64>().ok()?;
            (numerator > 0.0 && denominator > 0.0).then_some(numerator / denominator)
        }),
        audio_codec: audio.and_then(|stream| stream.codec_name.clone()),
        sample_rate: audio
            .and_then(|stream| stream.sample_rate)
            .map(|rate| rate as u32),
        audio_channels: audio
            .and_then(|stream| stream.channels)
            .map(|channels| channels as u16),
    })
}

/// Whether encoded video can be copied without changing its picture or timing.
pub fn video_matches(path: &Path, target: &ConvertTarget) -> Result<bool> {
    let discovered = discover_media(path.to_string_lossy().as_ref())?;
    let Some(video) = discovered.video_streams().into_iter().next() else {
        return Ok(false);
    };
    let caps = video.caps();
    let structure = caps.as_ref().and_then(|caps| caps.structure(0));
    let compatible_depth = structure.is_some_and(|structure| {
        let chroma = structure.get::<String>("chroma-format").ok();
        let profile = structure.get::<String>("profile").ok();
        let depth = structure.get::<u32>("bit-depth-luma").ok();
        chroma.as_deref().is_none_or(|chroma| chroma == "4:2:0")
            && depth.is_none_or(|depth| depth == 8)
            && profile.as_deref().is_none_or(|profile| {
                !profile.contains("10") && !profile.contains("4:4:4") && !profile.contains("4:2:2")
            })
    });
    let fps = video.framerate();
    Ok(codec_name(caps.as_deref()).as_deref() == Some("h264")
        && compatible_depth
        && !video.is_interlaced()
        && video.width() == target.width
        && video.height() == target.height
        && fps.numer() > 0
        && (f64::from(fps.numer()) / f64::from(fps.denom()) - f64::from(target.fps)).abs() < 0.01)
}

/// Convert the source to the target picture and sound. This blocks on its own
/// worker and checks cancellation while native decoding and encoding continue.
pub fn convert_to_output(
    input: &Path,
    output: &Path,
    target: ConvertTarget,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<()> {
    ensure!(
        target.width >= 2 && target.height >= 2 && target.fps > 0 && target.sample_rate > 0,
        "the output size, frame rate and sample rate must be set"
    );
    ensure!(!cancel.load(Ordering::Acquire), "conversion stopped");
    if video_matches(input, &target).unwrap_or(false) {
        match convert(input, output, target, cancel, &mut progress, true) {
            Ok(()) => return Ok(()),
            Err(error) if cancel.load(Ordering::Acquire) => return Err(error),
            Err(error) => {
                warn!(
                    "Copying the video of {} failed, encoding it instead: {error:#}",
                    input.display()
                );
                let _ = std::fs::remove_file(output);
            }
        }
    }
    convert(input, output, target, cancel, &mut progress, false)
}

fn convert(
    input: &Path,
    output: &Path,
    target: ConvertTarget,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(f64),
    copy_video: bool,
) -> Result<()> {
    crate::gstreamer::init()?;
    let metadata = probe_media(input.to_string_lossy().as_ref())?;
    ensure!(!metadata.video.is_empty(), "the file has no video");
    let pipeline = gst::Pipeline::new();
    let _shutdown = PipelineShutdown(pipeline.clone());
    let decoder = gst::ElementFactory::make("uridecodebin")
        .property("uri", media_uri(input.to_string_lossy().as_ref())?)
        .build()?;
    if copy_video {
        decoder.set_property("caps", gst::Caps::from_str("video/x-h264;audio/x-raw")?);
    }
    let extension = output
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4")
        .to_ascii_lowercase();
    let mux = match extension.as_str() {
        "mp4" | "m4v" | "mov" => gst::ElementFactory::make("mp4mux")
            .property("faststart", true)
            .build()?,
        "mkv" | "mka" | "webm" => gst::ElementFactory::make("matroskamux").build()?,
        "ts" | "mts" | "m2ts" => gst::ElementFactory::make("mpegtsmux").build()?,
        "flv" => gst::ElementFactory::make("flvmux").build()?,
        _ => bail!("unsupported GStreamer conversion container {extension:?}"),
    };
    let sink = gst::ElementFactory::make("filesink")
        .property("location", output.to_string_lossy().as_ref())
        .build()?;
    pipeline.add_many([&decoder, &mux, &sink])?;
    mux.link(&sink)?;
    let video = if copy_video {
        gst::parse::bin_from_description("queue ! h264parse", true)?
    } else {
        let threads = std::thread::available_parallelism()
            .map_or(1, |cores| cores.get() / 2)
            .max(1);
        gst::parse::bin_from_description(
            &format!(
                "queue ! videoconvert ! videoscale add-borders=true ! videorate ! video/x-raw,format=I420,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! x264enc speed-preset=veryfast pass=qual quantizer=18 threads={} key-int-max={} ! h264parse",
                target.width,
                target.height,
                target.fps,
                threads,
                target.fps * 2
            ),
            true,
        )?
    };
    pipeline.add(&video)?;
    video.link(&mux)?;
    let video_pad = video.static_pad("sink").context("conversion video pad")?;
    let audio_pad = if metadata.audio.is_empty() {
        None
    } else {
        let audio = gst::parse::bin_from_description(
            &format!(
                "queue ! audioconvert ! audioresample ! audiorate skip-to-first=false ! audio/x-raw,rate={},channels=2 ! audioconvert ! avenc_aac bitrate=192000 ! aacparse",
                target.sample_rate
            ),
            true,
        )?;
        pipeline.add(&audio)?;
        audio.link(&mux)?;
        Some(audio.static_pad("sink").context("conversion audio pad")?)
    };
    let link_error = Arc::new(Mutex::new(None));
    let link_error_worker = link_error.clone();
    decoder.connect_pad_added(move |_, pad| {
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let name = caps
            .structure(0)
            .map(|structure| structure.name().to_string())
            .unwrap_or_default();
        let target = if name.starts_with("video/") {
            Some(&video_pad)
        } else if name.starts_with("audio/") {
            audio_pad.as_ref()
        } else {
            None
        };
        if let Some(target) = target.filter(|target| !target.is_linked())
            && let Err(error) = pad.link(target)
        {
            *link_error_worker
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(error.to_string());
        }
    });
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("conversion bus")?;
    let mut last_reported = 0.0;
    loop {
        ensure!(!cancel.load(Ordering::Acquire), "conversion stopped");
        if let Some(error) = link_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            bail!("linking conversion input: {error}");
        }
        if let Some(message) = bus.timed_pop(gst::ClockTime::from_mseconds(100)) {
            match message.view() {
                gst::MessageView::Eos(_) => {
                    progress(1.0);
                    return Ok(());
                }
                gst::MessageView::Error(error) => {
                    return Err(anyhow!(
                        "GStreamer conversion failed: {} ({:?})",
                        error.error(),
                        error.debug()
                    ));
                }
                _ => {}
            }
        }
        if let Some(duration) = metadata.format.duration.filter(|seconds| *seconds > 0.0)
            && let Some(position) = pipeline.query_position::<gst::ClockTime>()
        {
            let share = (position.seconds_f64() / duration).clamp(0.0, 1.0);
            if share - last_reported >= 0.01 {
                last_reported = share;
                progress(share);
            }
        }
    }
}

struct PipelineShutdown(gst::Pipeline);

impl Drop for PipelineShutdown {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

use std::str::FromStr;

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;
    use gstreamer_app::AppSink;

    /// A clip of `seconds` with a moving gradient and a tone, in Matroska.
    fn write_test_clip(
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        sample_rate: u32,
        channels: u16,
        seconds: f64,
    ) -> Result<()> {
        write_delayed_test_clip(
            path,
            width,
            height,
            fps,
            sample_rate,
            channels,
            seconds,
            0.0,
        )
    }

    /// The same, with the sound starting `audio_delay` seconds after the picture.
    #[allow(clippy::too_many_arguments)]
    fn write_delayed_test_clip(
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        sample_rate: u32,
        channels: u16,
        seconds: f64,
        audio_delay: f64,
    ) -> Result<()> {
        crate::gstreamer::init()?;
        let frames = (seconds * f64::from(fps)) as u64;
        // The sound in 10 ms buffers.
        let audio_buffers = (seconds * 100.0) as u64;
        let pipeline = gst::parse::launch(&format!(
            "videotestsrc num-buffers={frames} pattern=ball \
               ! video/x-raw,format=I420,width={width},height={height},framerate={fps}/1 \
               ! x264enc speed-preset=ultrafast key-int-max={fps} ! h264parse ! queue ! mux. \
             audiotestsrc num-buffers={audio_buffers} samplesperbuffer={samples} wave=sine \
               freq=440 volume=0.2 timestamp-offset={offset} \
               ! audio/x-raw,format=F32LE,rate={sample_rate},channels={channels} \
               ! audioconvert ! avenc_aac bitrate=96000 ! aacparse ! queue ! mux. \
             matroskamux name=mux ! filesink location=\"{path}\"",
            samples = sample_rate / 100,
            offset = (audio_delay * 1_000_000_000.0) as i64,
            path = path.display(),
        ))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| anyhow!("test clip is not a pipeline"))?;

        run_to_end(&pipeline)
    }

    fn run_to_end(pipeline: &gst::Pipeline) -> Result<()> {
        pipeline.set_state(gst::State::Playing)?;
        let message = pipeline
            .bus()
            .context("pipeline has no bus")?
            .timed_pop_filtered(
                gst::ClockTime::from_seconds(60),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
        pipeline.set_state(gst::State::Null)?;

        let Some(message) = message else {
            bail!("the pipeline did not finish in a minute");
        };

        if let gst::MessageView::Error(error) = message.view() {
            bail!("{}", error.error());
        }

        Ok(())
    }

    /// Every sample pulled from `branch` of `path`, read to its end.
    fn pull_all(path: &Path, branch: &str) -> Vec<gst::Sample> {
        crate::gstreamer::init().unwrap();
        let pipeline = gst::parse::launch(&format!(
            "filesrc location=\"{}\" ! {branch} ! appsink name=sink sync=false",
            path.display()
        ))
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let sink = pipeline
            .by_name("sink")
            .unwrap()
            .downcast::<AppSink>()
            .unwrap();
        pipeline.set_state(gst::State::Playing).unwrap();
        let mut samples = Vec::new();

        while let Ok(sample) = sink.pull_sample() {
            samples.push(sample);
        }

        pipeline.set_state(gst::State::Null).unwrap();

        samples
    }

    /// Seconds of decoded sound, counted in samples.
    fn audio_seconds(path: &Path) -> f64 {
        let mut frames = 0;
        let mut rate = 0;

        for sample in pull_all(
            path,
            "decodebin ! audioconvert ! audio/x-raw,format=F32LE,layout=interleaved",
        ) {
            let info = gstreamer_audio::AudioInfo::from_caps(sample.caps().unwrap()).unwrap();
            rate = info.rate();
            frames += sample.buffer().unwrap().size() / info.bpf() as usize;
        }

        frames as f64 / f64::from(rate)
    }

    /// Encoded video frames in the file.
    fn video_packets(path: &Path) -> usize {
        pull_all(path, "parsebin ! video/x-h264").len()
    }

    #[test]
    fn a_file_in_another_format_comes_out_in_the_target_one() {
        let dir = env::temp_dir().join(format!("noar_convert_{}", process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mkv");
        let target_path = dir.join("converted.mp4");
        // Four seconds of 4:3 at 30 fps with mono 44.1 kHz sound.
        write_test_clip(&source, 320, 240, 30, 44_100, 1, 4.0).unwrap();

        let target = ConvertTarget {
            width: 640,
            height: 360,
            fps: 25,
            sample_rate: 48_000,
        };
        assert!(!probe_shape(&source).unwrap().matches(&target));

        let mut shares = Vec::new();
        convert_to_output(
            &source,
            &target_path,
            target,
            &AtomicBool::new(false),
            |share| {
                shares.push(share);
            },
        )
        .unwrap();

        let shape = probe_shape(&target_path).unwrap();
        assert!(shape.matches(&target), "{shape:?}");
        assert_eq!(shares.last(), Some(&1.0));

        let frames = video_packets(&target_path);
        assert!(
            (98..=102).contains(&frames),
            "{frames} frames for 4 s at 25 fps"
        );
        // 44.1 kHz to 48 kHz: all the sound, not as many samples as came in.
        let seconds = audio_seconds(&target_path);
        assert!((3.9..=4.15).contains(&seconds), "{seconds} s of sound");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_picture_already_in_the_output_format_is_copied() {
        let dir = env::temp_dir().join(format!("noar_convert_copy_{}", process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mkv");
        let target_path = dir.join("converted.mkv");
        // The output's picture; mono 44.1 kHz sound.
        write_test_clip(&source, 320, 240, 25, 44_100, 1, 3.0).unwrap();
        let target = ConvertTarget {
            width: 320,
            height: 240,
            fps: 25,
            sample_rate: 48_000,
        };
        let discovered = discover_media(source.to_str().unwrap()).unwrap();
        let stream = discovered.video_streams().into_iter().next().unwrap();
        assert!(
            video_matches(&source, &target).unwrap(),
            "unexpected native video metadata: {:?}, fps {:?}, interlaced {}, dimensions {}x{}",
            stream.caps(),
            stream.framerate(),
            stream.is_interlaced(),
            stream.width(),
            stream.height()
        );
        assert!(!video_matches(&source, &ConvertTarget { fps: 30, ..target }).unwrap());
        assert!(!probe_shape(&source).unwrap().matches(&target));

        convert_to_output(
            &source,
            &target_path,
            target,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();

        let shape = probe_shape(&target_path).unwrap();
        assert!(shape.matches(&target), "{shape:?}");
        // The same packets: copied, not encoded again.
        assert_eq!(video_packets(&target_path), video_packets(&source));
        let seconds = audio_seconds(&target_path);
        assert!((2.9..=3.15).contains(&seconds), "{seconds} s of sound");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sound_that_starts_late_stays_late() {
        let dir = env::temp_dir().join(format!("noar_convert_delay_{}", process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mkv");
        // Three seconds of picture; the sound half a second later.
        write_delayed_test_clip(&source, 320, 240, 25, 48_000, 2, 3.0, 0.5).unwrap();

        // Encoded, and with the picture copied.
        for (name, width) in [("encoded.mp4", 640), ("copied.mkv", 320)] {
            let target_path = dir.join(name);
            convert_to_output(
                &source,
                &target_path,
                ConvertTarget {
                    width,
                    height: if width == 640 { 480 } else { 240 },
                    fps: 25,
                    sample_rate: 48_000,
                },
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap();

            // Half a second of silence, then the three seconds of sound.
            let seconds = audio_seconds(&target_path);
            assert!(
                (3.4..=3.65).contains(&seconds),
                "{name}: {seconds} s of sound"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stopped_conversion_fails() {
        let dir = env::temp_dir().join(format!("noar_convert_stop_{}", process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("source.mkv");
        write_test_clip(&source, 160, 120, 25, 48_000, 2, 2.0).unwrap();

        let result = convert_to_output(
            &source,
            &dir.join("out.mp4"),
            ConvertTarget {
                width: 320,
                height: 240,
                fps: 25,
                sample_rate: 48_000,
            },
            &AtomicBool::new(true),
            |_| {},
        );
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
