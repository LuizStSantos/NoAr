//! Engine output configuration facade. Validation and option catalogs have
//! dedicated owners; public imports remain stable for application consumers.

use std::{
    collections::BTreeMap,
    fmt,
    str::FromStr,
    sync::{Arc, PoisonError, RwLock},
};

mod codecs;
mod hls;
mod metadata;
mod recording;

pub use codecs::{
    AudioOptions, OUTPUT_AUDIO_SAMPLE_RATES, VideoOptionChoice, VideoOptionKind, VideoOptionSpec,
    VideoOptionVisibility, VideoOptions, audio_codec_uses_bitrate, validate_audio_options,
    validate_video_options, video_codec_uses_bitrate, video_option_defaults, video_option_specs,
};
pub use hls::{HlsSubtitle, HlsVariant};
pub use metadata::validate_output_metadata;
pub use recording::{RecordingConfig, RecordingEncodeConfig};

use crate::time_base::Rational;

use crate::{
    AudioEffectsControl, AudioFrameCallback, AudioLevelCallback, LiveLoudnessConfig,
    LiveLoudnessControl, LogoControl, LoudnessMeterControl,
};

#[cfg(test)]
mod log_level_tests {
    use super::LogLevel;

    #[test]
    fn parses_ui_log_levels() {
        assert_eq!("INFO".parse::<LogLevel>(), Ok(LogLevel::Info));
        assert_eq!("WARNING".parse::<LogLevel>(), Ok(LogLevel::Warning));
        assert_eq!("ERROR".parse::<LogLevel>(), Ok(LogLevel::Error));
    }

    #[test]
    fn rejects_unknown_log_levels() {
        assert!("everything".parse::<LogLevel>().is_err());
    }
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub width: u32,
    pub height: u32,
    pub desktop_window_size: Option<(u32, u32)>,
    pub desktop_fullscreen: bool,
    pub fps: u32,
    pub sample_rate: u32,
    pub video_time_base: Rational,
    pub audio_time_base: Rational,
    pub audio_effects: AudioEffectsControl,
    /// Live-ingest-only EBU R128 gain rider and ceiling limiter settings.
    pub loudness_scope: crate::LoudnessScope,
    pub live_loudness: LiveLoudnessConfig,
    pub live_loudness_control: LiveLoudnessControl,
    pub audio_level_callback: Option<AudioLevelCallback>,
    pub audio_frame_callback: Option<AudioFrameCallback>,
    pub loudness_meter_control: LoudnessMeterControl,
    pub logo: Option<LogoConfig>,
    /// Hides or swaps the logo on air from outside the playout.
    pub logo_control: LogoControl,
    /// Switches the L-shape on air from outside the playout.
    pub lshape_control: crate::LShapeControl,
    /// Takes template graphics on and off air from outside the playout.
    pub graphics_control: crate::GraphicsControl,
    pub text: Option<TextConfig>,
    pub text_overlay_state: TextOverlayState,
    pub stream_type: StreamType,
    pub stream_format: String,
    pub video_codec: String,
    pub video_options: VideoOptions,
    /// Validated AVIO/protocol options used only while opening network output.
    pub protocol_options: BTreeMap<String, String>,
    pub muxer_options: BTreeMap<String, String>,
    pub metadata_options: BTreeMap<String, String>,
    pub audio_codec: String,
    pub audio_options: AudioOptions,
    pub audio_bitrate: u64,
    /// Sample rate of the encoded output audio. `None` encodes at the
    /// engine rate (`sample_rate`); otherwise the encoder input is
    /// resampled, leaving the internal timeline and processing unchanged.
    pub output_sample_rate: Option<u32>,
    pub media_log_level: LogLevel,
    pub ingest_log_level: LogLevel,
    pub log_ignore_lines: Vec<String>,
    pub channel_id: Option<i32>,
    pub desktop_control_callback: Option<DesktopControlCallback>,
    pub recording: Option<RecordingConfig>,
    /// Address that receives a live MPEG-TS copy of the first encoded
    /// rendition, for a low-latency preview (for example
    /// `udp://127.0.0.1:41000?pkt_size=1316`).
    pub preview_url: Option<String>,
    /// SCTE-35 ad break signalling, on MPEG-TS stream outputs.
    pub scte35: Option<crate::Scte35Config>,
    /// The breaks the channel signals; outlasts a playout restart.
    pub splice_control: crate::SpliceControl,
    /// How a network output reaches its destination; outlasts a restart.
    pub output_link: crate::OutputLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopControlCommand {
    Back,
    Next,
    Reset,
}

#[derive(Clone)]
pub struct DesktopControlCallback(Arc<dyn Fn(DesktopControlCommand) + Send + Sync>);

impl fmt::Debug for DesktopControlCallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DesktopControlCallback(..)")
    }
}

