//! Converts a video file to the channel's output format, so the playout gets
//! a picture of the size and frame rate it sends, and sound at its sample
//! rate, and does no scaling or frame rate conversion on air. A file that
//! does not decode cleanly is rewritten from what decodes, which also takes
//! damaged spots out of it.
//!
//! The picture is fitted the way the playout fits it: the aspect ratio kept,
//! with black bars. The video is H.264 at a high quality (it is the master
//! the channel re-encodes), the sound AAC stereo.

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};
use ffmpeg_next::{
    ChannelLayout, Dictionary, Packet, Rational, codec,
    format::{self, Pixel, Sample},
    frame, media,
    software::{resampling, scaling},
};

use crate::{
    OutputConfig,
    playout::{FrameRateConverter, VideoScale, black_video_frame, copy_video_frame},
};

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

/// Reads what a file is.
pub fn probe_shape(path: &Path) -> Result<MediaShape> {
    let ictx = format::input(path).with_context(|| format!("cannot open {}", path.display()))?;
    let video = ictx.streams().best(media::Type::Video);
    let audio = ictx.streams().best(media::Type::Audio);
    let (video_codec, width, height, fps) = match &video {
        Some(stream) => {
            let decoder = codec::context::Context::from_parameters(stream.parameters())?
                .decoder()
                .video()?;
            // The nominal rate: the average drifts on short files.
            let rate = Some(stream.rate())
                .filter(|rate| rate.1 != 0 && rate.0 > 0)
                .unwrap_or_else(|| stream.avg_frame_rate());
            let fps = (rate.1 != 0 && rate.0 > 0).then(|| f64::from(rate.0) / f64::from(rate.1));

            (
                Some(decoder.id().name().to_string()),
                decoder.width(),
                decoder.height(),
                fps,
            )
        }
        None => (None, 0, 0, None),
    };
    let (audio_codec, sample_rate, audio_channels) = match &audio {
        Some(stream) => {
            let decoder = codec::context::Context::from_parameters(stream.parameters())?
                .decoder()
                .audio()?;

            (
                Some(decoder.id().name().to_string()),
                Some(decoder.rate()),
                Some(decoder.channels()),
            )
        }
        None => (None, None, None),
    };

    Ok(MediaShape {
        video_codec,
        width,
        height,
        fps,
        audio_codec,
        sample_rate,
        audio_channels,
    })
}

