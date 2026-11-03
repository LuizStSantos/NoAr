use crate::frame::{AudioFrame, VideoFrame};
use std::{error::Error, fmt};

use anyhow::Result;
#[cfg(feature = "desktop-base")]
use anyhow::anyhow;

mod backlog;
#[cfg(feature = "desktop-base")]
pub(crate) mod desktop;
#[path = "gstreamer_output.rs"]
mod encoded;
mod gst_hls;
mod gst_metadata;
mod hls;
pub(crate) mod link;
pub(crate) mod ndi;
pub(crate) mod pipeline;
mod recording_native;
mod recording_retention;
pub(crate) mod scte35;
pub(crate) mod video_card;
mod vtt;

#[cfg(feature = "desktop-base")]
use crate::benchmark::BenchHandle;
use crate::{
    HlsHealth,
    compositor::logo::{LogoOverlay, blend_logo},
    utils::config::{HlsSubtitle, HlsVariant, OutputConfig},
};

#[cfg(feature = "desktop-base")]
use desktop::{DesktopFrameSender, DesktopOutput};
use encoded::{EncodedFormat, EncodedOutput};
pub use hls::resolved_variant_playlist_path;

#[derive(Debug)]
pub(crate) struct PlaybackStopped;

impl fmt::Display for PlaybackStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("desktop playback stopped by user")
    }
}

impl Error for PlaybackStopped {}

pub(crate) trait FrameOutput {
    fn audio_frame_size(&self) -> usize;

    fn encode_video(&mut self, frame: &VideoFrame) -> Result<()>;

    /// The newest picture on its way out, to hold on screen while a source
    /// connects. `None` holds black.
    fn held_video_frame(&self) -> Option<VideoFrame> {
        None
    }

    fn encode_audio(&mut self, frame: &AudioFrame) -> Result<()>;

    /// Volume has already been applied; meters and encoding still run normally.
    fn encode_processed_audio(&mut self, frame: &AudioFrame) -> Result<()> {
        self.encode_audio(frame)
    }

    fn encode_live_audio(&mut self, frame: &AudioFrame) -> Result<()> {
        self.encode_audio(frame)
    }

    fn encode_audio_with_gain_hold(&mut self, frame: &AudioFrame, _hold: bool) -> Result<()> {
        self.encode_audio(frame)
    }

    /// Discard output buffered past a manual clip skip and re-anchor at the
    /// supplied synchronized timeline position. Encoded outputs cannot
    /// retract muxed packets and therefore use the default padding path.
    fn reset_after_skip(&mut self, _video_pts: i64, _audio_pts: i64) -> Result<bool> {
        Ok(false)
    }

    fn apply_logo_overlay(
        &mut self,
        frame: &mut VideoFrame,
        logo: &LogoOverlay,
        opacity_factor: f64,
    ) {
        blend_logo(frame, logo, opacity_factor);
    }

    fn benchmarks_logo_overlay(&self) -> bool {
        true
    }

    fn set_video_end(&mut self, _video_end_pts: Option<i64>) -> Result<()> {
        Ok(())
    }

    /// Signals that all decoded video frames have been queued. Realtime
    /// outputs can start short clips without ending their timeline early.
    fn video_decoded(&mut self) -> Result<()> {
        Ok(())
    }

    fn video_finished(&mut self) -> Result<()> {
        Ok(())
    }

    /// Lets realtime outputs account for silence that already elapsed while
    /// decode was blocked behind video backpressure.
    fn pad_audio(&mut self, _samples: i64) -> Result<bool> {
        Ok(false)
    }

    fn write_vtt_subtitles(
        &mut self,
        _media_path: &str,
        _output_start_ms: i64,
        _source_start_ms: i64,
    ) -> Result<()> {
        Ok(())
    }

    /// Discard subtitles queued for the previous clip. Encoded outputs keep
    /// cues pending until file playback reaches their timestamp.
    fn clear_vtt_subtitles(&mut self) -> Result<()> {
        Ok(())
    }

    /// Release subtitle cues whose output timestamp has been reached by an
    /// actually emitted file video frame.
    fn advance_vtt_subtitles(&mut self, _output_position_ms: i64) -> Result<()> {
        Ok(())
    }
}

pub(crate) struct Output {
    kind: OutputKind,
}

enum OutputKind {
    Encoded(Box<EncodedOutput>),
    #[cfg(feature = "desktop-base")]
    Desktop(Box<DesktopOutput>),
}

