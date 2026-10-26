use std::{error::Error, fmt, str::FromStr};

use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Pool, Row, Sqlite, sqlite::SqliteRow};

use crate::{
    db::handles,
    utils::config::{OutputMode, PlayoutConfig},
};

#[derive(Clone, Default, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct GlobalSettings {
    pub id: i32,
    pub secret: Option<String>,
    pub logs: String,
    pub playlists: String,
    pub public: String,
    pub storage: String,
    pub shared: bool,
    pub smtp_server: String,
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_starttls: bool,
    pub smtp_port: u16,
    pub notification_server: String,
    pub notification_token: String,
    pub setup_completed: bool,
}

impl GlobalSettings {
    pub async fn new(conn: &Pool<Sqlite>) -> Self {
        let global_settings = handles::select_global(conn);

        match global_settings.await {
            Ok(g) => g,
            Err(_) => Self {
                id: 0,
                secret: None,
                logs: String::new(),
                playlists: String::new(),
                public: String::new(),
                storage: String::new(),
                shared: false,
                smtp_server: String::new(),
                smtp_user: String::new(),
                smtp_password: String::new(),
                smtp_starttls: false,
                smtp_port: 465,
                notification_server: String::new(),
                notification_token: String::new(),
                setup_completed: true,
            },
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Channel {
    #[serde(default = "default_id", skip_deserializing)]
    pub id: i32,
    pub name: String,
    pub preview_url: String,
    pub extra_extensions: String,
    pub active: bool,
    pub public: String,
    pub playlists: String,
    pub storage: String,
    pub last_date: Option<String>,
    pub time_shift: f64,
    #[serde(default)]
    pub timezone: Option<Tz>,
}

impl FromRow<'_, SqliteRow> for Channel {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let mut timezone = None;

        if let Some(tz) = row
            .try_get::<String, _>("timezone")
            .ok()
            .and_then(|t: String| Tz::from_str(&t).ok())
        {
            timezone = Some(tz);
        } else if let Some(tz) = iana_time_zone::get_timezone()
            .ok()
            .and_then(|t: String| Tz::from_str(&t).ok())
        {
            timezone = Some(tz);
        }

        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            preview_url: row.try_get("preview_url").unwrap_or_default(),
            extra_extensions: row.try_get("extra_extensions").unwrap_or_default(),
            active: row.try_get("active").unwrap_or_default(),
            public: row.try_get("public").unwrap_or_default(),
            playlists: row.try_get("playlists").unwrap_or_default(),
            storage: row.try_get("storage").unwrap_or_default(),
            last_date: row.try_get("last_date").unwrap_or_default(),
            time_shift: row.try_get("time_shift").unwrap_or_default(),
            timezone,
        })
    }
}

fn default_id() -> i32 {
    1
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct User {
    #[serde(skip_deserializing)]
    pub id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail: Option<String>,
    pub username: String,
    #[serde(skip_serializing, default = "String::new")]
    pub password: String,
    pub role_id: Option<i32>,
    pub channel_ids: Option<Vec<i32>>,
    #[serde(default = "default_true")]
    pub two_factor: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

impl FromRow<'_, SqliteRow> for User {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id").unwrap_or_default(),
            mail: row.try_get("mail").ok(),
            username: row.try_get("username").unwrap_or_default(),
            password: row.try_get("password").unwrap_or_default(),
            role_id: row.try_get("role_id").ok(),
            channel_ids: Some(
                row.try_get::<String, &str>("channel_ids")
                    .unwrap_or_default()
                    .split(',')
                    .map(|i| i.parse::<i32>().unwrap_or_default())
                    .collect(),
            ),
            two_factor: row.try_get("two_factor").unwrap_or(true),
            token: None,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserMeta {
    pub id: i32,
    pub channels: Vec<i32>,
}

impl UserMeta {
    pub fn new(id: i32, channels: Vec<i32>) -> Self {
        Self { id, channels }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    GlobalAdmin,
    ChannelAdmin,
    User,
    #[default]
    Guest,
}

impl FromStr for Role {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "global_admin" => Ok(Self::GlobalAdmin),
            "channel_admin" => Ok(Self::ChannelAdmin),
            "user" => Ok(Self::User),
            _ => Ok(Self::Guest),
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::GlobalAdmin => write!(f, "global_admin"),
            Self::ChannelAdmin => write!(f, "channel_admin"),
            Self::User => write!(f, "user"),
            Self::Guest => write!(f, "guest"),
        }
    }
}

impl<'r> sqlx::decode::Decode<'r, ::sqlx::Sqlite> for Role
where
    &'r str: sqlx::decode::Decode<'r, sqlx::Sqlite>,
{
    fn decode(
        value: sqlx::sqlite::SqliteValueRef<'r>,
    ) -> Result<Self, Box<dyn Error + 'static + Send + Sync>> {
        let value = <&str as sqlx::decode::Decode<sqlx::Sqlite>>::decode(value)?;

        Ok(value.parse()?)
    }
}

