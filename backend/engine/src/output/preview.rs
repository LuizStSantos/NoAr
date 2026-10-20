use anyhow::{Context, Result, anyhow};
use ffmpeg::{codec, format, util::rational::Rational};
use ffmpeg_next as ffmpeg;

use super::timestamps::DtsGuard;
use crate::utils::ffmpeg::reference_packet;

/// Sends a live MPEG-TS copy of the encoded output to a local address, so the
/// application can show the channel with about a second of delay, whatever
/// the real output is (RTMP, SRT, HLS). The packets are the ones the output
/// encoders already produced: nothing is encoded twice.
pub(super) struct PreviewMuxer {
    octx: format::context::Output,
    dts_guard: DtsGuard,
    video_stream_index: usize,
    audio_stream_index: usize,
}

impl PreviewMuxer {
    pub(super) fn open(
        url: &str,
        video_encoder: &codec::encoder::video::Encoder,
        audio_encoder: &codec::encoder::audio::Encoder,
        channel_id: i32,
    ) -> Result<Self> {
        if url.trim().is_empty() {
            return Err(anyhow!("preview address must not be empty"));
        }

        let mut octx = format::output_as(url, "mpegts")
            .with_context(|| format!("failed to open preview output {url}"))?;

        let mut video_stream = octx.add_stream_with(video_encoder)?;
        video_stream.set_time_base(video_encoder.time_base());
        let video_stream_index = video_stream.index();
        let mut audio_stream = octx.add_stream_with(audio_encoder)?;
        audio_stream.set_time_base(audio_encoder.time_base());
        let audio_stream_index = audio_stream.index();

        // Repeat the program tables often, so a viewer that joins mid-stream
        // finds them right away and only waits for the next key frame.
        let mut options = ffmpeg::Dictionary::new();
        options.set("pat_period", "0.2");
        options.set("sdt_period", "1");
        let unused = octx
            .write_header_with(options)?
            .iter()
            .map(|(key, _)| key.to_string())
            .collect::<Vec<_>>();

        if !unused.is_empty() {
            return Err(anyhow!(
                "unused preview muxer option(s): {}",
                unused.join(", ")
            ));
        }

        Ok(Self {
            dts_guard: DtsGuard::new(&octx, "Player preview", channel_id),
            octx,
            video_stream_index,
            audio_stream_index,
        })
    }

    pub(super) fn write_video(
        &mut self,
        packet: &ffmpeg::Packet,
        time_base: Rational,
    ) -> Result<()> {
        self.write_packet(packet, self.video_stream_index, time_base)
    }

    pub(super) fn write_audio(
        &mut self,
        packet: &ffmpeg::Packet,
        time_base: Rational,
    ) -> Result<()> {
        self.write_packet(packet, self.audio_stream_index, time_base)
    }

    fn write_packet(
        &mut self,
        packet: &ffmpeg::Packet,
        stream_index: usize,
        encoder_time_base: Rational,
    ) -> Result<()> {
        let stream_time_base = self
            .octx
            .stream(stream_index)
            .context("preview output stream is missing")?
            .time_base();
        // Share the encoded payload; only the header fields change.
        let mut packet = reference_packet(packet)?;
        packet.set_stream(stream_index);
        packet.rescale_ts(encoder_time_base, stream_time_base);
        self.dts_guard.fix(&mut packet, stream_index);
        // Written as they come instead of interleaved: the encoders already
        // deliver audio and video in step, and waiting would add delay.
        packet.write(&mut self.octx)?;

        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<()> {
        self.octx.write_trailer()?;

        Ok(())
    }
}
