//! Output DTOs and encoder/transport validation used before playout starts.

use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::models;

use super::ENGINE_AUDIO_SAMPLE_RATE;

#[derive(Debug, Clone, Default, Eq, PartialEq, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    Desktop,
    #[default]
    HLS,
    Stream,
    /// The uncompressed program published as an NDI source on the network.
    Ndi,
    /// The program on a baseband video card (Blackmagic DeckLink SDI/HDMI).
    Sdi,
}

impl OutputMode {
    fn new(s: &str) -> Self {
        match s {
            "desktop" => Self::Desktop,
            "stream" => Self::Stream,
            "ndi" => Self::Ndi,
            "sdi" => Self::Sdi,
            _ => Self::HLS,
        }
    }
}

impl FromStr for OutputMode {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "desktop" => Ok(Self::Desktop),
            "hls" => Ok(Self::HLS),
            "stream" => Ok(Self::Stream),
            "ndi" => Ok(Self::Ndi),
            "sdi" => Ok(Self::Sdi),
            _ => Err("Use 'desktop', 'hls', 'stream', 'ndi' or 'sdi'".to_string()),
        }
    }
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            OutputMode::Desktop => write!(f, "desktop"),
            OutputMode::HLS => write!(f, "hls"),
            OutputMode::Stream => write!(f, "stream"),
            OutputMode::Ndi => write!(f, "ndi"),
            OutputMode::Sdi => write!(f, "sdi"),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum StreamType {
    #[default]
    Rtmp,
    Srt,
    Udp,
    Custom,
}

impl StreamType {
    fn output_target(self) -> Option<noar_engine::OutputTarget> {
        match self {
            Self::Rtmp => Some(noar_engine::OutputTarget::Rtmp),
            Self::Srt => Some(noar_engine::OutputTarget::Srt),
            Self::Udp => Some(noar_engine::OutputTarget::Udp),
            Self::Custom => None,
        }
    }

    pub fn engine_stream_type(self) -> noar_engine::StreamType {
        match self {
            Self::Rtmp => noar_engine::StreamType::Rtmp,
            Self::Srt => noar_engine::StreamType::Srt,
            Self::Udp => noar_engine::StreamType::Udp,
            Self::Custom => noar_engine::StreamType::Custom,
        }
    }
}