impl FromRow<'_, SqliteRow> for Role {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        match row.get("name") {
            "global_admin" => Ok(Self::GlobalAdmin),
            "channel_admin" => Ok(Self::ChannelAdmin),
            "user" => Ok(Self::User),
            _ => Ok(Self::Guest),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow)]
/// Flattened read model assembled from the thematic `config_*` tables.
///
/// Keeping this projection separate from persistence lets the runtime build a
/// `PlayoutConfig` in one query without coupling writes to one wide table.
pub struct Configuration {
    pub id: i32,
    pub channel_id: i32,
    pub general_stop_threshold: f64,

    pub mail_subject: String,
    pub mail_recipient: String,
    pub mail_level: String,
    pub mail_interval: i64,

    pub notification_topic: String,
    pub notification_level: String,
    pub notification_tags: String,

    pub logging_ffmpeg_level: String,
    pub logging_ingest_level: String,
    pub logging_detect_silence: bool,
    #[serde(default)]
    pub logging_ignore: String,

    pub processing_mode: String,
    pub processing_add_logo: bool,
    pub processing_logo: String,
    pub processing_logo_scale: String,
    pub processing_logo_opacity: f64,
    pub processing_logo_position: String,
    pub processing_volume: f64,
    pub processing_loudness_scope: String,
    pub processing_compressor_ratio: f64,
    pub processing_compressor_attack_ms: f64,
    pub processing_compressor_hold_ms: f64,
    pub processing_compressor_release_ms: f64,
    pub processing_compressor_strong_release_ms: f64,
    pub processing_compressor_knee_db: f64,
    pub processing_pause_hold_ms: f64,
    pub processing_pause_return_delay_ms: f64,
    pub processing_loudness_output_max_correction_db: f64,
    pub processing_loudness_output_gain_up_db_per_second: f64,
    pub processing_loudness_output_gain_down_db_per_second: f64,

    pub processing_compressor_threshold_dbfs: f64,
    pub processing_pause_threshold_dbfs: f64,
    pub processing_loudness_enable: bool,
    pub processing_loudness_target_lufs: f64,
    pub processing_loudness_dead_band_lu: f64,
    pub processing_loudness_max_gain_db: f64,
    pub processing_loudness_max_attenuation_db: f64,
    pub processing_loudness_gain_up_db_per_second: f64,
    pub processing_loudness_gain_down_db_per_second: f64,
    pub processing_loudness_silence_gate_lufs: f64,
    pub processing_loudness_true_peak_ceiling_dbtp: f64,
    #[serde(default)]
    pub processing_vtt_enable: bool,
    #[serde(default)]
    pub processing_vtt_dummy: Option<String>,
    #[serde(default = "default_vtt_name")]
    pub processing_vtt_name: String,
    #[serde(default = "default_vtt_language")]
    pub processing_vtt_language: String,
    #[serde(default)]
    pub processing_vtt_default: bool,
    #[serde(default)]
    pub processing_guide_logo: String,
    #[serde(default)]
    pub processing_logo_key: String,
    #[serde(default = "default_logo_transition")]
    pub processing_logo_transition_in: String,
    #[serde(default = "default_logo_transition")]
    pub processing_logo_transition_out: String,
    #[serde(default)]
    pub processing_logo_transition_seconds: Option<f64>,

    pub playlist_day_start: String,
    pub playlist_length: String,
    pub playlist_infinit: bool,

    pub storage_filler: String,
    pub storage_extensions: String,
    pub storage_shuffle: bool,

    pub task_enable: bool,
    pub task_path: String,

    pub output_id: i32,
}

