//! A lighter copy of the Player preview, for viewers on a slow connection.
//!
//! The preview the engine sends is the output itself (the same picture and
//! bit rate as the air). This re-encodes it to a small picture at a low bit
//! rate, and only while someone watches it: it reads the preview copy, never
//! the playout, so a failure here cannot reach the air.

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use ffmpeg_next::{
    Dictionary, Packet, Rational, codec,
    format::{self, Pixel, context::StreamIo},
    frame, media,
    software::scaling,
};

/// The size and rate of the light preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightPreview {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
}

impl Default for LightPreview {
    /// 640×360 at 15 frames per second and 500 kb/s: about a fifth of a
    /// typical 720p output.
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            fps: 15,
            video_kbps: 500,
        }
    }
}

/// Reads MPEG-TS bytes from a channel; ends when it closes or `stop` is set.
struct Feed {
    receiver: Receiver<Vec<u8>>,
    pending: Vec<u8>,
    offset: usize,
    stop: Arc<AtomicBool>,
}

impl io::Read for Feed {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        while self.offset >= self.pending.len() {
            if self.stop.load(Ordering::Acquire) {
                return Ok(0);
            }

            match self.receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(bytes) => {
                    self.pending = bytes;
                    self.offset = 0;
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }

        let size = buffer.len().min(self.pending.len() - self.offset);
        buffer[..size].copy_from_slice(&self.pending[self.offset..self.offset + size]);
        self.offset += size;

        Ok(size)
    }
}

/// Hands the light preview on; `deliver` returns false once nobody watches.
struct Delivery<F> {
    deliver: F,
    stop: Arc<AtomicBool>,
}

impl<F: FnMut(Vec<u8>) -> bool> io::Write for Delivery<F> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if !(self.deliver)(data.to_vec()) {
            self.stop.store(true, Ordering::Release);
        }

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Re-encodes the preview copy that arrives on `input` and passes the light
/// MPEG-TS to `deliver`, until the input ends, `stop` is set or `deliver`
/// returns false. Blocks: run it on a thread of its own.
pub fn transcode_light_preview<F>(
    input: Receiver<Vec<u8>>,
    deliver: F,
    settings: LightPreview,
    stop: Arc<AtomicBool>,
) -> Result<()>
where
    F: FnMut(Vec<u8>) -> bool + Send + 'static,
{
    // Joining mid-stream, FFmpeg's stream analysis decodes pictures whose
    // references it never got; those messages say nothing about the air.
    crate::utils::logging::quiet_ffmpeg_logs(|| transcode(input, deliver, settings, stop))
}