impl FromStr for StreamType {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "rtmp" => Ok(Self::Rtmp),
            "srt" => Ok(Self::Srt),
            "udp" => Ok(Self::Udp),
            "custom" => Ok(Self::Custom),
            _ => Err("Use 'rtmp', 'srt', 'udp' or 'custom'".to_string()),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Output {
    pub id: i32,
    pub mode: OutputMode,
    #[serde(default)]
    pub stream_url: String,
    #[serde(default)]
    pub stream_type: StreamType,
    #[serde(default)]
    pub stream_format: String,
    #[serde(default = "default_hls_playlist_name")]
    pub hls_playlist_name: String,
    #[serde(default = "default_hls_segment_duration")]
    pub hls_segment_duration: u32,
    #[serde(default = "default_hls_list_size")]
    pub hls_list_size: u32,
    #[serde(default)]
    pub desktop_fullscreen: bool,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    #[serde(default = "default_video_codec")]
    pub video_codec: String,
    #[serde(default)]
    pub video_options: BTreeMap<String, String>,
    /// GStreamer transport options used while opening a network stream.
    #[serde(default)]
    pub protocol_options: BTreeMap<String, String>,
    /// GStreamer muxer options and supported legacy HLS options for this output.
    #[serde(default)]
    pub muxer_options: BTreeMap<String, String>,
    /// Global container metadata. Availability depends on the output muxer.
    #[serde(default)]
    pub metadata_options: BTreeMap<String, String>,
    #[serde(default = "default_audio_codec")]
    pub audio_codec: String,
    /// GStreamer properties and supported legacy options for the selected audio encoder.
    #[serde(default)]
    pub audio_options: BTreeMap<String, String>,
    #[serde(default = "default_audio_bitrate")]
    pub audio_bitrate: u32,
    /// Encoded output audio sample rate in Hz (32000, 44100 or 48000). The
    /// engine processes at 48 kHz and resamples just before the encoder.
    #[serde(default = "default_output_audio_sample_rate")]
    pub audio_sample_rate: u32,
    /// Adaptive HLS renditions, one per entry, each formatted as
    /// `NAME:WIDTHxHEIGHT:VIDEO_BITRATE[:AUDIO_BITRATE]` (e.g.
    /// `high:1920x1080:5000k:192k`). Only relevant when `mode == HLS`;
    /// entries are added to the base rendition configured directly on this
    /// output.
    #[serde(default)]
    pub hls_variants: Vec<String>,
}

fn default_hls_playlist_name() -> String {
    "stream".to_string()
}

const fn default_hls_segment_duration() -> u32 {
    6
}

const fn default_hls_list_size() -> u32 {
    600
}

fn default_video_codec() -> String {
    "libx264".to_string()
}

fn default_audio_codec() -> String {
    "aac".to_string()
}

const fn default_output_audio_sample_rate() -> u32 {
    ENGINE_AUDIO_SAMPLE_RATE
}

const fn default_audio_bitrate() -> u32 {
    128
}

impl Output {
    pub(super) fn new(config: &models::Configuration, outputs: Vec<models::Output>) -> Self {
        let output = outputs
            .iter()
            .find(|output| output.id == config.output_id)
            .cloned()
            .unwrap_or_default();
        let video_codec = output.video_codec.unwrap_or_else(default_video_codec);
        let video_options = serde_json::from_str(&output.video_options)
            .unwrap_or_else(|_| noar_engine::video_option_defaults(&video_codec));
        let protocol_options = serde_json::from_str(&output.protocol_options).unwrap_or_default();
        let muxer_options = serde_json::from_str(&output.muxer_options).unwrap_or_default();
        let metadata_options = serde_json::from_str(&output.metadata_options).unwrap_or_default();
        let audio_options = serde_json::from_str(&output.audio_options).unwrap_or_default();

        Self {
            id: output.id,
            mode: OutputMode::new(&output.name),
            stream_url: output.stream_url,
            stream_type: output
                .stream_type
                .as_deref()
                .unwrap_or("rtmp")
                .parse()
                .unwrap_or_default(),
            stream_format: output.stream_format.unwrap_or_default(),
            hls_playlist_name: output
                .hls_playlist_name
                .unwrap_or_else(default_hls_playlist_name),
            hls_segment_duration: output
                .hls_segment_duration
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(default_hls_segment_duration),
            hls_list_size: output
                .hls_list_size
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(default_hls_list_size),
            desktop_fullscreen: output.desktop_fullscreen,
            width: u32::try_from(output.width).unwrap_or(1280),
            height: u32::try_from(output.height).unwrap_or(720),
            fps: output.fps,
            video_codec,
            video_options,
            protocol_options,
            muxer_options,
            metadata_options,
            audio_codec: output.audio_codec.unwrap_or_else(default_audio_codec),
            audio_options,
            audio_bitrate: output
                .audio_bitrate
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(default_audio_bitrate),
            audio_sample_rate: output
                .audio_sample_rate
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or_else(default_output_audio_sample_rate),
            hls_variants: output
                .hls_variants
                .split(';')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        }
    }

    /// Parses `hls_variants` into engine-ready [`noar_engine::HlsVariant`]s,
    /// returning a descriptive error for the offending entry on failure.
    pub fn parsed_hls_variants(&self) -> Result<Vec<noar_engine::HlsVariant>, String> {
        self.hls_variants
            .iter()
            .map(|spec| {
                spec.parse::<noar_engine::HlsVariant>()
                    .map_err(|e| format!("invalid HLS variant \"{spec}\": {e}"))
            })
            .collect()
    }

    /// Returns all HLS renditions, with the configured base output first and
    /// additional variants appended.
    pub fn hls_streams(&self) -> Result<Vec<noar_engine::HlsVariant>, String> {
        let base = noar_engine::HlsVariant {
            name: self.hls_playlist_name.trim().to_string(),
            width: self.width,
            height: self.height,
            video_bitrate: self.video_maxrate(),
            audio_bitrate: u64::from(self.audio_bitrate) * 1_000,
        };
        validate_hls_name(&base.name)?;

        let additional = self.parsed_hls_variants()?;
        let mut names = HashSet::new();

        names.insert(base.name.as_str());
        for stream in &additional {
            validate_hls_name(&stream.name)?;
            if !names.insert(stream.name.as_str()) {
                return Err(format!("duplicate HLS stream name {:?}", stream.name));
            }
        }

        let mut streams = Vec::with_capacity(additional.len() + 1);
        streams.push(base);
        streams.extend(additional);
        Ok(streams)
    }

