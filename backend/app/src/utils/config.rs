//! Channel configuration facade and database-to-runtime assembly.
//!
//! Output/recording policies are owned by their child modules; `update` applies
//! validated changes transactionally and reconciles long-lived runtime controls.
//! Public DTO names, serde defaults and generated TypeScript contracts remain here
//! through re-exports so callers do not depend on the implementation layout.

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::NaiveTime;
use chrono_tz::Tz;
use flexi_logger::Level;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use tokio::{fs, io::AsyncReadExt};
use ts_rs::TS;

mod output;
mod recording;
pub(crate) mod update;

use crate::{
    ARGS,
    db::{handles, models},
    file::norm_abs_path,
    utils::{errors::ServiceError, time_to_sec},
};

pub use output::{Output, OutputMode, StreamType};
pub use recording::{Recording, RecordingSource};

pub const DUMMY_LEN: f64 = 60.0;
pub(crate) const ENGINE_AUDIO_SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum ProcessMode {
    Folder,
    #[default]
    Playlist,
}

impl ProcessMode {
    fn new(s: &str) -> Self {
        match s {
            "folder" => Self::Folder,
            _ => Self::Playlist,
        }
    }
}

impl fmt::Display for ProcessMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ProcessMode::Folder => write!(f, "folder"),
            ProcessMode::Playlist => write!(f, "playlist"),
        }
    }
}