impl Output {
    pub(crate) fn open(path: &str, cfg: &OutputConfig) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Encoded(Box::new(EncodedOutput::open(
                path,
                cfg,
                EncodedFormat::Auto,
            )?)),
        })
    }

    pub(crate) fn open_stream(path: &str, cfg: &OutputConfig) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Encoded(Box::new(EncodedOutput::open(
                path,
                cfg,
                EncodedFormat::Stream {
                    muxer: cfg.stream_type.muxer(&cfg.stream_format).to_string(),
                },
            )?)),
        })
    }

    pub(crate) fn open_hls(
        path: &str,
        cfg: &OutputConfig,
        hls_variants: &[HlsVariant],
        hls_subtitle: Option<HlsSubtitle>,
        hls_segment_seconds: u32,
        hls_list_size: u32,
        hls_health: HlsHealth,
    ) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Encoded(Box::new(EncodedOutput::open_with_hls_health(
                path,
                cfg,
                EncodedFormat::Hls {
                    variants: hls_variants.to_vec(),
                    subtitle: hls_subtitle,
                    segment_seconds: hls_segment_seconds,
                    list_size: hls_list_size,
                },
                Some(hls_health),
            )?)),
        })
    }

    /// Publishes the program as the NDI source `name`, uncompressed.
    pub(crate) fn open_ndi(name: &str, cfg: &OutputConfig) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Encoded(Box::new(EncodedOutput::open(
                name,
                cfg,
                EncodedFormat::Ndi {
                    name: name.to_owned(),
                },
            )?)),
        })
    }

    /// Plays the program on a DeckLink card (`decklink://N?mode=…`).
    pub(crate) fn open_decklink(address: &str, cfg: &OutputConfig) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Encoded(Box::new(EncodedOutput::open(
                address,
                cfg,
                EncodedFormat::DeckLink {
                    address: address.to_owned(),
                },
            )?)),
        })
    }

    #[cfg(feature = "desktop-base")]
    pub(crate) fn open_desktop(cfg: &OutputConfig) -> Result<Self> {
        Ok(Self {
            kind: OutputKind::Desktop(Box::new(DesktopOutput::open(cfg)?)),
        })
    }

    pub(crate) fn audio_frame_size(&self) -> usize {
        match &self.kind {
            OutputKind::Encoded(output) => output.audio_frame_size(),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(output) => output.audio_frame_size(),
        }
    }

    pub(crate) fn encode_video(&mut self, frame: &VideoFrame) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => output.encode_video(frame),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(output) => output.encode_video(frame),
        }
    }

    pub(crate) fn encode_audio(&mut self, frame: &AudioFrame) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => output.encode_audio(frame),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(output) => output.encode_audio(frame),
        }
    }

    pub(crate) fn set_playout_rate(&mut self, rate: f64) {
        match &mut self.kind {
            OutputKind::Encoded(output) => output.set_playout_rate(rate),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => {}
        }
    }

    pub(crate) fn finish(self) -> Result<()> {
        match self.kind {
            OutputKind::Encoded(output) => output.finish(),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(output) => output.finish(),
        }
    }

    #[cfg(feature = "desktop-base")]
    pub(crate) fn is_desktop(&self) -> bool {
        matches!(self.kind, OutputKind::Desktop(_))
    }

    #[cfg(feature = "desktop-base")]
    pub(crate) fn run_desktop<T, F>(&mut self, benchmark: BenchHandle, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut DesktopFrameSender) -> T + Send + 'static,
    {
        match &mut self.kind {
            OutputKind::Desktop(output) => output.run_operation(benchmark, operation),
            OutputKind::Encoded(_) => Err(anyhow!("output is not in desktop mode")),
        }
    }
}

impl FrameOutput for Output {
    fn audio_frame_size(&self) -> usize {
        Self::audio_frame_size(self)
    }

    fn held_video_frame(&self) -> Option<VideoFrame> {
        match &self.kind {
            OutputKind::Encoded(output) => output.held_video_frame(),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => None,
        }
    }

    fn encode_video(&mut self, frame: &VideoFrame) -> Result<()> {
        Self::encode_video(self, frame)
    }

    fn encode_audio(&mut self, frame: &AudioFrame) -> Result<()> {
        Self::encode_audio(self, frame)
    }

    fn encode_processed_audio(&mut self, frame: &AudioFrame) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => output.encode_processed_audio(frame),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(output) => output.encode_audio(frame),
        }
    }

    fn apply_logo_overlay(
        &mut self,
        frame: &mut VideoFrame,
        logo: &LogoOverlay,
        opacity_factor: f64,
    ) {
        match &mut self.kind {
            OutputKind::Encoded(_) => blend_logo(frame, logo, opacity_factor),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => {}
        }
    }

    fn benchmarks_logo_overlay(&self) -> bool {
        match self.kind {
            OutputKind::Encoded(_) => true,
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => false,
        }
    }

    fn write_vtt_subtitles(
        &mut self,
        media_path: &str,
        output_start_ms: i64,
        source_start_ms: i64,
    ) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => {
                output.write_vtt_subtitles(media_path, output_start_ms, source_start_ms)
            }
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => Ok(()),
        }
    }

    fn clear_vtt_subtitles(&mut self) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => {
                output.clear_vtt_subtitles();

                Ok(())
            }
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => Ok(()),
        }
    }

    fn advance_vtt_subtitles(&mut self, output_position_ms: i64) -> Result<()> {
        match &mut self.kind {
            OutputKind::Encoded(output) => output.advance_vtt_subtitles(output_position_ms),
            #[cfg(feature = "desktop-base")]
            OutputKind::Desktop(_) => Ok(()),
        }
    }
}