    fn validate_hls_audio_options(&self) -> Result<(), String> {
        for variant in self.hls_streams()?.iter().skip(1) {
            noar_engine::validate_audio_options(
                &self.audio_codec,
                &self.audio_options,
                self.audio_sample_rate,
                variant.audio_bitrate,
            )
            .map_err(|error| format!("HLS variant {:?}: {error}", variant.name))?;
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("output size must be greater than zero".to_string());
        }
        if !self.fps.is_finite() || self.fps < 1.0 || self.fps > f64::from(u32::MAX) {
            return Err("output fps must be a positive number".to_string());
        }
        if self
            .muxer_options
            .iter()
            .any(|(key, value)| key.trim().is_empty() || value.trim().is_empty())
        {
            return Err("muxer option names and values must not be empty".to_string());
        }
        if self.mode != OutputMode::Stream && !self.protocol_options.is_empty() {
            return Err(
                "protocol options can only be used with network stream outputs".to_string(),
            );
        }
        noar_engine::validate_output_metadata(&self.metadata_options)?;
        if self.mode == OutputMode::Desktop && !self.metadata_options.is_empty() {
            return Err("container metadata is unavailable for desktop output".to_string());
        }

        if self.mode == OutputMode::Ndi {
            if !self.muxer_options.is_empty() {
                return Err("NDI output has no container options".to_string());
            }
            noar_engine::validate_ndi_name(&self.stream_url)?;
            if let Some(reason) = noar_engine::ndi_unavailable_reason() {
                return Err(format!(
                    "NDI output is unavailable on this server: {reason}"
                ));
            }
        }

        if self.mode == OutputMode::Sdi {
            if !self.muxer_options.is_empty() {
                return Err("SDI output has no container options".to_string());
            }
            let card = noar_engine::parse_decklink(&self.stream_url, true)?.ok_or(
                "SDI output needs a DeckLink address, such as decklink://0?mode=1080i5994",
            )?;
            let mode = card.mode.ok_or("SDI output needs a video mode")?;
            // The channel renders whole frames; the card's 29.97/59.94 is met
            // by repeating a frame now and then, other rates would judder.
            let card_fps = f64::from(mode.fps_num) / f64::from(mode.fps_den);
            if (self.fps.round() - card_fps).abs() > 0.5 {
                return Err(format!(
                    "the output frame rate ({}) must match the card mode {} ({card_fps:.2} frames/s)",
                    self.fps, mode.name
                ));
            }
        }

        // NDI and SDI encode only the Player preview, as MPEG-TS like UDP.
        if matches!(
            self.mode,
            OutputMode::HLS | OutputMode::Stream | OutputMode::Ndi | OutputMode::Sdi
        ) {
            let target = match self.mode {
                OutputMode::HLS => noar_engine::OutputTarget::Hls,
                OutputMode::Ndi | OutputMode::Sdi => noar_engine::OutputTarget::Udp,
                OutputMode::Stream => self
                    .stream_type
                    .output_target()
                    .unwrap_or(noar_engine::OutputTarget::Rtmp),
                OutputMode::Desktop => unreachable!("desktop output is not encoded"),
            };
            let capabilities = noar_engine::media_capabilities();
            let custom_format = self.stream_format.trim();
            if self.mode == OutputMode::Stream && self.stream_type == StreamType::Custom {
                if custom_format.is_empty() {
                    return Err("custom stream format must not be empty".to_string());
                }
                if !capabilities.has_muxer_named(custom_format) {
                    return Err(format!(
                        "GStreamer output format {custom_format:?} is not available"
                    ));
                }
            }
            let muxer_name = match self.mode {
                OutputMode::HLS => "hls",
                OutputMode::Ndi | OutputMode::Sdi => "mpegts",
                OutputMode::Stream => match self.stream_type {
                    StreamType::Rtmp => "flv",
                    StreamType::Srt | StreamType::Udp => "mpegts",
                    StreamType::Custom => custom_format,
                },
                OutputMode::Desktop => unreachable!("desktop output is not encoded"),
            };
            noar_engine::validate_muxer_options(muxer_name, &self.muxer_options)?;
            if self.mode == OutputMode::Stream {
                noar_engine::validate_output_protocol_options(
                    self.stream_type.engine_stream_type(),
                    &self.stream_url,
                    &self.protocol_options,
                )?;
            }
            let video_codecs =
                if self.mode == OutputMode::Stream && self.stream_type == StreamType::Custom {
                    capabilities.usable_codecs(noar_engine::MediaKind::Video)
                } else {
                    capabilities.video_codecs_for(target)
                };
            if !video_codecs
                .iter()
                .any(|codec| codec.name == self.video_codec)
            {
                return Err(format!(
                    "unsupported video codec {:?} for {} output",
                    self.video_codec, self.mode
                ));
            }
            let audio_codecs =
                if self.mode == OutputMode::Stream && self.stream_type == StreamType::Custom {
                    capabilities.usable_codecs(noar_engine::MediaKind::Audio)
                } else {
                    capabilities.audio_codecs_for(target)
                };
            if !audio_codecs
                .iter()
                .filter(|codec| !codec.hardware)
                .any(|codec| codec.name == self.audio_codec)
            {
                return Err(format!(
                    "unsupported audio codec {:?} for {} output",
                    self.audio_codec, self.mode
                ));
            }
            noar_engine::validate_video_options(&self.video_codec, &self.video_options)?;
            if !noar_engine::OUTPUT_AUDIO_SAMPLE_RATES.contains(&self.audio_sample_rate) {
                return Err(format!(
                    "audio sample rate must be one of {:?} Hz",
                    noar_engine::OUTPUT_AUDIO_SAMPLE_RATES
                ));
            }
            noar_engine::validate_audio_options(
                &self.audio_codec,
                &self.audio_options,
                self.audio_sample_rate,
                u64::from(self.audio_bitrate) * 1_000,
            )?;
            if noar_engine::audio_codec_uses_bitrate(&self.audio_codec) && self.audio_bitrate == 0 {
                return Err("audio bitrate must be greater than zero".to_string());
            }
        }

        match self.mode {
            OutputMode::HLS => {
                if self.hls_segment_duration == 0 {
                    return Err("HLS segment duration must be greater than zero".to_string());
                }
                self.validate_hls_audio_options()?;
            }
            OutputMode::Stream if self.stream_url.trim().is_empty() => {
                return Err("stream output URL must not be empty".to_string());
            }
            _ => {}
        }

        Ok(())
    }