impl Configuration {
    pub fn from(id: i32, channel_id: i32, config: PlayoutConfig) -> Self {
        Self {
            id,
            channel_id,
            general_stop_threshold: config.general.stop_threshold,
            mail_subject: config.mail.subject,
            mail_recipient: config.mail.recipient,
            mail_level: config.mail.mail_level.to_string(),
            mail_interval: config.mail.interval,
            notification_topic: config.notification.topic,
            notification_level: config.notification.level.to_string(),
            notification_tags: config.notification.tags,
            logging_ffmpeg_level: config.logging.ffmpeg_level,
            logging_ingest_level: config.logging.ingest_level,
            logging_detect_silence: config.logging.detect_silence,
            logging_ignore: config.logging.ignore_lines.join(";"),
            processing_mode: config.processing.mode.to_string(),
            processing_add_logo: config.processing.add_logo,
            processing_logo: config.processing.logo,
            processing_logo_scale: config.processing.logo_scale,
            processing_logo_opacity: config.processing.logo_opacity,
            processing_logo_position: config.processing.logo_position,
            processing_volume: config.audio.volume,
            processing_loudness_scope: config.audio.loudness_scope.clone(),
            processing_compressor_ratio: config.audio.compressor_ratio,
            processing_compressor_attack_ms: config.audio.compressor_attack_ms,
            processing_compressor_hold_ms: config.audio.compressor_hold_ms,
            processing_compressor_release_ms: config.audio.compressor_release_ms,
            processing_compressor_strong_release_ms: config.audio.compressor_strong_release_ms,
            processing_compressor_knee_db: config.audio.compressor_knee_db,
            processing_pause_hold_ms: config.audio.pause_hold_ms,
            processing_pause_return_delay_ms: config.audio.pause_return_delay_ms,
            processing_loudness_output_max_correction_db: config
                .audio
                .loudness_output_max_correction_db,
            processing_loudness_output_gain_up_db_per_second: config
                .audio
                .loudness_output_gain_up_db_per_second,
            processing_loudness_output_gain_down_db_per_second: config
                .audio
                .loudness_output_gain_down_db_per_second,

            processing_compressor_threshold_dbfs: config.audio.compressor_threshold_dbfs,
            processing_pause_threshold_dbfs: config.audio.pause_threshold_dbfs,
            processing_loudness_enable: config.audio.loudness_scope != "off",
            processing_loudness_target_lufs: config.audio.loudness_target_lufs,
            processing_loudness_dead_band_lu: config.audio.loudness_dead_band_lu,
            processing_loudness_max_gain_db: config.audio.loudness_max_gain_db,
            processing_loudness_max_attenuation_db: config.audio.loudness_max_attenuation_db,
            processing_loudness_gain_up_db_per_second: config.audio.loudness_gain_up_db_per_second,
            processing_loudness_gain_down_db_per_second: config
                .audio
                .loudness_gain_down_db_per_second,
            processing_loudness_silence_gate_lufs: config.audio.loudness_silence_gate_lufs,
            processing_loudness_true_peak_ceiling_dbtp: config
                .audio
                .loudness_true_peak_ceiling_dbtp,
            processing_vtt_enable: config.processing.vtt_enable,
            processing_vtt_dummy: config.processing.vtt_dummy,
            processing_vtt_name: config.processing.vtt_name,
            processing_vtt_language: config.processing.vtt_language,
            processing_vtt_default: config.processing.vtt_default,
            processing_guide_logo: config.processing.guide_logo,
            processing_logo_key: config.processing.logo_key,
            processing_logo_transition_in: config.processing.logo_transition_in.to_string(),
            processing_logo_transition_out: config.processing.logo_transition_out.to_string(),
            processing_logo_transition_seconds: config.processing.logo_transition_seconds,
            playlist_day_start: config.playlist.day_start,
            playlist_length: config.playlist.length,
            playlist_infinit: config.playlist.infinit,
            storage_filler: config.storage.filler,
            storage_extensions: config.storage.extensions.join(";"),
            storage_shuffle: config.storage.shuffle,
            task_enable: config.task.enable,
            task_path: config.task.path.to_string_lossy().to_string(),
            output_id: config.output.id,
        }
    }
}