fn transcode<F>(
    input: Receiver<Vec<u8>>,
    deliver: F,
    settings: LightPreview,
    stop: Arc<AtomicBool>,
) -> Result<()>
where
    F: FnMut(Vec<u8>) -> bool + Send + 'static,
{
    let feed = Feed {
        receiver: input,
        pending: Vec::new(),
        offset: 0,
        stop: stop.clone(),
    };
    let mut probe = Dictionary::new();
    // A viewer joins mid-stream: the codec settings only come with the next
    // key frame, a GOP away, and a 1080p GOP can pass 2 MB. The analysis
    // ends as soon as it has them; these are only its limits.
    probe.set("analyzeduration", "15000000");
    probe.set("probesize", "33554432");
    let mut ictx = format::input_from_stream(
        StreamIo::from_read_with_capacity(feed, 64 * 1024)?,
        Some("preview.ts"),
        Some(probe),
    )
    .context("reading the preview")?;

    let video_in = ictx
        .streams()
        .best(media::Type::Video)
        .context("the preview has no video")?;
    let video_index = video_in.index();
    let video_in_time_base = video_in.time_base();
    let audio_in = ictx
        .streams()
        .best(media::Type::Audio)
        .map(|stream| (stream.index(), stream.time_base(), stream.parameters()));
    let mut decoder = codec::context::Context::from_parameters(video_in.parameters())?
        .decoder()
        .video()?;

    let encoder_codec = codec::encoder::find_by_name("libx264")
        .or_else(|| codec::encoder::find(codec::Id::H264))
        .context("no H.264 encoder for the light preview")?;
    let mut encoder_ctx = codec::context::Context::new_with_codec(encoder_codec)
        .encoder()
        .video()?;
    encoder_ctx.set_width(settings.width);
    encoder_ctx.set_height(settings.height);
    encoder_ctx.set_format(Pixel::YUV420P);
    encoder_ctx.set_time_base(video_in_time_base);
    encoder_ctx.set_frame_rate(Some(Rational(settings.fps as i32, 1)));
    encoder_ctx.set_bit_rate(settings.video_kbps as usize * 1000);
    encoder_ctx.set_max_bit_rate(settings.video_kbps as usize * 1000);
    // A key frame every second, so a new viewer starts fast.
    encoder_ctx.set_gop(settings.fps.max(1));
    let mut options = Dictionary::new();
    if encoder_codec.name() == "libx264" {
        options.set("preset", "veryfast");
        options.set("tune", "zerolatency");
    }
    let mut encoder = encoder_ctx.open_as_with(encoder_codec, options)?;

    let delivery = Delivery {
        deliver,
        stop: stop.clone(),
    };
    let mut octx = format::output_to_stream(
        StreamIo::from_write_with_capacity(delivery, 7 * 188)?,
        Some("light.ts"),
        Some("mpegts"),
    )?;
    let mut video_out = octx.add_stream_with(&encoder)?;
    video_out.set_time_base(video_in_time_base);
    let video_out_index = video_out.index();
    let audio_out = match &audio_in {
        Some((_, _, parameters)) => {
            let mut stream = octx.add_stream(codec::encoder::find(codec::Id::None))?;
            stream.set_parameters(parameters.clone());
            unsafe { (*(*stream.as_mut_ptr()).codecpar).codec_tag = 0 };
            Some(stream.index())
        }
        None => None,
    };
    octx.write_header()?;
    let video_out_time_base = octx
        .stream(video_out_index)
        .context("light preview video stream")?
        .time_base();

    let mut scaler: Option<(scaling::Context, (Pixel, u32, u32))> = None;
    let frame_step = i64::from(video_in_time_base.1)
        / i64::from(settings.fps.max(1))
        / i64::from(video_in_time_base.0.max(1));
    let mut last_kept: Option<i64> = None;
    // The copy is joined mid-stream: wait for a key frame rather than
    // decode pictures whose references never came (and log about each).
    let mut key_frame_seen = false;
    let mut last_dts = [i64::MIN; 2];
    let mut decoded = frame::Video::empty();
    let mut scaled = frame::Video::empty();

    let mut write = |octx: &mut format::context::Output, packet: Packet, slot: usize| {
        // A restarted playout starts its timestamps over: drop what would
        // go backwards rather than fail the light preview.
        let dts = packet.dts().unwrap_or(i64::MIN);
        if dts != i64::MIN && dts <= last_dts[slot] {
            return Ok(());
        }
        last_dts[slot] = dts;
        packet.write_interleaved(octx)
    };

    for (stream, packet) in ictx.packets() {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let index = stream.index();

        if let Some((audio_index, audio_time_base, _)) = audio_in
            && index == audio_index
            && let Some(audio_out_index) = audio_out
        {
            let out_time_base = octx
                .stream(audio_out_index)
                .context("light preview audio stream")?
                .time_base();
            let mut packet = packet.clone();
            packet.set_stream(audio_out_index);
            packet.rescale_ts(audio_time_base, out_time_base);
            packet.set_position(-1);
            write(&mut octx, packet, 1)?;
            continue;
        }

        if index != video_index {
            continue;
        }

        key_frame_seen |= packet.is_key();

        if !key_frame_seen || decoder.send_packet(&packet).is_err() {
            continue;
        }

        while decoder.receive_frame(&mut decoded).is_ok() {
            let Some(pts) = decoded.timestamp().or(decoded.pts()) else {
                continue;
            };
            // Fewer frames: keep one per 1/fps.
            if let Some(last) = last_kept
                && pts > last
                && pts - last < frame_step
            {
                continue;
            }
            last_kept = Some(pts);

            let source = (decoded.format(), decoded.width(), decoded.height());
            if scaler.as_ref().is_none_or(|(_, size)| *size != source) {
                scaler = Some((
                    scaling::Context::get(
                        source.0,
                        source.1,
                        source.2,
                        Pixel::YUV420P,
                        settings.width,
                        settings.height,
                        scaling::flag::Flags::BILINEAR,
                    )?,
                    source,
                ));
            }
            let (context, _) = scaler.as_mut().ok_or_else(|| anyhow!("scaler"))?;
            context.run(&decoded, &mut scaled)?;
            scaled.set_pts(Some(pts));
            encoder.send_frame(&scaled)?;

            let mut encoded = Packet::empty();
            while encoder.receive_packet(&mut encoded).is_ok() {
                let mut packet = encoded.clone();
                packet.set_stream(video_out_index);
                packet.rescale_ts(video_in_time_base, video_out_time_base);
                write(&mut octx, packet, 0)?;
            }
        }
    }

    let _ = octx.write_trailer();

    Ok(())
}
