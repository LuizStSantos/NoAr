//! Recording settings shared by encoded and desktop outputs.

use super::{AudioOptions, VideoOptions};

/// Settings for a segmented Matroska recording derived from an encoded output.
/// `video_stream_index` selects the HLS rendition to copy; ordinary stream
/// outputs always use index zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingConfig {
    pub path: String,
    pub segment_duration: u32,
    pub retention_days: u32,
    pub minimum_free_space_gb: u32,
    pub video_stream_index: usize,
    pub encode: Option<RecordingEncodeConfig>,
    pub channel_id: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingEncodeConfig {
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub video_options: VideoOptions,
    pub audio_codec: String,
    pub audio_options: AudioOptions,
    pub audio_bitrate: u64,
}

impl RecordingConfig {
    pub fn new(path: impl Into<String>, segment_duration: u32) -> Self {
        Self {
            path: path.into(),
            segment_duration,
            retention_days: 0,
            minimum_free_space_gb: 0,
            video_stream_index: 0,
            encode: None,
            channel_id: None,
        }
    }

    pub fn with_video_stream_index(mut self, video_stream_index: usize) -> Self {
        self.video_stream_index = video_stream_index;
        self
    }

    pub fn with_retention_days(mut self, retention_days: u32) -> Self {
        self.retention_days = retention_days;
        self
    }

    pub fn with_minimum_free_space_gb(mut self, minimum_free_space_gb: u32) -> Self {
        self.minimum_free_space_gb = minimum_free_space_gb;
        self
    }

    pub fn with_encode(mut self, encode: RecordingEncodeConfig) -> Self {
        self.encode = Some(encode);
        self
    }

    pub fn with_channel_id(mut self, channel_id: Option<i32>) -> Self {
        self.channel_id = channel_id;
        self
    }
}