    fn video_maxrate(&self) -> u64 {
        self.video_options
            .get("maxrate")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(2_400)
            .saturating_mul(1_000)
    }
}

fn validate_hls_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("HLS stream name must not be empty".to_string());
    }
    if !name
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(
            "HLS stream name may only contain ASCII letters, numbers, '_' and '-'".to_string(),
        );
    }
    if name == "master" {
        return Err("HLS stream name \"master\" is reserved".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod output_tests {
    use std::collections::BTreeMap;

    use super::{Output, OutputMode, StreamType};

    fn output(mode: OutputMode) -> Output {
        Output {
            id: 1,
            mode,
            stream_url: "rtmp://localhost/live/test".to_string(),
            stream_type: StreamType::Rtmp,
            stream_format: String::new(),
            hls_playlist_name: "stream".to_string(),
            hls_segment_duration: 6,
            hls_list_size: 600,
            desktop_fullscreen: false,
            width: 1280,
            height: 720,
            fps: 25.0,
            video_codec: "libx264".to_string(),
            video_options: noar_engine::video_option_defaults("libx264"),
            protocol_options: BTreeMap::new(),
            muxer_options: BTreeMap::new(),
            metadata_options: BTreeMap::new(),
            audio_codec: "aac".to_string(),
            audio_options: BTreeMap::new(),
            audio_bitrate: 128,
            audio_sample_rate: 48_000,
            hls_variants: Vec::new(),
        }
    }

    #[test]
    fn sdi_output_needs_a_card_mode_matching_the_frame_rate() {
        let mut output = output(OutputMode::Sdi);
        output.width = 1920;
        output.height = 1080;
        output.fps = 29.97;
        output.stream_url = "decklink://1?mode=1080i5994".to_string();
        assert!(output.validate().is_ok(), "{:?}", output.validate());

        output.fps = 30.0;
        assert!(output.validate().is_ok());
        output.stream_url = "decklink://1?mode=1080p30".to_string();
        assert!(output.validate().is_ok());

        output.fps = 25.0;
        assert!(output.validate().unwrap_err().contains("frame rate"));

        output.fps = 30.0;
        output.stream_url = "decklink://1".to_string();
        assert!(output.validate().unwrap_err().contains("mode"));
        output.stream_url = "rtmp://example.com/live".to_string();
        assert!(output.validate().unwrap_err().contains("DeckLink"));
        assert_eq!("sdi".parse::<OutputMode>(), Ok(OutputMode::Sdi));
    }

    #[test]
    fn ndi_output_needs_a_name_and_the_runtime() {
        let mut output = output(OutputMode::Ndi);
        output.stream_url = "   ".to_string();
        assert!(output.validate().unwrap_err().contains("NDI source name"));

        output.stream_url = "NoAr Canal 1".to_string();
        output
            .muxer_options
            .insert("x".to_string(), "1".to_string());
        assert!(output.validate().unwrap_err().contains("container"));
        output.muxer_options.clear();

        match noar_engine::ndi_unavailable_reason() {
            Some(_) => assert!(output.validate().unwrap_err().contains("unavailable")),
            None => assert!(output.validate().is_ok()),
        }
        assert_eq!("ndi".parse::<OutputMode>(), Ok(OutputMode::Ndi));
        assert_eq!(OutputMode::Ndi.to_string(), "ndi");
    }

    #[test]
    fn output_audio_sample_rate_is_limited_to_supported_rates() {
        let mut output = output(OutputMode::Stream);
        output.audio_sample_rate = 44_100;
        assert!(output.validate().is_ok());
        output.audio_sample_rate = 22_050;
        assert!(output.validate().unwrap_err().contains("sample rate"));
    }

    #[test]
    fn validates_structured_output_settings() {
        assert!(output(OutputMode::HLS).validate().is_ok());
        assert!(output(OutputMode::Stream).validate().is_ok());
    }

    #[test]
    fn output_metadata_roundtrips_and_is_rejected_for_desktop() {
        let mut output = output(OutputMode::Stream);
        output
            .metadata_options
            .insert("copyright".into(), "© Example".into());
        assert!(output.validate().is_ok());
        let decoded: Output =
            serde_json::from_value(serde_json::to_value(&output).unwrap()).unwrap();
        assert_eq!(decoded.metadata_options, output.metadata_options);

        output.mode = OutputMode::Desktop;
        assert!(output.validate().unwrap_err().contains("desktop"));
    }

    #[test]
    fn validates_output_protocol_options_before_saving() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Srt;
        output.stream_url = "srt://localhost:9000".to_string();
        output
            .protocol_options
            .insert("latency".to_string(), "2000000".to_string());
        assert!(output.validate().is_ok());

        output
            .protocol_options
            .insert("rw_timeout".to_string(), "0".to_string());
        assert!(output.validate().unwrap_err().contains("managed by NoAr"));
        output.protocol_options.remove("rw_timeout");
        output.stream_url = "srt://localhost:9000?latency=20000".to_string();
        assert!(
            output
                .validate()
                .unwrap_err()
                .contains("conflicts with the output URL")
        );
        output.stream_url = "srt://localhost:9000?latency=2000000".to_string();
        assert!(output.validate().is_ok());
    }

    #[test]
    fn protocol_options_roundtrip_through_the_configuration_api_shape() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Srt;
        output.stream_url = "srt://localhost:9000".to_string();
        output.protocol_options = BTreeMap::from([
            ("latency".to_string(), "2000000".to_string()),
            (
                "passphrase".to_string(),
                "a sufficiently long secret".to_string(),
            ),
        ]);

        let json = serde_json::to_value(&output).unwrap();
        let decoded: Output = serde_json::from_value(json).unwrap();

        assert_eq!(decoded.protocol_options, output.protocol_options);
    }

    #[test]
    fn rejects_protocol_options_for_non_stream_outputs() {
        for mode in [OutputMode::HLS, OutputMode::Desktop] {
            let mut output = output(mode);
            output
                .protocol_options
                .insert("pkt_size".to_string(), "1316".to_string());
            assert!(
                output
                    .validate()
                    .unwrap_err()
                    .contains("only be used with network stream outputs")
            );
        }
    }

    #[test]
    fn audio_validation_checks_output_and_hls_variant_bitrates() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Srt;
        output.stream_url = "srt://localhost:9000".to_string();
        output.audio_codec = "mp2".to_string();
        output
            .audio_options
            .insert("cutoff".to_string(), "0".to_string());
        output.validate().unwrap();
        output.audio_bitrate = 129;
        assert!(
            output
                .validate()
                .unwrap_err()
                .contains("audio encoder options")
        );
        output.audio_bitrate = 128;
        // Exercise rendition validation directly: installations differ in
        // whether an MP2 encoder is available.
        output.mode = OutputMode::HLS;
        output.hls_variants = vec!["low:640x360:1000k:129k".to_string()];
        assert!(
            output
                .validate_hls_audio_options()
                .unwrap_err()
                .contains("HLS variant")
        );
        output.hls_variants = vec!["low:640x360:1000k:96k".to_string()];
        assert!(output.validate_hls_audio_options().is_ok());
    }

    #[test]
    fn validates_audio_encoder_options_before_saving() {
        let mut output = output(OutputMode::HLS);
        output
            .audio_options
            .insert("aac_coder".to_string(), "fast".to_string());
        assert!(output.validate().is_ok());

        output
            .audio_options
            .insert("aac_codre".to_string(), "fast".to_string());
        assert!(
            output
                .validate()
                .unwrap_err()
                .contains("unsupported audio option")
        );
    }

    #[test]
    fn custom_stream_requires_an_output_format() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Custom;

        assert_eq!(
            output.validate(),
            Err("custom stream format must not be empty".to_string())
        );
    }

    #[test]
    fn custom_stream_accepts_an_available_muxer() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Custom;
        output.stream_format = "matroska".to_string();

        assert!(output.validate().is_ok());
    }

    #[test]
    fn custom_pcm_output_does_not_require_an_audio_bitrate() {
        let mut output = output(OutputMode::Stream);
        output.stream_type = StreamType::Custom;
        output.stream_format = "matroska".to_string();
        output.audio_codec = "pcm_s16le".to_string();
        output.audio_bitrate = 0;

        assert!(output.validate().is_ok());
    }

    #[test]
    fn rejects_zero_hls_segment_duration() {
        let mut output = output(OutputMode::HLS);
        output.hls_segment_duration = 0;
        assert_eq!(
            output.validate().unwrap_err(),
            "HLS segment duration must be greater than zero"
        );
    }

    #[test]
    fn rejects_hls_options_that_conflict_with_segment_management() {
        let mut config = output(OutputMode::HLS);
        config
            .muxer_options
            .insert("hls_time".to_string(), "30".to_string());

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unknown_muxer_options_before_saving() {
        let mut config = output(OutputMode::HLS);
        config
            .muxer_options
            .insert("hls_flgs".to_string(), "program_date_time".to_string());

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_compatible_hls_flags() {
        let mut config = output(OutputMode::HLS);
        config.muxer_options.insert(
            "hls_flags".to_string(),
            "program_date_time+independent_segments".to_string(),
        );

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_hls_variant() {
        let mut output = output(OutputMode::HLS);
        output.hls_variants = vec!["invalid".to_string()];
        assert!(
            output
                .validate()
                .unwrap_err()
                .contains("invalid HLS variant")
        );
    }

    #[test]
    fn hls_variants_are_added_after_base_output() {
        let mut output = output(OutputMode::HLS);
        output.hls_variants = vec!["low:640x360:800k:96k".to_string()];
        let streams = output.hls_streams().unwrap();

        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].name, "stream");
        assert_eq!(streams[0].width, 1280);
        assert_eq!(streams[1].name, "low");
    }

    #[test]
    fn standalone_hls_output_uses_base_stream() {
        let output = output(OutputMode::HLS);
        let streams = output.hls_streams().unwrap();

        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].name, "stream");
        assert_eq!(streams[0].width, 1280);
    }

    #[test]
    fn rejects_variant_with_same_name_as_base_output() {
        let mut output = output(OutputMode::HLS);
        output.hls_variants = vec!["stream:640x360:800k".to_string()];
        assert!(
            output
                .validate()
                .unwrap_err()
                .contains("duplicate HLS stream name")
        );
    }

    #[test]
    fn rejects_invalid_quality_for_visible_setting() {
        let mut output = output(OutputMode::Stream);
        output
            .video_options
            .insert("quality".to_string(), "52".to_string());
        assert!(output.validate().unwrap_err().contains("quality"));
    }

    #[test]
    fn cbr_does_not_validate_unused_quality() {
        let mut output = output(OutputMode::Stream);
        output
            .video_options
            .insert("rate_control".to_string(), "cbr".to_string());
        output
            .video_options
            .insert("quality".to_string(), "52".to_string());
        assert!(output.validate().is_ok());
    }
}