impl DesktopControlCallback {
    pub fn new(callback: impl Fn(DesktopControlCommand) + Send + Sync + 'static) -> Self {
        Self(Arc::new(callback))
    }

    pub fn invoke(&self, command: DesktopControlCommand) {
        (self.0)(command);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum StreamType {
    #[default]
    Rtmp,
    Srt,
    Udp,
    Custom,
}

impl StreamType {
    pub fn muxer(self, custom_format: &str) -> &str {
        match self {
            Self::Rtmp => "flv",
            Self::Srt | Self::Udp => "mpegts",
            Self::Custom => custom_format,
        }
    }
}
pub use super::protocol::validate_output_protocol_options;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Quiet,
    Panic,
    Fatal,
    Error,
    #[default]
    Warning,
    Info,
    Verbose,
    Debug,
    Trace,
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(level: &str) -> Result<Self, Self::Err> {
        match level.to_ascii_lowercase().as_str() {
            "quiet" | "off" => Ok(Self::Quiet),
            "panic" => Ok(Self::Panic),
            "fatal" => Ok(Self::Fatal),
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warning),
            "info" => Ok(Self::Info),
            "verbose" => Ok(Self::Verbose),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(format!("unsupported log level {level:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogoConfig {
    pub path: String,
    pub scale: Option<String>,
    pub opacity: f64,
    pub position: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextConfig {
    pub text: Option<String>,
    pub use_filename: bool,
    pub filename_regex: Option<String>,
    pub font_family: Option<String>,
    pub font_weight: TextWeight,
    pub font_size: f32,
    pub line_spacing: f32,
    pub text_color: RgbaColor,
    pub opacity: f64,
    pub position_x: TextPosition,
    pub position_y: TextPosition,
    pub background: Option<TextBackgroundConfig>,
    pub scroll: TextScroll,
    pub scroll_repeat: i32,
    pub fade_in_seconds: f64,
    pub fade_out_seconds: f64,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            text: None,
            use_filename: false,
            filename_regex: None,
            font_family: None,
            font_weight: TextWeight::Normal,
            font_size: 48.0,
            line_spacing: 0.0,
            text_color: RgbaColor::opaque(255, 255, 255),
            opacity: 1.0,
            position_x: TextPosition::Pixels(32),
            position_y: TextPosition::Pixels(32),
            background: None,
            scroll: TextScroll::None,
            scroll_repeat: -1,
            fade_in_seconds: 0.0,
            fade_out_seconds: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextWeight {
    #[default]
    Normal,
    Semibold,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPosition {
    Pixels(i32),
    Center,
    End(i32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBackgroundConfig {
    pub color: RgbaColor,
    pub padding: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextScroll {
    None,
    LeftToRight { pixels_per_second: u32 },
    RightToLeft { pixels_per_second: u32 },
}

#[derive(Debug, Clone, Default)]
pub struct TextOverlayState {
    inner: Arc<RwLock<TextOverlayStateInner>>,
}

#[derive(Debug, Clone, Default)]
struct TextOverlayStateInner {
    revision: u64,
    config: Option<TextConfig>,
    start_pts: Option<i64>,
}

impl TextOverlayState {
    pub fn set(&self, config: Option<TextConfig>) {
        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);
        inner.revision = inner.revision.wrapping_add(1);
        inner.config = config;
        inner.start_pts = None;
    }

    pub fn clear(&self) {
        self.set(None);
    }

    pub(crate) fn snapshot_at(&self, pts: i64) -> TextOverlaySnapshot {
        // Fast path with a read lock: this is called once per rendered frame,
        // the write lock is only needed right after a new config was set.
        {
            let inner = self.inner.read().unwrap_or_else(PoisonError::into_inner);

            if inner.config.is_none() || inner.start_pts.is_some() {
                return TextOverlaySnapshot {
                    revision: inner.revision,
                    config: inner.config.clone(),
                    start_pts: inner.start_pts,
                };
            }
        }

        let mut inner = self.inner.write().unwrap_or_else(PoisonError::into_inner);

        if inner.config.is_some() && inner.start_pts.is_none() {
            inner.start_pts = Some(pts);
        }
        TextOverlaySnapshot {
            revision: inner.revision,
            config: inner.config.clone(),
            start_pts: inner.start_pts,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TextOverlaySnapshot {
    pub revision: u64,
    pub config: Option<TextConfig>,
    pub start_pts: Option<i64>,
}

impl OutputConfig {
    pub fn new(width: u32, height: u32, fps: u32, sample_rate: u32) -> Self {
        Self {
            width,
            height,
            desktop_window_size: None,
            desktop_fullscreen: false,
            fps,
            sample_rate,
            video_time_base: Rational(1, fps as i32),
            audio_time_base: Rational(1, sample_rate as i32),
            audio_effects: AudioEffectsControl::default(),
            loudness_scope: crate::LoudnessScope::Off,
            live_loudness: LiveLoudnessConfig::default(),
            live_loudness_control: LiveLoudnessControl::new(false, LiveLoudnessConfig::default()),
            audio_level_callback: None,
            audio_frame_callback: None,
            loudness_meter_control: LoudnessMeterControl::default(),
            logo: None,
            logo_control: LogoControl::default(),
            lshape_control: crate::LShapeControl::default(),
            graphics_control: crate::GraphicsControl::default(),
            text: None,
            text_overlay_state: TextOverlayState::default(),
            stream_type: StreamType::Rtmp,
            stream_format: String::new(),
            video_codec: "libx264".to_string(),
            video_options: video_option_defaults("libx264"),
            protocol_options: BTreeMap::new(),
            muxer_options: BTreeMap::new(),
            metadata_options: BTreeMap::new(),
            audio_codec: "aac".to_string(),
            audio_options: AudioOptions::new(),
            audio_bitrate: 128_000,
            output_sample_rate: None,
            media_log_level: LogLevel::Warning,
            ingest_log_level: LogLevel::Warning,
            log_ignore_lines: Vec::new(),
            channel_id: None,
            desktop_control_callback: None,
            recording: None,
            preview_url: None,
            scte35: None,
            splice_control: crate::SpliceControl::default(),
            output_link: crate::OutputLink::default(),
        }
    }

    pub fn with_volume(mut self, volume: f64) -> anyhow::Result<Self> {
        self.audio_effects = AudioEffectsControl::new(volume)?;

        Ok(self)
    }

    pub fn with_audio_effects(mut self, audio_effects: AudioEffectsControl) -> Self {
        self.audio_effects = audio_effects;
        self
    }

    pub fn with_loudness_scope(mut self, scope: crate::LoudnessScope) -> Self {
        self.loudness_scope = scope;

        self
    }

    pub fn with_live_loudness(mut self, live_loudness: LiveLoudnessConfig) -> Self {
        self.live_loudness = live_loudness;
        self
    }

    pub fn with_live_loudness_control(mut self, control: LiveLoudnessControl) -> Self {
        self.live_loudness_control = control;
        self
    }

    pub fn with_audio_level_callback(mut self, callback: Option<AudioLevelCallback>) -> Self {
        self.audio_level_callback = callback;
        self
    }

    pub fn with_audio_frame_callback(mut self, callback: Option<AudioFrameCallback>) -> Self {
        self.audio_frame_callback = callback;
        self
    }

    pub fn with_loudness_meter_control(mut self, control: LoudnessMeterControl) -> Self {
        self.loudness_meter_control = control;
        self
    }

    pub fn with_desktop_fullscreen(mut self, fullscreen: bool) -> Self {
        self.desktop_fullscreen = fullscreen;
        self
    }

    pub fn with_desktop_control_callback(mut self, callback: DesktopControlCallback) -> Self {
        self.desktop_control_callback = Some(callback);
        self
    }

    pub fn with_logo(mut self, logo: Option<LogoConfig>) -> Self {
        self.logo = logo;
        self
    }

    pub fn with_logo_control(mut self, logo_control: LogoControl) -> Self {
        self.logo_control = logo_control;
        self
    }

    pub fn with_lshape_control(mut self, lshape_control: crate::LShapeControl) -> Self {
        self.lshape_control = lshape_control;
        self
    }

    pub fn with_graphics_control(mut self, graphics_control: crate::GraphicsControl) -> Self {
        self.graphics_control = graphics_control;
        self
    }

    pub fn with_text(mut self, text: Option<TextConfig>) -> Self {
        self.text = text;
        self
    }

    pub fn with_text_overlay_state(mut self, text_overlay_state: TextOverlayState) -> Self {
        self.text_overlay_state = text_overlay_state;
        self
    }

    pub fn with_stream_type(mut self, stream_type: StreamType) -> Self {
        self.stream_type = stream_type;
        self
    }

    pub fn with_stream_format(mut self, stream_format: String) -> Self {
        self.stream_format = stream_format;
        self
    }

    pub fn with_encoding(
        mut self,
        video_codec: String,
        video_options: VideoOptions,
        audio_codec: String,
        audio_options: AudioOptions,
        audio_bitrate: u64,
    ) -> Self {
        self.video_codec = video_codec;
        self.video_options = video_options;
        self.audio_codec = audio_codec;
        self.audio_options = audio_options;
        self.audio_bitrate = audio_bitrate;
        self
    }

    /// Sets the encoded output's audio sample rate; `None` keeps the engine rate.
    pub fn with_output_sample_rate(mut self, output_sample_rate: Option<u32>) -> Self {
        self.output_sample_rate = output_sample_rate;
        self
    }

    /// The sample rate the audio encoder receives.
    pub fn encoded_sample_rate(&self) -> u32 {
        self.output_sample_rate.unwrap_or(self.sample_rate)
    }

    pub fn with_muxer_options(mut self, muxer_options: BTreeMap<String, String>) -> Self {
        self.muxer_options = muxer_options;
        self
    }

    pub fn with_metadata_options(mut self, metadata_options: BTreeMap<String, String>) -> Self {
        self.metadata_options = metadata_options;
        self
    }

    pub fn with_protocol_options(mut self, protocol_options: BTreeMap<String, String>) -> Self {
        self.protocol_options = protocol_options;
        self
    }

    pub fn with_recording(mut self, recording: Option<RecordingConfig>) -> Self {
        self.recording = recording;
        self
    }

    pub fn with_preview_url(mut self, preview_url: Option<String>) -> Self {
        self.preview_url = preview_url;
        self
    }

    pub fn with_scte35(mut self, scte35: Option<crate::Scte35Config>) -> Self {
        self.scte35 = scte35;
        self
    }

    pub fn with_splice_control(mut self, splice_control: crate::SpliceControl) -> Self {
        self.splice_control = splice_control;
        self
    }

    pub fn with_output_link(mut self, output_link: crate::OutputLink) -> Self {
        self.output_link = output_link;
        self
    }

    pub fn video_option(&self, key: &str) -> Option<&str> {
        self.video_options.get(key).map(String::as_str)
    }

    pub fn video_maxrate(&self) -> u64 {
        self.video_option("maxrate")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(2_400)
            .saturating_mul(1_000)
    }

    pub fn with_logging(mut self, media_log_level: LogLevel, ingest_log_level: LogLevel) -> Self {
        self.media_log_level = media_log_level;
        self.ingest_log_level = ingest_log_level;
        self
    }

    pub fn with_log_ignore_lines(mut self, ignore_lines: Vec<String>) -> Self {
        self.log_ignore_lines = ignore_lines;
        self
    }

    pub fn with_channel_id(mut self, channel_id: i32) -> Self {
        self.channel_id = Some(channel_id);
        self
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self::new(1024, 576, 25, 48_000)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputSize {
    pub width: u32,
    pub height: u32,
}

impl FromStr for OutputSize {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (width, height) = value
            .split_once(':')
            .or_else(|| value.split_once('x'))
            .ok_or_else(|| "size must use WIDTH:HEIGHT or WIDTHxHEIGHT".to_string())?;
        let width = width
            .parse::<u32>()
            .map_err(|_| "width must be a positive integer".to_string())?;
        let height = height
            .parse::<u32>()
            .map_err(|_| "height must be a positive integer".to_string())?;
        if width == 0 || height == 0 {
            return Err("width and height must be greater than zero".to_string());
        }

        if width % 2 != 0 || height % 2 != 0 {
            return Err("width and height must be even for YUV420 output".to_string());
        }

        Ok(Self { width, height })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{OutputSize, StreamType, validate_output_protocol_options};

    #[test]
    fn validates_srt_latency_and_udp_packet_size() {
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "srt://example.invalid:9000",
                &BTreeMap::from([("latency".to_string(), "2000000".to_string())]),
            )
            .is_ok()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Udp,
                "udp://239.0.0.1:1234",
                &BTreeMap::from([("pkt_size".to_string(), "1316".to_string())]),
            )
            .is_ok()
        );
    }

    #[test]
    fn validates_srt_passphrase_without_logging_or_transforming_it() {
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "srt://example.invalid:9000",
                &BTreeMap::from([(
                    "passphrase".to_string(),
                    "a sufficiently long secret".to_string(),
                )]),
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_unknown_mismatched_and_managed_protocol_options() {
        let option =
            |name: &str, value: &str| BTreeMap::from([(name.to_string(), value.to_string())]);
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "srt://example.invalid:9000",
                &option("latnecy", "2000000"),
            )
            .is_err()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Udp,
                "srt://example.invalid:9000",
                &option("pkt_size", "1316"),
            )
            .is_err()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Rtmp,
                "rtmp://example.invalid/live",
                &option("rw_timeout", "0"),
            )
            .is_err()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Srt,
                "srt://example.invalid:9000",
                &option("mode", "listener"),
            )
            .is_err()
        );
    }

    #[test]
    fn custom_outputs_use_the_transport_from_the_url() {
        assert!(
            validate_output_protocol_options(
                StreamType::Custom,
                "udp://239.0.0.1:1234",
                &BTreeMap::from([("pkt_size".to_string(), "1316".to_string())]),
            )
            .is_ok()
        );
        assert!(
            validate_output_protocol_options(
                StreamType::Custom,
                "rist://example.invalid:9000",
                &BTreeMap::from([("buffer_size".to_string(), "65536".to_string())]),
            )
            .is_err()
        );
    }

    #[test]
    fn parses_output_size_with_colon() {
        let size = "1280:720".parse::<OutputSize>().unwrap();
        assert_eq!(size.width, 1280);
        assert_eq!(size.height, 720);
    }

    #[test]
    fn parses_output_size_with_x() {
        let size = "1920x1080".parse::<OutputSize>().unwrap();
        assert_eq!(size.width, 1920);
        assert_eq!(size.height, 1080);
    }

    #[test]
    fn rejects_odd_output_size() {
        assert!("1023:576".parse::<OutputSize>().is_err());
        assert!("1024:575".parse::<OutputSize>().is_err());
    }
}