impl FromStr for ProcessMode {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "folder" => Ok(Self::Folder),
            "playlist" => Ok(Self::Playlist),
            _ => Err("Use 'folder' or 'playlist'".to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct Template {
    pub sources: Vec<Source>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct Source {
    #[ts(type = "string")]
    pub start: NaiveTime,
    #[ts(type = "string")]
    pub duration: NaiveTime,
    pub shuffle: bool,
    pub paths: Vec<PathBuf>,
}

/// Channel Config
///
/// This we init ones, when NoAr is starting and use them globally in the hole program.
#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct PlayoutConfig {
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub channel: Channel,
    #[ts(skip)]
    #[serde(skip)]
    pub logo_model: Option<crate::db::models::GraphicsTemplate>,
    #[ts(skip)]
    #[serde(skip)]
    pub graphics_data:
        std::collections::HashMap<i32, std::sync::Arc<crate::graphics::sources::DataRows>>,
    pub general: General,
    pub mail: Mail,
    #[serde(default)]
    pub notification: Notification,
    pub logging: Logging,
    pub processing: Processing,
    pub audio: Audio,
    pub playlist: Playlist,
    pub storage: Storage,
    pub task: Task,
    pub recording: Recording,
    #[serde(default)]
    pub scte35: Scte35,
    #[serde(alias = "out")]
    pub output: Output,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
pub struct Channel {
    pub logs: PathBuf,
    pub public: PathBuf,
    pub playlists: PathBuf,
    pub storage: PathBuf,
    pub shared: bool,
    #[ts(type = "string")]
    pub timezone: Option<Tz>,
}

impl Channel {
    pub fn new(config: &models::GlobalSettings, channel: models::Channel) -> Self {
        Self {
            logs: PathBuf::from(config.logs.clone()),
            public: PathBuf::from(channel.public.clone()),
            playlists: PathBuf::from(channel.playlists.clone()),
            storage: PathBuf::from(channel.storage.clone()),
            shared: config.shared,
            timezone: channel.timezone,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct General {
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub id: i32,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub channel_id: i32,
    pub stop_threshold: f64,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub generate: Option<Vec<String>>,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub template: Option<Template>,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub skip_validation: bool,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub validate: bool,
}

impl General {
    fn new(config: &models::Configuration) -> Self {
        Self {
            id: config.id,
            channel_id: config.channel_id,
            stop_threshold: config.general_stop_threshold,
            generate: None,
            template: None,
            skip_validation: false,
            validate: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Mail {
    #[serde(skip_deserializing)]
    pub show: bool,
    pub subject: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub smtp_server: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub smtp_starttls: bool,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub smtp_user: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub smtp_password: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub smtp_port: u16,
    pub recipient: String,
    #[ts(type = "string")]
    pub mail_level: Level,
    pub interval: i64,
}

impl Mail {
    fn new(global: &models::GlobalSettings, config: &models::Configuration) -> Self {
        Self {
            show: !global.smtp_password.is_empty() && global.smtp_server != "mail.example.org",
            subject: config.mail_subject.clone(),
            smtp_server: global.smtp_server.clone(),
            smtp_starttls: global.smtp_starttls,
            smtp_user: global.smtp_user.clone(),
            smtp_password: global.smtp_password.clone(),
            smtp_port: global.smtp_port,
            recipient: config.mail_recipient.clone(),
            mail_level: string_to_log_level(config.mail_level.clone()),
            interval: config.mail_interval,
        }
    }
}

impl Default for Mail {
    fn default() -> Self {
        Mail {
            show: false,
            subject: String::default(),
            smtp_server: String::default(),
            smtp_starttls: bool::default(),
            smtp_user: String::default(),
            smtp_password: String::default(),
            smtp_port: 465,
            recipient: String::default(),
            mail_level: Level::Debug,
            interval: i64::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
    #[default]
    Fatal,
}

impl fmt::Display for NotificationLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        })
    }
}

impl NotificationLevel {
    fn new(value: &str) -> Self {
        match value {
            "INFO" => Self::Info,
            "WARNING" => Self::Warning,
            "ERROR" => Self::Error,
            "FATAL" => Self::Fatal,
            invalid => {
                log::warn!(
                    target: "notification",
                    "Unknown notification level {invalid:?}; falling back to FATAL"
                );
                Self::Fatal
            }
        }
    }

    pub fn accepts(self, level: Level, fatal: bool) -> bool {
        match self {
            Self::Fatal => fatal,
            Self::Error => level <= Level::Error,
            Self::Warning => level <= Level::Warn,
            Self::Info => level <= Level::Info,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Notification {
    #[serde(skip_deserializing)]
    pub show: bool,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub server: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub token: String,
    pub topic: String,
    pub level: NotificationLevel,
    pub tags: String,
}

impl Notification {
    fn new(global: &models::GlobalSettings, config: &models::Configuration) -> Self {
        Self {
            show: !global.notification_server.is_empty(),
            server: global.notification_server.clone(),
            token: global.notification_token.clone(),
            topic: config.notification_topic.clone(),
            level: NotificationLevel::new(&config.notification_level),
            tags: config.notification_tags.clone(),
        }
    }
}

impl Default for Notification {
    fn default() -> Self {
        Self {
            show: false,
            server: String::new(),
            token: String::new(),
            topic: String::new(),
            level: NotificationLevel::Fatal,
            tags: String::new(),
        }
    }
}

#[cfg(test)]
mod notification_level_tests {
    use flexi_logger::Level;

    use super::NotificationLevel;

    #[test]
    fn fatal_notifications_only_accept_fatal_records() {
        assert!(NotificationLevel::Fatal.accepts(Level::Error, true));
        assert!(!NotificationLevel::Fatal.accepts(Level::Error, false));
    }

    #[test]
    fn error_notifications_include_fatal_and_regular_errors() {
        assert!(NotificationLevel::Error.accepts(Level::Error, true));
        assert!(NotificationLevel::Error.accepts(Level::Error, false));
        assert!(!NotificationLevel::Error.accepts(Level::Warn, false));
    }

    #[test]
    fn info_notifications_exclude_debug_and_trace_records() {
        assert!(NotificationLevel::Info.accepts(Level::Info, false));
        assert!(!NotificationLevel::Info.accepts(Level::Debug, false));
        assert!(!NotificationLevel::Info.accepts(Level::Trace, false));
    }

    #[test]
    fn unknown_notification_level_falls_back_to_fatal() {
        assert_eq!(NotificationLevel::new("INVALID"), NotificationLevel::Fatal);
    }

    #[test]
    fn the_media_log_level_is_read_under_its_old_name_too() {
        let old: super::Logging = serde_json::from_value(serde_json::json!({
            "ffmpeg_level": "WARNING",
            "ingest_level": "ERROR",
            "detect_silence": false,
            "ignore_lines": []
        }))
        .unwrap();
        assert_eq!(old.media_level, "WARNING");

        // A client sending back what it read has both names: the new one wins.
        let both: super::Logging = serde_json::from_value(serde_json::json!({
            "media_level": "ERROR",
            "ffmpeg_level": "DEBUG",
            "ingest_level": "ERROR",
            "detect_silence": false,
            "ignore_lines": []
        }))
        .unwrap();
        assert_eq!(both.media_level, "ERROR");

        let written = serde_json::to_value(&both).unwrap();
        assert_eq!(written["media_level"], "ERROR");
        assert_eq!(written["ffmpeg_level"], "ERROR");
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[serde(from = "LoggingFields")]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Logging {
    /// Level of the media engine's (GStreamer's) messages in the log.
    pub media_level: String,
    pub ingest_level: String,
    pub detect_silence: bool,
    pub ignore_lines: Vec<String>,
    /// `media_level` under its name before 2.1, still returned so older API
    /// clients keep reading it. Deprecated: read `media_level`.
    #[ts(skip)]
    #[serde(rename = "ffmpeg_level")]
    pub deprecated_ffmpeg_level: String,
}

/// What a client may send: `media_level`, or the older `ffmpeg_level`.
/// When both come (a client sending back what it read), `media_level` wins.
#[derive(Deserialize)]
struct LoggingFields {
    #[serde(default)]
    media_level: Option<String>,
    #[serde(default)]
    ffmpeg_level: Option<String>,
    #[serde(default)]
    ingest_level: String,
    #[serde(default)]
    detect_silence: bool,
    #[serde(default)]
    ignore_lines: Vec<String>,
}

impl From<LoggingFields> for Logging {
    fn from(fields: LoggingFields) -> Self {
        let media_level = fields
            .media_level
            .filter(|level| !level.is_empty())
            .or(fields.ffmpeg_level)
            .unwrap_or_default();

        Self {
            deprecated_ffmpeg_level: media_level.clone(),
            media_level,
            ingest_level: fields.ingest_level,
            detect_silence: fields.detect_silence,
            ignore_lines: fields.ignore_lines,
        }
    }
}

impl Logging {
    fn new(config: &models::Configuration) -> Self {
        Self {
            media_level: config.logging_media_level.clone(),
            deprecated_ffmpeg_level: config.logging_media_level.clone(),
            ingest_level: config.logging_ingest_level.clone(),
            detect_silence: config.logging_detect_silence,
            ignore_lines: config
                .logging_ignore
                .split(';')
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(String::from)
                .collect(),
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Processing {
    pub mode: ProcessMode,
    pub add_logo: bool,
    pub logo: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub logo_path: String,
    pub logo_scale: String,
    pub logo_opacity: f64,
    pub logo_position: String,
    #[serde(default)]
    pub vtt_enable: bool,
    #[serde(default)]
    pub vtt_dummy: Option<String>,
    #[serde(default = "default_vtt_name")]
    pub vtt_name: String,
    #[serde(default = "default_vtt_language")]
    pub vtt_language: String,
    #[serde(default)]
    pub vtt_default: bool,
    /// Channel icon of the XMLTV guide, relative to the storage; empty uses
    /// the logo.
    #[serde(default)]
    pub guide_logo: String,
    /// Key that lets an encoder hide, show or swap the logo on air; empty
    /// turns that off.
    #[serde(default)]
    pub logo_key: String,
    /// Key that lets an encoder or a script extend or end the live on air;
    /// empty turns that off.
    #[serde(default)]
    pub live_key: String,
    /// How the logo comes up.
    #[serde(default)]
    pub logo_transition_in: LogoTransition,
    /// How the logo goes off.
    #[serde(default)]
    pub logo_transition_out: LogoTransition,
    /// Length of the logo's fades, and of the cross-fade when another image
    /// takes its place, in seconds; empty keeps the built-in lengths (1 s on
    /// clips, 0.4 s when switched from outside).
    #[serde(default)]
    pub logo_transition_seconds: Option<f64>,
    /// The L-shape graphic, a full-screen image relative to the storage;
    /// empty turns the L-shape off.
    #[serde(default)]
    pub lshape: String,
    /// Size of the picture in the L-shape, in percent of the screen.
    #[serde(default = "default_lshape_scale")]
    pub lshape_scale: f64,
    /// Corner the picture goes to: `top-right`, `top-left`, `bottom-right`
    /// or `bottom-left`.
    #[serde(default = "default_lshape_corner")]
    pub lshape_corner: String,
    /// Quick scenes: named sets of logo and L-shape changes applied in one
    /// go, from NoAr or an encoder.
    #[serde(default)]
    pub graphics_scenes: Vec<GraphicsScene>,
}

/// The most scenes a channel keeps.
pub const MAX_GRAPHICS_SCENES: usize = 24;

/// What a scene does to the logo.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum SceneLogo {
    /// Leaves it as it is.
    #[default]
    Keep,
    Hide,
    /// Brings back the channel logo.
    Show,
    /// Shows `logo_image` in its place.
    Replace,
}

/// What a scene does to the L-shape.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum SceneLShape {
    #[default]
    Keep,
    On,
    Off,
}

/// A named set of logo and L-shape changes, applied in one go.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct GraphicsScene {
    /// The scene's name; a scheduled graphics event takes its title.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub logo: SceneLogo,
    /// Storage path of the image, for `replace`.
    #[serde(default)]
    pub logo_image: Option<String>,
    /// Where the logo goes (`X%:Y%`, a place or `default`); none leaves it.
    #[serde(default)]
    pub logo_position: Option<String>,
    #[serde(default)]
    pub lshape: SceneLShape,
    /// Storage path of the L-shape graphic to use; none leaves the current.
    #[serde(default)]
    pub lshape_graphic: Option<String>,
    /// The corner the picture goes to (or `default`); none leaves it.
    #[serde(default)]
    pub lshape_corner: Option<String>,
    /// Size of the picture, in percent of the screen; none keeps it.
    #[serde(default)]
    pub lshape_scale: Option<f64>,
}

impl GraphicsScene {
    /// How a scene is called from an address: its name, lower case, with
    /// dashes for anything but letters and digits.
    pub fn slug(&self) -> String {
        slug(&self.name)
    }
}

impl GraphicsScene {
    /// Why the scene's values are not accepted.
    pub fn check(&self) -> Result<(), String> {
        let scene = self;

        let blank =
            |value: &Option<String>| value.as_deref().is_none_or(|value| value.trim().is_empty());

        if scene.logo == SceneLogo::Replace && blank(&scene.logo_image) {
            return Err(format!(
                "scene '{}': choose the image that replaces the logo",
                scene.name.trim()
            ));
        }

        if let Some(corner) = scene.lshape_corner.as_deref().map(str::trim)
            && corner != "default"
            && noar_engine::LShapeCorner::parse(corner).is_none()
        {
            return Err(format!(
                "scene '{}': unknown L-shape corner '{corner}'",
                scene.name.trim()
            ));
        }

        if let Some(scale) = scene.lshape_scale
            && !(noar_engine::LSHAPE_MIN_SCALE..=noar_engine::LSHAPE_MAX_SCALE).contains(&scale)
        {
            return Err(format!(
                "scene '{}': the picture in the L-shape must be between {} and {}% of the screen",
                scene.name.trim(),
                noar_engine::LSHAPE_MIN_SCALE,
                noar_engine::LSHAPE_MAX_SCALE
            ));
        }

        Ok(())
    }
}

/// `name` as it is written in a scene's address.
pub fn slug(name: &str) -> String {
    let mut slug = String::new();

    for character in name.trim().to_lowercase().chars() {
        if character.is_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }

    slug.trim_matches('-').to_string()
}

fn default_lshape_scale() -> f64 {
    75.0
}

fn default_lshape_corner() -> String {
    "top-right".to_string()
}

/// How the logo comes up or goes off.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "lowercase")]
pub enum LogoTransition {
    #[default]
    Fade,
    Cut,
}

impl LogoTransition {
    fn new(s: &str) -> Self {
        match s {
            "cut" => Self::Cut,
            _ => Self::Fade,
        }
    }

    fn engine(self) -> noar_engine::LogoTransition {
        match self {
            Self::Fade => noar_engine::LogoTransition::Fade,
            Self::Cut => noar_engine::LogoTransition::Cut,
        }
    }
}

impl fmt::Display for LogoTransition {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Fade => write!(f, "fade"),
            Self::Cut => write!(f, "cut"),
        }
    }
}

fn default_vtt_name() -> String {
    "Subtitles".to_string()
}

fn default_vtt_language() -> String {
    "und".to_string()
}

impl Processing {
    fn new(config: &models::Configuration) -> Self {
        Self {
            mode: ProcessMode::new(&config.processing_mode.clone()),
            add_logo: config.processing_add_logo,
            logo: config.processing_logo.clone(),
            logo_path: config.processing_logo.clone(),
            logo_scale: config.processing_logo_scale.clone(),
            logo_opacity: config.processing_logo_opacity,
            logo_position: config.processing_logo_position.clone(),
            vtt_enable: config.processing_vtt_enable,
            vtt_dummy: config.processing_vtt_dummy.clone(),
            vtt_name: config.processing_vtt_name.clone(),
            vtt_language: config.processing_vtt_language.clone(),
            vtt_default: config.processing_vtt_default,
            guide_logo: config.processing_guide_logo.clone(),
            logo_key: config.processing_logo_key.clone(),
            live_key: config.processing_live_key.clone(),
            logo_transition_in: LogoTransition::new(&config.processing_logo_transition_in),
            logo_transition_out: LogoTransition::new(&config.processing_logo_transition_out),
            logo_transition_seconds: config.processing_logo_transition_seconds,
            lshape: config.processing_lshape.clone(),
            lshape_scale: config.processing_lshape_scale,
            lshape_corner: config.processing_lshape_corner.clone(),
            graphics_scenes: serde_json::from_str(&config.processing_graphics_scenes)
                .unwrap_or_default(),
        }
    }

    /// Why the scenes are not accepted: names, actions and values.
    pub fn check_graphics_scenes(&self) -> Result<(), String> {
        if self.graphics_scenes.len() > MAX_GRAPHICS_SCENES {
            return Err(format!("at most {MAX_GRAPHICS_SCENES} scenes"));
        }

        let mut names = std::collections::HashSet::new();

        for scene in &self.graphics_scenes {
            let slug = scene.slug();

            if slug.is_empty() {
                return Err("every scene needs a name".to_string());
            }

            if !names.insert(slug) {
                return Err(format!("two scenes are called '{}'", scene.name.trim()));
            }

            scene.check()?;
        }

        Ok(())
    }

    /// The L-shape for a storage at `storage`, or none when it is off; why
    /// the settings are not accepted.
    pub fn lshape_config(
        &self,
        storage: &std::path::Path,
    ) -> Result<Option<noar_engine::LShapeConfig>, String> {
        if self.lshape.trim().is_empty() {
            return Ok(None);
        }

        let corner = noar_engine::LShapeCorner::parse(&self.lshape_corner)
            .ok_or_else(|| format!("unknown L-shape corner '{}'", self.lshape_corner))?;

        if !(noar_engine::LSHAPE_MIN_SCALE..=noar_engine::LSHAPE_MAX_SCALE)
            .contains(&self.lshape_scale)
        {
            return Err(format!(
                "the picture in the L-shape must be between {} and {}% of the screen",
                noar_engine::LSHAPE_MIN_SCALE,
                noar_engine::LSHAPE_MAX_SCALE
            ));
        }

        Ok(Some(noar_engine::LShapeConfig {
            path: storage
                .join(self.lshape.trim())
                .to_string_lossy()
                .to_string(),
            scale: self.lshape_scale,
            corner,
        }))
    }

    /// How the logo comes up, goes off and gives way to another image, or
    /// why the length is not accepted.
    pub fn logo_transitions(&self) -> Result<noar_engine::LogoTransitions, String> {
        if let Some(seconds) = self.logo_transition_seconds
            && !(seconds.is_finite()
                && seconds > 0.0
                && seconds <= noar_engine::MAX_LOGO_TRANSITION_SECONDS)
        {
            return Err(format!(
                "logo transition length must be more than 0 and at most {} seconds",
                noar_engine::MAX_LOGO_TRANSITION_SECONDS
            ));
        }

        Ok(noar_engine::LogoTransitions {
            show: self.logo_transition_in.engine(),
            hide: self.logo_transition_out.engine(),
            seconds: self.logo_transition_seconds,
        })
    }

    pub fn hls_subtitle(&self) -> Result<Option<noar_engine::HlsSubtitle>, String> {
        if !self.vtt_enable {
            return Ok(None);
        }

        let subtitle = noar_engine::HlsSubtitle {
            name: self.vtt_name.trim().to_string(),
            language: self.vtt_language.trim().to_string(),
            default: self.vtt_default,
        };
        subtitle.validate()?;
        Ok(Some(subtitle))
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Audio {
    pub volume: f64,
    #[ts(type = "\"all\" | \"live\" | \"off\"")]
    pub loudness_scope: String,
    pub compressor_ratio: f64,
    pub compressor_attack_ms: f64,
    pub compressor_hold_ms: f64,
    pub compressor_release_ms: f64,
    pub compressor_strong_release_ms: f64,
    pub compressor_knee_db: f64,
    pub pause_hold_ms: f64,
    pub pause_return_delay_ms: f64,
    pub loudness_output_max_correction_db: f64,
    pub loudness_output_gain_up_db_per_second: f64,
    pub loudness_output_gain_down_db_per_second: f64,

    pub compressor_threshold_dbfs: f64,
    pub pause_threshold_dbfs: f64,
    // Derived from loudness_scope.
    pub loudness_enable: bool,
    pub loudness_target_lufs: f64,
    pub loudness_dead_band_lu: f64,
    pub loudness_max_gain_db: f64,
    pub loudness_max_attenuation_db: f64,
    pub loudness_gain_up_db_per_second: f64,
    pub loudness_gain_down_db_per_second: f64,
    pub loudness_silence_gate_lufs: f64,
    pub loudness_true_peak_ceiling_dbtp: f64,
}

impl Audio {
    fn new(config: &models::Configuration) -> Self {
        Self {
            volume: config.processing_volume,
            loudness_scope: config.processing_loudness_scope.clone(),
            compressor_ratio: config.processing_compressor_ratio,
            compressor_attack_ms: config.processing_compressor_attack_ms,
            compressor_hold_ms: config.processing_compressor_hold_ms,
            compressor_release_ms: config.processing_compressor_release_ms,
            compressor_strong_release_ms: config.processing_compressor_strong_release_ms,
            compressor_knee_db: config.processing_compressor_knee_db,
            pause_hold_ms: config.processing_pause_hold_ms,
            pause_return_delay_ms: config.processing_pause_return_delay_ms,
            loudness_output_max_correction_db: config.processing_loudness_output_max_correction_db,
            loudness_output_gain_up_db_per_second: config
                .processing_loudness_output_gain_up_db_per_second,
            loudness_output_gain_down_db_per_second: config
                .processing_loudness_output_gain_down_db_per_second,

            compressor_threshold_dbfs: config.processing_compressor_threshold_dbfs,
            pause_threshold_dbfs: config.processing_pause_threshold_dbfs,
            loudness_enable: config.processing_loudness_scope != "off",
            loudness_target_lufs: config.processing_loudness_target_lufs,
            loudness_dead_band_lu: config.processing_loudness_dead_band_lu,
            loudness_max_gain_db: config.processing_loudness_max_gain_db,
            loudness_max_attenuation_db: config.processing_loudness_max_attenuation_db,
            loudness_gain_up_db_per_second: config.processing_loudness_gain_up_db_per_second,
            loudness_gain_down_db_per_second: config.processing_loudness_gain_down_db_per_second,
            loudness_silence_gate_lufs: config.processing_loudness_silence_gate_lufs,
            loudness_true_peak_ceiling_dbtp: config.processing_loudness_true_peak_ceiling_dbtp,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Playlist {
    pub day_start: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub start_sec: Option<f64>,
    pub length: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub length_sec: Option<f64>,
    pub infinit: bool,
}

impl Playlist {
    fn new(config: &models::Configuration) -> Self {
        Self {
            day_start: config.playlist_day_start.clone(),
            start_sec: None,
            length: config.playlist_length.clone(),
            length_sec: None,
            infinit: config.playlist_infinit,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Storage {
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub path: PathBuf,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub paths: Vec<PathBuf>,
    pub filler: String,
    #[ts(skip)]
    #[serde(skip_serializing, skip_deserializing)]
    pub filler_path: PathBuf,
    pub extensions: Vec<String>,
    pub shuffle: bool,
    /// Convert uploaded videos to the output's size, frame rate and sample
    /// rate when they arrive.
    #[serde(default)]
    pub convert_uploads: bool,
    #[serde(skip_deserializing)]
    pub shared_storage: bool,
}

impl Storage {
    fn new(config: &models::Configuration, path: PathBuf, shared_storage: bool) -> Self {
        Self {
            path,
            paths: vec![],
            filler: config.storage_filler.clone(),
            filler_path: PathBuf::from(config.storage_filler.clone()),
            extensions: config
                .storage_extensions
                .split(';')
                .map(String::from)
                .collect(),
            shuffle: config.storage_shuffle,
            convert_uploads: config.storage_convert_uploads,
            shared_storage,
        }
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Task {
    pub enable: bool,
    pub path: PathBuf,
}

impl Task {
    fn new(config: &models::Configuration) -> Self {
        Self {
            enable: config.task_enable,
            path: PathBuf::from(config.task_path.clone()),
        }
    }
}

/// SCTE-35 ad break signalling. NoAr marks every break the same way and
/// leaves adapting it to each platform to what receives the stream.
#[derive(Debug, Clone, Deserialize, Serialize, TS, PartialEq)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(default)]
pub struct Scte35 {
    pub enable: bool,
    /// PID the splice messages are carried on.
    pub pid: u16,
    /// Seconds between the first message of a break and its start.
    pub preroll: f64,
    /// Runs of commercials outside these lengths, in seconds, are not
    /// signalled.
    pub min_break: f64,
    pub max_break: f64,
    /// Lets encoders and automation mark breaks at `/scte35/{id}/...`
    /// without logging in; empty turns that route off.
    pub key: String,
}

impl Default for Scte35 {
    fn default() -> Self {
        Self {
            enable: false,
            pid: noar_engine::DEFAULT_SCTE35_PID,
            preroll: noar_engine::DEFAULT_SCTE35_PREROLL_SECONDS,
            min_break: 30.0,
            max_break: 300.0,
            key: String::new(),
        }
    }
}

impl Scte35 {
    fn new(scte35: &models::Scte35) -> Self {
        Self {
            enable: scte35.enabled,
            pid: u16::try_from(scte35.pid).unwrap_or(noar_engine::DEFAULT_SCTE35_PID),
            preroll: scte35.preroll,
            min_break: scte35.min_break,
            max_break: scte35.max_break,
            key: scte35.api_key.clone(),
        }
    }

    pub fn engine_config(&self) -> Option<noar_engine::Scte35Config> {
        self.enable.then_some(noar_engine::Scte35Config {
            pid: self.pid,
            preroll_seconds: self.preroll,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        noar_engine::Scte35Config {
            pid: self.pid,
            preroll_seconds: self.preroll,
        }
        .validate()?;

        if !self.min_break.is_finite()
            || !self.max_break.is_finite()
            || self.min_break <= 0.0
            || self.max_break < self.min_break
        {
            return Err(
                "SCTE-35 break lengths must be positive, the shortest no longer than the longest"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Whether a run of commercials this long is signalled.
    pub fn signals(&self, seconds: f64) -> bool {
        (self.min_break..=self.max_break).contains(&seconds)
    }
}

pub fn string_to_log_level(l: String) -> Level {
    match l.to_lowercase().as_str() {
        "error" => Level::Error,
        "info" => Level::Info,
        "trace" => Level::Trace,
        "warning" => Level::Warn,
        _ => Level::Debug,
    }
}

impl PlayoutConfig {
    pub async fn new(
        pool: &Pool<Sqlite>,
        channel_id: i32,
        output_id: Option<i32>,
    ) -> Result<Self, ServiceError> {
        let global = handles::select_global(pool).await?;
        let channel = handles::select_channel(pool, &channel_id).await?;
        let mut config = handles::select_configuration(pool, channel_id).await?;
        let recording_config = handles::select_recording(pool, channel_id).await?;
        let scte35 = Scte35::new(&handles::select_scte35(pool, channel_id).await?);
        let outputs = handles::select_outputs(pool, channel_id).await?;

        if let Some(id) = output_id {
            config.output_id = id;
        }

        let channel = Channel::new(&global, channel);
        let general = General::new(&config);
        let mail = Mail::new(&global, &config);
        let notification = Notification::new(&global, &config);
        let logging = Logging::new(&config);
        let mut processing = Processing::new(&config);
        let audio = Audio::new(&config);
        let mut playlist = Playlist::new(&config);
        let task = Task::new(&config);
        let recording = Recording::new(&recording_config);
        let output = Output::new(&config, outputs);
        let mut storage = Storage::new(&config, channel.storage.clone(), channel.shared);

        if !channel.playlists.is_dir() {
            fs::create_dir_all(&channel.playlists).await?;
        }

        if !channel.logs.is_dir() {
            fs::create_dir_all(&channel.logs).await?;
        }

        let (filler_path, _, filler) = norm_abs_path(&channel.storage, &config.storage_filler)?;

        storage.filler = filler;
        storage.filler_path = filler_path;

        playlist.start_sec = Some(time_to_sec(&playlist.day_start, &channel.timezone));

        if playlist.length.contains(':') {
            playlist.length_sec = Some(time_to_sec(&playlist.length, &channel.timezone));
        } else {
            playlist.length_sec = Some(86400.0);
        }

        let (logo_path, _, logo) = norm_abs_path(&channel.storage, &processing.logo)?;

        processing.logo = logo;
        processing.logo_path = logo_path.to_string_lossy().to_string();

        let bindings = handles::select_graphics_bindings(pool, channel_id).await?;
        let logo_model = match bindings
            .iter()
            .find(|binding| binding.role == crate::db::models::GraphicsRole::Logo)
        {
            Some(binding) => {
                handles::select_graphics_template(pool, channel_id, binding.template_id).await?
            }
            None => None,
        };

        let graphics_data = handles::select_graphics_sources(pool, channel_id)
            .await?
            .into_iter()
            .map(|source| (source.id, std::sync::Arc::new(source.rows)))
            .collect();

        Ok(Self {
            channel,
            logo_model,
            graphics_data,
            general,
            mail,
            notification,
            logging,
            processing,
            audio,
            playlist,
            storage,
            task,
            recording,
            scte35,
            output,
        })
    }

    pub async fn dump(pool: &Pool<Sqlite>, id: i32) -> Result<(), ServiceError> {
        let config = Self::new(pool, id, None).await?;

        let toml_string = toml_edit::ser::to_string_pretty(&config)?;
        tokio::fs::write(&format!("noar_{id}.toml"), toml_string).await?;

        Ok(())
    }

    pub async fn import(pool: &Pool<Sqlite>, id: i32, path: &Path) -> Result<(), ServiceError> {
        if path.is_file() {
            let mut file = tokio::fs::File::open(path).await?;
            let mut contents = String::new();
            file.read_to_string(&mut contents).await?;

            let config: PlayoutConfig = toml_edit::de::from_str(&contents).unwrap();

            handles::update_configuration(pool, id, config).await?;
        } else {
            return Err(ServiceError::BadRequest("Path not exists!".to_string()));
        }

        Ok(())
    }
}

/// Read command line arguments, and override the config with them.
pub async fn get_config(
    pool: &Pool<Sqlite>,
    channel_id: i32,
) -> Result<PlayoutConfig, ServiceError> {
    let args = ARGS.clone();
    let output_id = if let Some(output_name) = &args.output {
        let outputs = handles::select_outputs(pool, channel_id).await?;
        outputs
            .iter()
            .find(|out| out.name == output_name.to_string())
            .map(|out| out.id)
    } else {
        None
    };

    let mut config = PlayoutConfig::new(pool, channel_id, output_id).await?;

    config.general.generate = args.generate;
    config.general.validate = args.validate;
    config.general.skip_validation = args.skip_validation;

    if let Some(template_file) = args.template {
        let mut f = fs::File::options()
            .read(true)
            .write(false)
            .open(template_file)
            .await?;
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer).await?;

        let mut template: Template = serde_json::from_slice(&buffer)?;

        template.sources.sort_by_key(|d1| d1.start);

        config.general.template = Some(template);
    }

    if let Some(paths) = args.paths {
        config.storage.paths = paths;
    }

    if let Some(storage) = args.storage {
        config.channel.storage = PathBuf::from(&storage);
        config.storage.path = PathBuf::from(&storage);
    }

    if let Some(log) = args.logs {
        config.channel.logs = PathBuf::from(&log);
    }

    if let Some(playlist) = args.playlists {
        config.channel.playlists = PathBuf::from(&playlist);
    }

    if let Some(public) = args.public {
        config.channel.public = PathBuf::from(&public);
    }

    if let Some(folder) = args.folder {
        config.channel.storage = folder;
        config.processing.mode = ProcessMode::Folder;
    }

    if let Some(start) = args.start {
        config.playlist.day_start.clone_from(&start);
        config.playlist.start_sec = Some(time_to_sec(&start, &config.channel.timezone));
    }

    if let Some(output) = args.output {
        config.output.mode = output;
    }

    if let Some(volume) = args.volume {
        config.audio.volume = volume;
    }

    if let Some(smtp_server) = args.smtp_server {
        config.mail.smtp_server = smtp_server;
    }

    if let Some(smtp_user) = args.smtp_user {
        config.mail.smtp_user = smtp_user;
    }

    if let Some(smtp_password) = args.smtp_password {
        config.mail.smtp_password = smtp_password;
    }

    if args.smtp_starttls.is_some_and(|v| &v == "true") {
        config.mail.smtp_starttls = true;
    }

    if let Some(smtp_port) = args.smtp_port {
        config.mail.smtp_port = smtp_port;
    }

    Ok(config)
}

#[cfg(test)]
mod external_live_source_tests {
    use super::PlayoutConfig;

    #[test]
    fn configuration_api_has_no_input_server_settings() {
        let config = serde_json::to_value(PlayoutConfig::default()).unwrap();

        assert!(config.get("ingest").is_none());
    }

    #[test]
    fn legacy_input_server_payload_cannot_restore_listener_settings() {
        let mut payload = serde_json::to_value(PlayoutConfig::default()).unwrap();
        payload["ingest"] = serde_json::json!({
            "listeners": [{"enabled": true, "backend": "srt", "identifier": "srt://0.0.0.0:9000"}]
        });
        let config: PlayoutConfig = serde_json::from_value(payload).unwrap();

        assert!(
            serde_json::to_value(config)
                .unwrap()
                .get("ingest")
                .is_none()
        );
    }
}

#[cfg(test)]
mod graphics_scene_tests {
    use super::{GraphicsScene, Processing, SceneLogo, slug};

    fn scene(name: &str) -> GraphicsScene {
        GraphicsScene {
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_scene_is_called_by_its_name_in_lower_case_with_dashes() {
        assert_eq!(slug("  Intervalo Comercial! "), "intervalo-comercial");
        assert_eq!(slug("Plantão 24h"), "plantão-24h");
        assert_eq!(scene("A / B").slug(), "a-b");
    }

    #[test]
    fn scenes_need_unique_names_and_valid_values() {
        let mut processing = Processing {
            graphics_scenes: vec![scene("Intervalo"), scene("Plantão")],
            ..Default::default()
        };
        assert_eq!(processing.check_graphics_scenes(), Ok(()));

        processing.graphics_scenes = vec![scene("Intervalo"), scene("intervalo!")];
        assert!(processing.check_graphics_scenes().is_err());

        processing.graphics_scenes = vec![scene("  ")];
        assert!(processing.check_graphics_scenes().is_err());

        let mut replace = scene("Troca");
        replace.logo = SceneLogo::Replace;
        processing.graphics_scenes = vec![replace.clone()];
        assert!(processing.check_graphics_scenes().is_err());
        replace.logo_image = Some("00-assets/ao-vivo.png".to_string());
        processing.graphics_scenes = vec![replace];
        assert_eq!(processing.check_graphics_scenes(), Ok(()));

        let mut corner = scene("Canto");
        corner.lshape_corner = Some("middle".to_string());
        processing.graphics_scenes = vec![corner.clone()];
        assert!(processing.check_graphics_scenes().is_err());
        corner.lshape_corner = Some("bottom-left".to_string());
        corner.lshape_scale = Some(20.0);
        processing.graphics_scenes = vec![corner.clone()];
        assert!(processing.check_graphics_scenes().is_err());
        corner.lshape_scale = Some(70.0);
        processing.graphics_scenes = vec![corner];
        assert_eq!(processing.check_graphics_scenes(), Ok(()));
    }
}