#[derive(Clone, Default, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Recording {
    pub id: i32,
    pub channel_id: i32,
    pub enabled: bool,
    pub source: String,
    pub source_output_id: Option<i32>,
    pub hls_variant: String,
    pub path: String,
    pub segment_duration: i64,
    pub retention_days: i64,
    pub minimum_free_space_gb: i64,
    pub width: i64,
    pub height: i64,
    pub video_codec: String,
    pub video_options: String,
    pub audio_codec: String,
    pub audio_options: String,
    pub audio_bitrate: i64,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Scte35 {
    pub config_id: i32,
    pub enabled: bool,
    pub pid: i64,
    pub preroll: f64,
    pub min_break: f64,
    pub max_break: f64,
    pub api_key: String,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct Output {
    pub id: i32,
    pub channel_id: i32,
    pub name: String,
    pub hls_variants: String,
    pub stream_url: String,
    pub stream_type: Option<String>,
    pub stream_format: Option<String>,
    pub hls_playlist_name: Option<String>,
    pub hls_segment_duration: Option<i64>,
    pub hls_list_size: Option<i64>,
    #[sqlx(default)]
    #[serde(default)]
    pub desktop_fullscreen: bool,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub video_codec: Option<String>,
    pub video_options: String,
    pub protocol_options: String,
    pub muxer_options: String,
    pub metadata_options: String,
    pub audio_codec: Option<String>,
    pub audio_options: String,
    pub audio_bitrate: Option<i64>,
}

impl Output {
    pub fn new(channel_id: i32, mode: OutputMode) -> Self {
        let stream_url = if mode == OutputMode::Stream {
            "rtmp://127.0.0.1/live/stream".to_string()
        } else {
            Default::default()
        };
        let hls_playlist_name = (mode == OutputMode::HLS).then(|| "stream".to_string());
        let hls_segment_duration = (mode == OutputMode::HLS).then_some(6);
        let hls_list_size = (mode == OutputMode::HLS).then_some(600);
        let encoded = matches!(mode, OutputMode::HLS | OutputMode::Stream);

        Self {
            id: 0,
            channel_id,
            name: mode.to_string(),
            hls_variants: String::new(),
            stream_url,
            stream_type: (mode == OutputMode::Stream).then(|| "rtmp".to_string()),
            stream_format: None,
            hls_playlist_name,
            hls_segment_duration,
            hls_list_size,
            desktop_fullscreen: false,
            width: 1280,
            height: 720,
            fps: 25.0,
            video_codec: encoded.then(|| "libx264".to_string()),
            video_options: if encoded {
                "{\"preset\":\"faster\",\"rate_control\":\"crf\",\"quality\":\"23\",\"maxrate\":\"2400\"}".to_string()
            } else {
                Default::default()
            },
            protocol_options: "{}".to_string(),
            muxer_options: "{}".to_string(),
            metadata_options: "{}".to_string(),
            audio_codec: encoded.then(|| "aac".to_string()),
            audio_options: "{}".to_string(),
            audio_bitrate: encoded.then_some(128),
        }
    }
}

fn default_vtt_name() -> String {
    "Subtitles".to_string()
}

fn default_vtt_language() -> String {
    "und".to_string()
}

#[derive(Debug, sqlx::FromRow)]
pub struct LiveInputRecord {
    pub id: i32,
    pub priority: i32,
    pub enabled: bool,
    pub name: String,
    pub backend: String,
    pub identifier: String,
    pub options: String,
    pub demuxer_options: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SetupSettings {
    pub logs: String,
    pub playlists: String,
    pub public: String,
    pub storage: String,
    pub shared: bool,
    pub smtp_server: String,
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_starttls: bool,
    pub smtp_port: u16,
}

impl From<GlobalSettings> for SetupSettings {
    fn from(settings: GlobalSettings) -> Self {
        Self {
            logs: settings.logs,
            playlists: settings.playlists,
            public: settings.public,
            storage: settings.storage,
            shared: settings.shared,
            smtp_server: settings.smtp_server,
            smtp_user: settings.smtp_user,
            smtp_password: String::new(),
            smtp_starttls: settings.smtp_starttls,
            smtp_port: settings.smtp_port,
        }
    }
}

#[derive(Debug)]
pub struct InitialSetup {
    pub settings: SetupSettings,
    pub channel_public: String,
    pub channel_playlists: String,
    pub channel_storage: String,
    pub username: String,
    pub mail: String,
    pub password_hash: String,
    pub two_factor: bool,
}

#[derive(Debug)]
pub(crate) struct SetupRollback {
    pub global: GlobalSettings,
    pub channel: Channel,
    pub active_outputs: Vec<i32>,
    pub user_id: i32,
}

/// What a scheduled event plays.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleKind {
    /// One or more clips from the storage.
    #[default]
    Media,
    /// A live source read from a `.live` file in the storage.
    Live,
}

/// How a scheduled event takes over the running programme.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryMode {
    /// Wait until the clip on air ends.
    #[default]
    AfterClip,
    /// Cut the clip on air at the exact time.
    Interrupt,
}

/// Where the programme continues after a scheduled event.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterMode {
    /// Continue where it stopped; the rest of the day runs later.
    #[default]
    Continue,
    /// Jump to what the playlist has at the current time.
    Clock,
}

macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl FromStr for $name {
            type Err = String;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $($text => Ok(Self::$variant),)+
                    other => Err(format!("unknown {} '{other}'", stringify!($name))),
                }
            }
        }
    };
}

text_enum!(ScheduleKind { Media => "media", Live => "live" });
text_enum!(EntryMode { AfterClip => "after_clip", Interrupt => "interrupt" });
text_enum!(AfterMode { Continue => "continue", Clock => "clock" });

