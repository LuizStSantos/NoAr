//! Recording DTOs and destination/encoder validation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{db::models, utils::paths::validate_directory_path};

use super::ENGINE_AUDIO_SAMPLE_RATE;

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "playout_config.d.ts")]
#[serde(rename_all = "snake_case")]
pub enum RecordingSource {
    HlsVariant,
    #[default]
    Stream,
    Encode,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, TS)]
#[ts(export, export_to = "playout_config.d.ts")]
pub struct Recording {
    pub enable: bool,
    pub source: RecordingSource,
    pub source_output_id: Option<i32>,
    pub variant: String,
    pub path: String,
    pub segment_duration: u32,
    pub retention_days: u32,
    pub minimum_free_space_gb: u32,
    pub width: u32,
    pub height: u32,
    pub video_codec: String,
    pub video_options: BTreeMap<String, String>,
    pub audio_codec: String,
    #[serde(default)]
    pub audio_options: BTreeMap<String, String>,
    pub audio_bitrate: u32,
}

impl Recording {
    pub(super) fn new(recording: &models::Recording) -> Self {
        Self {
            enable: recording.enabled,
            source: match recording.source.as_str() {
                "hls_variant" => RecordingSource::HlsVariant,
                "encode" => RecordingSource::Encode,
                _ => RecordingSource::Stream,
            },
            source_output_id: recording.source_output_id,
            variant: recording.hls_variant.clone(),
            path: recording.path.clone(),
            segment_duration: u32::try_from(recording.segment_duration).unwrap_or(300),
            retention_days: u32::try_from(recording.retention_days).unwrap_or_default(),
            minimum_free_space_gb: u32::try_from(recording.minimum_free_space_gb)
                .unwrap_or_default(),
            width: u32::try_from(recording.width).unwrap_or_default(),
            height: u32::try_from(recording.height).unwrap_or_default(),
            video_codec: recording.video_codec.clone(),
            video_options: serde_json::from_str(&recording.video_options)
                .unwrap_or_else(|_| noar_engine::video_option_defaults(&recording.video_codec)),
            audio_codec: recording.audio_codec.clone(),
            audio_options: serde_json::from_str(&recording.audio_options).unwrap_or_default(),
            audio_bitrate: u32::try_from(recording.audio_bitrate).unwrap_or(128),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.enable {
            return Ok(());
        }
        validate_directory_path("Recording", &self.path)?;
        if !(30..=3600).contains(&self.segment_duration) {
            return Err(
                "recording segment duration must be between 30 and 3600 seconds".to_string(),
            );
        }
        if self.source == RecordingSource::HlsVariant && self.variant.trim().is_empty() {
            return Err("recording HLS variant must not be empty".to_string());
        }
        if self.source != RecordingSource::Encode && self.source_output_id.is_none() {
            return Err("recording source output must be selected".to_string());
        }
        if self.source == RecordingSource::Encode {
            if (self.width == 0) != (self.height == 0) {
                return Err(
                    "recording width and height must both be set or both be zero".to_string(),
                );
            }
            let capabilities = noar_engine::media_capabilities();
            if !capabilities
                .video_codecs_for(noar_engine::OutputTarget::Matroska)
                .iter()
                .any(|codec| codec.name == self.video_codec)
            {
                return Err("unsupported recording video codec".to_string());
            }
            if !capabilities
                .audio_codecs_for(noar_engine::OutputTarget::Matroska)
                .iter()
                .any(|codec| codec.name == self.audio_codec && !codec.hardware)
            {
                return Err("unsupported recording audio codec".to_string());
            }
            noar_engine::validate_video_options(&self.video_codec, &self.video_options)?;
            noar_engine::validate_audio_options(
                &self.audio_codec,
                &self.audio_options,
                ENGINE_AUDIO_SAMPLE_RATE,
                u64::from(self.audio_bitrate) * 1_000,
            )?;
            if noar_engine::audio_codec_uses_bitrate(&self.audio_codec) && self.audio_bitrate == 0 {
                return Err("recording audio bitrate must be greater than zero".to_string());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod recording_tests {
    use std::collections::BTreeMap;

    use super::{Recording, RecordingSource};

    #[test]
    fn recording_requires_a_path_and_valid_segment_duration() {
        let mut recording = Recording {
            enable: true,
            path: String::new(),
            ..Default::default()
        };
        assert!(recording.validate().is_err());

        recording.path = "/var/lib/noar/recordings/1".to_string();
        recording.segment_duration = 29;
        assert!(recording.validate().is_err());

        recording.segment_duration = 300;
        recording.source_output_id = Some(1);
        assert!(recording.validate().is_ok());
    }

    #[test]
    fn recording_rejects_a_relative_or_system_path() {
        let mut recording = Recording {
            enable: true,
            path: "recordings".to_string(),
            segment_duration: 300,
            source_output_id: Some(1),
            ..Default::default()
        };
        assert!(recording.validate().is_err());

        recording.path = "/etc".to_string();
        assert!(recording.validate().is_err());
    }

    #[test]
    fn hls_variant_recording_requires_a_variant_name() {
        let recording = Recording {
            enable: true,
            source: RecordingSource::HlsVariant,
            source_output_id: Some(1),
            path: "/var/lib/noar/recordings/1".to_string(),
            segment_duration: 300,
            ..Recording::default()
        };
        assert!(recording.validate().is_err());
    }

    #[test]
    fn recording_audio_validation_uses_recording_bitrate() {
        let mut recording = Recording {
            enable: true,
            source: RecordingSource::Encode,
            path: "/var/lib/noar/recordings/1".to_string(),
            segment_duration: 300,
            video_codec: "libx264".to_string(),
            video_options: noar_engine::video_option_defaults("libx264"),
            audio_codec: "mp2".to_string(),
            audio_bitrate: 128,
            audio_options: BTreeMap::from([("cutoff".to_string(), "0".to_string())]),
            ..Recording::default()
        };
        assert!(recording.validate().is_ok());
        recording.audio_bitrate = 129;
        assert!(
            recording
                .validate()
                .unwrap_err()
                .contains("audio encoder options")
        );
    }

    #[test]
    fn recording_rejects_invalid_audio_encoder_options() {
        let recording = Recording {
            enable: true,
            source: RecordingSource::Encode,
            source_output_id: None,
            variant: String::new(),
            path: "/var/lib/noar/recordings/1".to_string(),
            segment_duration: 300,
            retention_days: 0,
            minimum_free_space_gb: 0,
            width: 0,
            height: 0,
            video_codec: "libx264".to_string(),
            video_options: noar_engine::video_option_defaults("libx264"),
            audio_codec: "aac".to_string(),
            audio_options: BTreeMap::from([("aac_codre".to_string(), "fast".to_string())]),
            audio_bitrate: 128,
        };

        assert!(
            recording
                .validate()
                .unwrap_err()
                .contains("unsupported audio option")
        );
    }
}