/// Converts `input` to `output` (whose extension picks the container), in
/// the target format. `progress` gets the share done, from 0 to 1; `cancel`
/// stops it. Blocks: run it on a thread of its own.
pub fn convert_to_output(
    input: &Path,
    output: &Path,
    target: ConvertTarget,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<()> {
    if target.width < 2 || target.height < 2 || target.fps == 0 || target.sample_rate == 0 {
        bail!("the output size, frame rate and sample rate must be set");
    }

    let mut ictx =
        format::input(input).with_context(|| format!("cannot open {}", input.display()))?;
    let duration = (ictx.duration() > 0).then(|| ictx.duration() as f64 / 1_000_000.0);
    let video_in = ictx
        .streams()
        .best(media::Type::Video)
        .context("the file has no video")?;
    let video_index = video_in.index();
    let video_time_base = video_in.time_base();
    let audio_in = ictx
        .streams()
        .best(media::Type::Audio)
        .map(|stream| (stream.index(), stream.parameters()));

    // Half the cores at most: the playout runs on the same machine.
    let threads = std::thread::available_parallelism()
        .map_or(1, |cores| cores.get() / 2)
        .max(1);
    let mut video_decoder = {
        let mut context = codec::context::Context::from_parameters(video_in.parameters())?;
        context.set_threading(codec::threading::Config {
            kind: codec::threading::Type::Frame,
            count: threads,
        });
        context.decoder().video()?
    };
    let mut audio_decoder = match &audio_in {
        Some((_, parameters)) => Some(
            codec::context::Context::from_parameters(parameters.clone())?
                .decoder()
                .audio()?,
        ),
        None => None,
    };

    let mut octx =
        format::output(output).with_context(|| format!("cannot write {}", output.display()))?;
    let global_header = octx
        .format()
        .flags()
        .contains(format::flag::Flags::GLOBAL_HEADER);

    // Video: H.264, the output's size and frame rate.
    let video_codec = codec::encoder::find_by_name("libx264")
        .or_else(|| codec::encoder::find(codec::Id::H264))
        .context("no H.264 encoder")?;
    let frame_time_base = Rational(1, target.fps as i32);
    let mut video_stream = octx.add_stream(video_codec)?;
    let mut video_context = codec::context::Context::new_with_codec(video_codec)
        .encoder()
        .video()?;
    video_context.set_width(target.width);
    video_context.set_height(target.height);
    video_context.set_format(Pixel::YUV420P);
    video_context.set_time_base(frame_time_base);
    video_context.set_frame_rate(Some(Rational(target.fps as i32, 1)));
    video_context.set_gop(target.fps * 2);
    if global_header {
        video_context.set_flags(codec::flag::Flags::GLOBAL_HEADER);
    }
    let mut video_options = Dictionary::new();
    if video_codec.name() == "libx264" {
        // A master to re-encode from: high quality, at a moderate CPU cost.
        video_options.set("preset", "faster");
        video_options.set("crf", "18");
        video_options.set("threads", &threads.to_string());
    }
    let mut video_encoder = video_context.open_as_with(video_codec, video_options)?;
    video_stream.set_parameters(&video_encoder);
    let video_out = video_stream.index();

    // Sound: AAC stereo at the output's sample rate.
    let mut audio = match audio_decoder.as_ref() {
        Some(decoder) => {
            let codec = codec::encoder::find(codec::Id::AAC).context("no AAC encoder")?;
            let mut stream = octx.add_stream(codec)?;
            let mut context = codec::context::Context::new_with_codec(codec)
                .encoder()
                .audio()?;
            context.set_rate(target.sample_rate as i32);
            context.set_channel_layout(ChannelLayout::STEREO);
            context.set_format(Sample::F32(format::sample::Type::Planar));
            context.set_time_base(Rational(1, target.sample_rate as i32));
            context.set_bit_rate(192_000);
            if global_header {
                context.set_flags(codec::flag::Flags::GLOBAL_HEADER);
            }
            let encoder = context.open_as(codec)?;
            stream.set_parameters(&encoder);
            let layout = if decoder.channel_layout().is_empty() {
                ChannelLayout::default(i32::from(decoder.channels()))
            } else {
                decoder.channel_layout()
            };
            let resampler = resampling::Context::get(
                decoder.format(),
                layout,
                decoder.rate(),
                Sample::F32(format::sample::Type::Planar),
                ChannelLayout::STEREO,
                target.sample_rate,
            )?;

            Some(AudioPath {
                stream_index: stream.index(),
                frame_size: (encoder.frame_size() as usize).max(1),
                encoder,
                resampler,
                pending: [Vec::new(), Vec::new()],
                next_pts: 0,
            })
        }
        None => None,
    };

    let mut options = Dictionary::new();
    if matches!(
        output.extension().and_then(|extension| extension.to_str()),
        Some("mp4" | "m4v" | "mov")
    ) {
        // The index at the start: the file opens and seeks fast.
        options.set("movflags", "+faststart");
    }
    octx.write_header_with(options)?;

    let scale = VideoScale::new(
        video_decoder.width(),
        video_decoder.height(),
        &OutputConfig::new(target.width, target.height, target.fps, target.sample_rate),
    );
    let mut scaler: Option<(scaling::Context, (Pixel, u32, u32))> = None;
    let mut rate = FrameRateConverter::new(video_time_base, target.fps, true);
    let mut next_frame = 0_i64;
    let mut decoded = frame::Video::empty();
    let mut last_reported = 0.0;

    let mut sink = Sink {
        octx: &mut octx,
        video_out,
        frame_time_base,
    };

    let mut handle_video = |decoder: &mut codec::decoder::Video,
                            sink: &mut Sink<'_>,
                            encoder: &mut codec::encoder::video::Encoder,
                            next_frame: &mut i64|
     -> Result<Option<f64>> {
        let mut reached = None;

        while decoder.receive_frame(&mut decoded).is_ok() {
            let timestamp = decoded.timestamp().or(decoded.pts());
            let copies = rate.output_frames(timestamp);

            if let Some(timestamp) = timestamp {
                reached = Some(timestamp as f64 * f64::from(video_time_base));
            }

            if copies == 0 {
                continue;
            }

            let source = (decoded.format(), decoded.width(), decoded.height());
            if scaler.as_ref().is_none_or(|(_, size)| *size != source) {
                scaler = Some((
                    scaling::Context::get(
                        source.0,
                        source.1,
                        source.2,
                        Pixel::YUV420P,
                        scale.scaled_width,
                        scale.scaled_height,
                        scaling::flag::Flags::BICUBIC,
                    )?,
                    source,
                ));
            }
            let (context, _) = scaler.as_mut().ok_or_else(|| anyhow!("scaler"))?;
            let mut scaled = frame::Video::empty();
            context.run(&decoded, &mut scaled)?;
            let mut picture = if scale.needs_padding() {
                let mut padded = black_video_frame(target.width, target.height);
                copy_video_frame(
                    &scaled,
                    &mut padded,
                    scale.x_offset,
                    scale.y_offset,
                    scale.scaled_width,
                    scale.scaled_height,
                );
                padded
            } else {
                scaled
            };

            for _ in 0..copies {
                picture.set_pts(Some(*next_frame));
                *next_frame += 1;
                encoder.send_frame(&picture)?;
                sink.write_video(encoder)?;
            }
        }

        Ok(reached)
    };

    for (stream, packet) in ictx.packets() {
        if cancel.load(Ordering::Acquire) {
            bail!("conversion stopped");
        }

        let index = stream.index();

        if index == video_index {
            // Damaged data is skipped, as the playout does.
            if video_decoder.send_packet(&packet).is_err() {
                continue;
            }

            if let Some(seconds) = handle_video(
                &mut video_decoder,
                &mut sink,
                &mut video_encoder,
                &mut next_frame,
            )? && let Some(duration) = duration
            {
                let share = (seconds / duration).clamp(0.0, 1.0);

                if share - last_reported >= 0.01 {
                    last_reported = share;
                    progress(share);
                }
            }
        } else if let (Some(decoder), Some(path), Some((audio_index, _))) =
            (audio_decoder.as_mut(), audio.as_mut(), audio_in.as_ref())
            && index == *audio_index
            && decoder.send_packet(&packet).is_ok()
        {
            path.drain_decoder(decoder, &mut sink)?;
        }
    }

    video_decoder.send_eof()?;
    handle_video(
        &mut video_decoder,
        &mut sink,
        &mut video_encoder,
        &mut next_frame,
    )?;
    video_encoder.send_eof()?;
    sink.write_video(&mut video_encoder)?;

    if let (Some(decoder), Some(path)) = (audio_decoder.as_mut(), audio.as_mut()) {
        decoder.send_eof()?;
        path.drain_decoder(decoder, &mut sink)?;
        path.finish(&mut sink)?;
    }

    octx.write_trailer()?;
    progress(1.0);

    Ok(())
}

struct Sink<'a> {
    octx: &'a mut format::context::Output,
    video_out: usize,
    frame_time_base: Rational,
}