/// A row of `schedule_events`. Times are in the channel time zone.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScheduleEvent {
    pub id: i32,
    pub channel_id: i32,
    pub kind: ScheduleKind,
    pub title: String,
    /// Storage paths: the clips to play, or the one `.live` file.
    pub sources: Vec<String>,
    /// Total clip length in seconds; zero for live events.
    pub duration: f64,
    /// `YYYY-MM-DD`
    pub start_date: String,
    /// `HH:MM:SS`
    pub start_time: String,
    /// `HH:MM:SS`, live events only.
    pub end_time: Option<String>,
    /// Weekday bits, Sunday = 1 ... Saturday = 64; zero plays once.
    pub repeat_days: u8,
    pub entry_mode: EntryMode,
    pub after_mode: AfterMode,
    pub show_in_guide: bool,
    pub retry_seconds: u32,
    pub enabled: bool,
}

impl FromRow<'_, SqliteRow> for ScheduleEvent {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let text_enum = |column: &str| -> sqlx::Result<String> { row.try_get(column) };
        let decode_error = |column: &str, error: String| sqlx::Error::ColumnDecode {
            index: column.to_string(),
            source: error.into(),
        };
        let sources: String = row.try_get("sources")?;

        Ok(Self {
            id: row.try_get("id")?,
            channel_id: row.try_get("channel_id")?,
            kind: text_enum("kind")?
                .parse()
                .map_err(|e| decode_error("kind", e))?,
            title: row.try_get("title")?,
            sources: serde_json::from_str(&sources)
                .map_err(|e| decode_error("sources", e.to_string()))?,
            duration: row.try_get("duration")?,
            start_date: row.try_get("start_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            repeat_days: row.try_get::<i64, _>("repeat_days")?.clamp(0, 127) as u8,
            entry_mode: text_enum("entry_mode")?
                .parse()
                .map_err(|e| decode_error("entry_mode", e))?,
            after_mode: text_enum("after_mode")?
                .parse()
                .map_err(|e| decode_error("after_mode", e))?,
            show_in_guide: row.try_get("show_in_guide")?,
            retry_seconds: row.try_get::<i64, _>("retry_seconds")?.max(0) as u32,
            enabled: row.try_get("enabled")?,
        })
    }
}

/// Outcome of one occurrence of a scheduled event, from `schedule_runs`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ScheduleRun {
    pub event_id: i32,
    /// `YYYY-MM-DD` of the occurrence.
    pub occurrence: String,
    /// `running`, `done`, `skipped` or `failed`.
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

/// An occurrence claimed to play that did not finish: it waited for its
/// start or was on air when the playout stopped.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct UnfinishedRun {
    pub event_id: i32,
    /// `YYYY-MM-DD` of the occurrence.
    pub occurrence: String,
    /// Local time it went on air, `YYYY-MM-DD HH:MM:SS%.3f`; `None` while it
    /// had not started.
    pub on_air_at: Option<String>,
    /// The programme's time shift when it went on air.
    pub shift_before: Option<f64>,
}

/// How NoAr looks for new versions of itself and installs them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct UpdateSettings {
    /// Look for a newer image every six hours.
    pub auto_check: bool,
    /// Install a newer image at `update_time`, through the updater.
    pub auto_update: bool,
    /// Server time, `HH:MM`.
    pub update_time: String,
    /// A one-off install, Unix milliseconds.
    pub scheduled_at: Option<i64>,
}

/// One item that went on air, from `as_run`. Times are Unix milliseconds.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct AsRunEntry {
    pub id: i64,
    pub started_at: i64,
    /// `None` while the item is on air.
    pub ended_at: Option<i64>,
    pub source: String,
    pub title: Option<String>,
    /// `program`, `commercial`, `live`, `filler` or `ingest`.
    pub kind: String,
    /// `playlist`, `folder`, `schedule` or `ingest`.
    pub origin: String,
    pub event_id: Option<i64>,
    pub event_title: Option<String>,
    pub scheduled_at: Option<i64>,
    pub seek: f64,
    pub planned_seconds: f64,
    pub replaces: Option<String>,
    /// `on_air`, `completed`, `cut`, `failed` or `stopped`.
    pub status: String,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

/// An item going on air, as the playout records it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewAsRunEntry {
    pub source: String,
    pub title: Option<String>,
    pub kind: &'static str,
    pub origin: &'static str,
    pub event_id: Option<i32>,
    pub event_title: Option<String>,
    pub scheduled_at: Option<i64>,
    pub seek: f64,
    pub planned_seconds: f64,
    pub replaces: Option<String>,
}

fn default_logo_transition() -> String {
    "fade".to_string()
}
