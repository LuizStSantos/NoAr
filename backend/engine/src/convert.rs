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

/// Reads the encoded stream metadata without opening FFmpeg contexts.
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
    use ffmpeg_next::{
        ChannelLayout, Packet, Rational, codec,
        format::{self, Pixel, Sample},
        frame, media,
    };

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
        let mut octx = format::output(path)?;
        let global = octx
            .format()
            .flags()
            .contains(format::flag::Flags::GLOBAL_HEADER);
        let video_codec = codec::encoder::find(codec::Id::H264).context("h264")?;
        let mut video_stream = octx.add_stream(video_codec)?;
        let mut video = codec::context::Context::new_with_codec(video_codec)
            .encoder()
            .video()?;
        video.set_width(width);
        video.set_height(height);
        video.set_format(Pixel::YUV420P);
        video.set_time_base(Rational(1, fps as i32));
        video.set_frame_rate(Some(Rational(fps as i32, 1)));
        if global {
            video.set_flags(codec::flag::Flags::GLOBAL_HEADER);
        }
        let mut video = video.open_as(video_codec)?;
        video_stream.set_parameters(&video);
        // Matroska needs container timing as well as the codec time base.
        video_stream.set_avg_frame_rate(Rational(fps as i32, 1));
        video_stream.set_rate(Rational(fps as i32, 1));
        let audio_codec = codec::encoder::find(codec::Id::AAC).context("aac")?;
        let mut audio_stream = octx.add_stream(audio_codec)?;
        let layout = ChannelLayout::default(i32::from(channels));
        let mut audio = codec::context::Context::new_with_codec(audio_codec)
            .encoder()
            .audio()?;
        audio.set_rate(sample_rate as i32);
        audio.set_channel_layout(layout);
        audio.set_format(Sample::F32(format::sample::Type::Planar));
        audio.set_time_base(Rational(1, sample_rate as i32));
        audio.set_bit_rate(96_000);
        if global {
            audio.set_flags(codec::flag::Flags::GLOBAL_HEADER);
        }
        let mut audio = audio.open_as(audio_codec)?;
        audio_stream.set_parameters(&audio);
        octx.write_header()?;

        let video_out = (
            video_stream_index(&octx, media::Type::Video),
            Rational(1, fps as i32),
        );
        let audio_out = (
            video_stream_index(&octx, media::Type::Audio),
            Rational(1, sample_rate as i32),
        );
        let frames = (seconds * f64::from(fps)) as i64;
        for index in 0..frames {
            let mut picture = frame::Video::new(Pixel::YUV420P, width, height);
            picture.data_mut(0).fill((16 + index % 200) as u8);
            picture.data_mut(1).fill(128);
            picture.data_mut(2).fill(128);
            picture.set_pts(Some(index));
            video.send_frame(&picture)?;
            drain(&mut video, &mut octx, video_out)?;
        }
        video.send_eof()?;
        drain(&mut video, &mut octx, video_out)?;

        let frame_size = audio.frame_size() as usize;
        let total = (seconds * f64::from(sample_rate)) as usize;
        let mut done = 0;
        while done < total {
            let mut tone = frame::Audio::new(audio.format(), frame_size, layout);
            tone.set_rate(sample_rate);
            for channel in 0..usize::from(channels) {
                for (offset, sample) in tone.plane_mut::<f32>(channel).iter_mut().enumerate() {
                    *sample = ((done + offset) as f32 * 0.05).sin() * 0.2;
                }
            }
            tone.set_pts(Some(
                done as i64 + (audio_delay * f64::from(sample_rate)) as i64,
            ));
            audio.send_frame(&tone)?;
            drain_audio(&mut audio, &mut octx, audio_out)?;
            done += frame_size;
        }
        audio.send_eof()?;
        drain_audio(&mut audio, &mut octx, audio_out)?;
        octx.write_trailer()?;

        Ok(())
    }

    fn video_stream_index(octx: &format::context::Output, kind: media::Type) -> usize {
        octx.streams()
            .find(|stream| stream.parameters().medium() == kind)
            .map(|stream| stream.index())
            .unwrap()
    }

    fn drain(
        encoder: &mut codec::encoder::video::Encoder,
        octx: &mut format::context::Output,
        (index, time_base): (usize, Rational),
    ) -> Result<()> {
        let mut packet = Packet::empty();
        while encoder.receive_packet(&mut packet).is_ok() {
            let stream_time_base = octx.stream(index).unwrap().time_base();
            packet.set_stream(index);
            packet.rescale_ts(time_base, stream_time_base);
            packet.write_interleaved(octx)?;
        }
        Ok(())
    }

    fn drain_audio(
        encoder: &mut codec::encoder::audio::Encoder,
        octx: &mut format::context::Output,
        (index, time_base): (usize, Rational),
    ) -> Result<()> {
        let mut packet = Packet::empty();
        while encoder.receive_packet(&mut packet).is_ok() {
            let stream_time_base = octx.stream(index).unwrap().time_base();
            packet.set_stream(index);
            packet.rescale_ts(time_base, stream_time_base);
            packet.write_interleaved(octx)?;
        }
        Ok(())
    }

    fn audio_seconds(path: &Path) -> f64 {
        let mut ictx = format::input(path).unwrap();
        let stream = ictx.streams().best(media::Type::Audio).unwrap();
        let index = stream.index();
        let mut decoder = codec::context::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .audio()
            .unwrap();
        let mut samples = 0;
        let mut decoded = frame::Audio::empty();
        let mut rate = 0;
        for (stream, packet) in ictx.packets() {
            if stream.index() == index && decoder.send_packet(&packet).is_ok() {
                while decoder.receive_frame(&mut decoded).is_ok() {
                    samples += decoded.samples();
                    rate = decoded.rate();
                }
            }
        }
        samples as f64 / f64::from(rate)
    }

    fn video_packets(path: &Path) -> usize {
        format::input(path)
            .unwrap()
            .packets()
            .filter(|(stream, _)| stream.parameters().medium() == media::Type::Video)
            .count()
    }

    #[test]
    fn a_file_in_another_format_comes_out_in_the_target_one() {
        ffmpeg_next::init().ok();
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

        let frames = format::input(&target_path)
            .unwrap()
            .packets()
            .filter(|(stream, _)| stream.parameters().medium() == media::Type::Video)
            .count();
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
        ffmpeg_next::init().ok();
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
        ffmpeg_next::init().ok();
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
        ffmpeg_next::init().ok();
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