impl Sink<'_> {
    fn write_video(&mut self, encoder: &mut codec::encoder::video::Encoder) -> Result<()> {
        let mut packet = Packet::empty();

        while encoder.receive_packet(&mut packet).is_ok() {
            self.write(&mut packet, self.video_out, self.frame_time_base)?;
        }

        Ok(())
    }

    fn write(&mut self, packet: &mut Packet, stream: usize, time_base: Rational) -> Result<()> {
        let stream_time_base = self
            .octx
            .stream(stream)
            .context("output stream is missing")?
            .time_base();
        packet.set_stream(stream);
        packet.rescale_ts(time_base, stream_time_base);
        packet.set_position(-1);
        packet.write_interleaved(self.octx)?;

        Ok(())
    }
}

struct AudioPath {
    stream_index: usize,
    frame_size: usize,
    encoder: codec::encoder::audio::Encoder,
    resampler: resampling::Context,
    /// Resampled samples not encoded yet, per channel.
    pending: [Vec<f32>; 2],
    next_pts: i64,
}

impl AudioPath {
    fn drain_decoder(
        &mut self,
        decoder: &mut codec::decoder::Audio,
        sink: &mut Sink<'_>,
    ) -> Result<()> {
        let mut decoded = frame::Audio::empty();

        while decoder.receive_frame(&mut decoded).is_ok() {
            if decoded.channel_layout().is_empty() {
                decoded.set_channel_layout(ChannelLayout::default(i32::from(decoded.channels())));
            }

            let mut converted = frame::Audio::empty();
            // A stream that changes its format midway is left as is.
            if self.resampler.run(&decoded, &mut converted).is_err() {
                continue;
            }
            self.take(&converted);
            self.encode_ready(sink, false)?;
        }

        Ok(())
    }

    fn take(&mut self, converted: &frame::Audio) {
        let samples = converted.samples();

        if samples == 0 {
            return;
        }

        for (channel, pending) in self.pending.iter_mut().enumerate() {
            pending.extend_from_slice(&converted.plane::<f32>(channel)[..samples]);
        }
    }

    fn encode_ready(&mut self, sink: &mut Sink<'_>, last: bool) -> Result<()> {
        while self.pending[0].len() >= self.frame_size || (last && !self.pending[0].is_empty()) {
            let samples = self.pending[0].len().min(self.frame_size);
            let mut frame = frame::Audio::new(
                Sample::F32(format::sample::Type::Planar),
                self.frame_size,
                ChannelLayout::STEREO,
            );
            frame.set_rate(self.encoder.rate());

            for (channel, pending) in self.pending.iter_mut().enumerate() {
                let plane = frame.plane_mut::<f32>(channel);
                plane[..samples].copy_from_slice(&pending[..samples]);
                // The last frame is padded with silence.
                plane[samples..].fill(0.0);
                pending.drain(..samples);
            }

            frame.set_pts(Some(self.next_pts));
            self.next_pts += self.frame_size as i64;
            self.encoder.send_frame(&frame)?;
            self.write(sink)?;
        }

        Ok(())
    }

    fn finish(&mut self, sink: &mut Sink<'_>) -> Result<()> {
        let mut converted = frame::Audio::empty();

        if self.resampler.flush(&mut converted).is_ok() {
            self.take(&converted);
        }
        self.encode_ready(sink, true)?;
        self.encoder.send_eof()?;
        self.write(sink)
    }

    fn write(&mut self, sink: &mut Sink<'_>) -> Result<()> {
        let mut packet = Packet::empty();
        let time_base = Rational(1, self.encoder.rate() as i32);

        while self.encoder.receive_packet(&mut packet).is_ok() {
            sink.write(&mut packet, self.stream_index, time_base)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{env, process};

    use super::*;

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
            tone.set_pts(Some(done as i64));
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
