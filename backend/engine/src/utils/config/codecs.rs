//! Encoder option catalogs, defaults and native capability validation.

use std::collections::BTreeMap;

pub type VideoOptions = BTreeMap<String, String>;
pub type AudioOptions = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoOptionKind {
    Select,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoOptionChoice {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoOptionVisibility {
    pub key: &'static str,
    pub value: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoOptionSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: VideoOptionKind,
    pub default: &'static str,
    pub choices: &'static [VideoOptionChoice],
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub visible_when: Option<VideoOptionVisibility>,
}

const X264_PRESETS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "ultrafast",
        label: "ultrafast",
    },
    VideoOptionChoice {
        value: "superfast",
        label: "superfast",
    },
    VideoOptionChoice {
        value: "veryfast",
        label: "veryfast",
    },
    VideoOptionChoice {
        value: "faster",
        label: "faster",
    },
    VideoOptionChoice {
        value: "fast",
        label: "fast",
    },
    VideoOptionChoice {
        value: "medium",
        label: "medium",
    },
    VideoOptionChoice {
        value: "slow",
        label: "slow",
    },
    VideoOptionChoice {
        value: "slower",
        label: "slower",
    },
    VideoOptionChoice {
        value: "veryslow",
        label: "veryslow",
    },
    VideoOptionChoice {
        value: "placebo",
        label: "placebo",
    },
];
const NVENC_PRESETS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "p1",
        label: "P1 (fastest)",
    },
    VideoOptionChoice {
        value: "p2",
        label: "P2",
    },
    VideoOptionChoice {
        value: "p3",
        label: "P3",
    },
    VideoOptionChoice {
        value: "p4",
        label: "P4 (default)",
    },
    VideoOptionChoice {
        value: "p5",
        label: "P5",
    },
    VideoOptionChoice {
        value: "p6",
        label: "P6",
    },
    VideoOptionChoice {
        value: "p7",
        label: "P7 (best quality)",
    },
];
const QSV_PRESETS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "veryfast",
        label: "veryfast",
    },
    VideoOptionChoice {
        value: "faster",
        label: "faster",
    },
    VideoOptionChoice {
        value: "fast",
        label: "fast",
    },
    VideoOptionChoice {
        value: "medium",
        label: "medium",
    },
    VideoOptionChoice {
        value: "slow",
        label: "slow",
    },
    VideoOptionChoice {
        value: "slower",
        label: "slower",
    },
    VideoOptionChoice {
        value: "veryslow",
        label: "veryslow",
    },
];
const VP9_DEADLINES: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "good",
        label: "Good quality",
    },
    VideoOptionChoice {
        value: "realtime",
        label: "Realtime",
    },
];
const ROW_MT: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "auto",
        label: "Automatic",
    },
    VideoOptionChoice {
        value: "1",
        label: "Enabled",
    },
    VideoOptionChoice {
        value: "0",
        label: "Disabled",
    },
];
const X264_RATE_CONTROLS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "crf",
        label: "CRF",
    },
    VideoOptionChoice {
        value: "cbr",
        label: "CBR",
    },
];
const VBR_CBR: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "vbr",
        label: "VBR",
    },
    VideoOptionChoice {
        value: "cbr",
        label: "CBR",
    },
];
const QSV_RATE_CONTROLS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "vbr",
        label: "VBR",
    },
    VideoOptionChoice {
        value: "cbr",
        label: "CBR",
    },
    VideoOptionChoice {
        value: "icq",
        label: "ICQ",
    },
];
const VAAPI_RATE_CONTROLS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "vbr",
        label: "VBR",
    },
    VideoOptionChoice {
        value: "cbr",
        label: "CBR",
    },
    VideoOptionChoice {
        value: "cqp",
        label: "CQP",
    },
];

const MAXRATE: VideoOptionSpec = VideoOptionSpec {
    key: "maxrate",
    label: "Maximum bitrate (kbit/s)",
    kind: VideoOptionKind::Number,
    default: "2400",
    choices: &[],
    minimum: Some(1.0),
    maximum: None,
    visible_when: None,
};
const X264_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "preset",
        label: "Preset",
        kind: VideoOptionKind::Select,
        default: "faster",
        choices: X264_PRESETS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "rate_control",
        label: "Rate control",
        kind: VideoOptionKind::Select,
        default: "crf",
        choices: X264_RATE_CONTROLS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "quality",
        label: "Quality",
        kind: VideoOptionKind::Number,
        default: "23",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(51.0),
        visible_when: Some(VideoOptionVisibility {
            key: "rate_control",
            value: "crf",
        }),
    },
    MAXRATE,
];
const NVENC_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "preset",
        label: "Preset",
        kind: VideoOptionKind::Select,
        default: "p4",
        choices: NVENC_PRESETS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "rate_control",
        label: "Rate control",
        kind: VideoOptionKind::Select,
        default: "vbr",
        choices: VBR_CBR,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "quality",
        label: "Constant quality",
        kind: VideoOptionKind::Number,
        default: "23",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(51.0),
        visible_when: Some(VideoOptionVisibility {
            key: "rate_control",
            value: "vbr",
        }),
    },
    MAXRATE,
];
const QSV_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "preset",
        label: "Preset",
        kind: VideoOptionKind::Select,
        default: "faster",
        choices: QSV_PRESETS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "rate_control",
        label: "Rate control",
        kind: VideoOptionKind::Select,
        default: "vbr",
        choices: QSV_RATE_CONTROLS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "global_quality",
        label: "Global quality",
        kind: VideoOptionKind::Number,
        default: "23",
        choices: &[],
        minimum: Some(1.0),
        maximum: Some(51.0),
        visible_when: Some(VideoOptionVisibility {
            key: "rate_control",
            value: "icq",
        }),
    },
    MAXRATE,
];
const VAAPI_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "rate_control",
        label: "Rate control",
        kind: VideoOptionKind::Select,
        default: "vbr",
        choices: VAAPI_RATE_CONTROLS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "quality",
        label: "Constant QP",
        kind: VideoOptionKind::Number,
        default: "23",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(52.0),
        visible_when: Some(VideoOptionVisibility {
            key: "rate_control",
            value: "cqp",
        }),
    },
    VideoOptionSpec {
        key: "maxrate",
        label: "Maximum bitrate (kbit/s)",
        kind: VideoOptionKind::Number,
        default: "2400",
        choices: &[],
        minimum: Some(1.0),
        maximum: None,
        // CBR and VBR use this value. It remains visible for CQP as well so
        // switching rate-control modes does not discard the user's value.
        visible_when: None,
    },
];
const VP9_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "rate_control",
        label: "Rate control",
        kind: VideoOptionKind::Select,
        default: "crf",
        choices: X264_RATE_CONTROLS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "quality",
        label: "Quality",
        kind: VideoOptionKind::Number,
        default: "31",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(63.0),
        visible_when: Some(VideoOptionVisibility {
            key: "rate_control",
            value: "crf",
        }),
    },
    VideoOptionSpec {
        key: "deadline",
        label: "Encoding mode",
        kind: VideoOptionKind::Select,
        default: "good",
        choices: VP9_DEADLINES,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "cpu-used",
        label: "Speed",
        kind: VideoOptionKind::Number,
        default: "4",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(8.0),
        visible_when: None,
    },
    VideoOptionSpec {
        key: "row-mt",
        label: "Row multithreading",
        kind: VideoOptionKind::Select,
        default: "auto",
        choices: ROW_MT,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    MAXRATE,
];
const SVT_AV1_PRESETS: &[VideoOptionChoice] = &[
    VideoOptionChoice {
        value: "0",
        label: "0 (slowest)",
    },
    VideoOptionChoice {
        value: "1",
        label: "1",
    },
    VideoOptionChoice {
        value: "2",
        label: "2",
    },
    VideoOptionChoice {
        value: "3",
        label: "3",
    },
    VideoOptionChoice {
        value: "4",
        label: "4",
    },
    VideoOptionChoice {
        value: "5",
        label: "5",
    },
    VideoOptionChoice {
        value: "6",
        label: "6",
    },
    VideoOptionChoice {
        value: "7",
        label: "7",
    },
    VideoOptionChoice {
        value: "8",
        label: "8 (default)",
    },
    VideoOptionChoice {
        value: "9",
        label: "9",
    },
    VideoOptionChoice {
        value: "10",
        label: "10",
    },
    VideoOptionChoice {
        value: "11",
        label: "11",
    },
    VideoOptionChoice {
        value: "12",
        label: "12",
    },
    VideoOptionChoice {
        value: "13",
        label: "13 (fastest)",
    },
];
const SVT_AV1_SETTINGS: &[VideoOptionSpec] = &[
    VideoOptionSpec {
        key: "preset",
        label: "Preset",
        kind: VideoOptionKind::Select,
        default: "8",
        choices: SVT_AV1_PRESETS,
        minimum: None,
        maximum: None,
        visible_when: None,
    },
    VideoOptionSpec {
        key: "quality",
        label: "Quality",
        kind: VideoOptionKind::Number,
        default: "30",
        choices: &[],
        minimum: Some(0.0),
        maximum: Some(63.0),
        visible_when: None,
    },
    MAXRATE,
];
const UNCOMPRESSED_VIDEO_SETTINGS: &[VideoOptionSpec] = &[];
const GENERIC_SETTINGS: &[VideoOptionSpec] = &[MAXRATE];

pub fn video_option_specs(codec: &str) -> &'static [VideoOptionSpec] {
    if !video_codec_uses_bitrate(codec) {
        UNCOMPRESSED_VIDEO_SETTINGS
    } else if codec.contains("x264") || codec.contains("x265") {
        X264_SETTINGS
    } else if codec.ends_with("_nvenc") {
        NVENC_SETTINGS
    } else if codec.ends_with("_qsv") {
        QSV_SETTINGS
    } else if codec.ends_with("_vaapi") {
        VAAPI_SETTINGS
    } else if codec == "libvpx-vp9" {
        VP9_SETTINGS
    } else if codec == "libsvtav1" {
        SVT_AV1_SETTINGS
    } else {
        GENERIC_SETTINGS
    }
}

pub fn video_codec_uses_bitrate(codec: &str) -> bool {
    !matches!(
        codec,
        "rawvideo" | "v210" | "r210" | "v308" | "v408" | "v410"
    )
}

pub fn audio_codec_uses_bitrate(codec: &str) -> bool {
    !codec.starts_with("pcm_") && !matches!(codec, "alac" | "flac" | "truehd")
}

/// Output audio sample rates an operator can select. The engine processes at
/// 48 kHz; these are produced by resampling just before the encoder.
pub const OUTPUT_AUDIO_SAMPLE_RATES: &[u32] = &[32_000, 44_100, 48_000];

const MANAGED_AUDIO_OPTIONS: &[&str] = &[
    "ab",
    "ac",
    "ar",
    "b",
    "channel_layout",
    "ch_layout",
    "flags",
    "request_sample_fmt",
    "sample_fmt",
    "time_base",
];

pub fn validate_audio_options(
    codec_name: &str,
    options: &AudioOptions,
    sample_rate: u32,
    bit_rate: u64,
) -> Result<(), String> {
    use ::gstreamer::{self as gst, prelude::*};

    crate::gstreamer::init().map_err(|error| error.to_string())?;
    if sample_rate == 0 || sample_rate > i32::MAX as u32 {
        return Err("audio sample rate must be positive and fit a native rate".to_string());
    }
    if audio_codec_uses_bitrate(codec_name) && bit_rate == 0 {
        return Err("audio bitrate must be positive".to_string());
    }
    for (key, value) in options {
        if key.trim() != key || key.is_empty() || value.trim().is_empty() {
            return Err("audio option names and values must not be empty".to_string());
        }
        if MANAGED_AUDIO_OPTIONS.contains(&key.as_str()) {
            return Err(format!(
                "audio option {key:?} is managed by NoAr and cannot be overridden"
            ));
        }
    }
    let encoder =
        crate::output::pipeline::audio_encoder(codec_name, options, bit_rate).map_err(|error| {
            format!("unsupported audio option or invalid native audio encoder options: {error:#}")
        })?;
    let required = gst::Caps::builder("audio/x-raw")
        .field("rate", sample_rate as i32)
        .field("channels", 2_i32)
        .build();
    let sink = encoder
        .static_pad("sink")
        .ok_or("audio encoder has no sink pad")?;
    if !sink.query_caps(None).can_intersect(&required) {
        return Err(format!(
            "audio encoder {codec_name:?} does not support stereo at {sample_rate} Hz"
        ));
    }

    Ok(())
}

pub fn video_option_defaults(codec: &str) -> VideoOptions {
    video_option_specs(codec)
        .iter()
        .map(|setting| (setting.key.to_string(), setting.default.to_string()))
        .collect()
}

pub fn validate_video_options(codec: &str, options: &VideoOptions) -> Result<(), String> {
    let specs = video_option_specs(codec);

    for (key, value) in options {
        let Some(spec) = specs.iter().find(|spec| spec.key == key) else {
            return Err(format!(
                "unsupported video option {key:?} for codec {codec:?}"
            ));
        };

        if !video_option_is_visible(spec, options) {
            continue;
        }

        if spec.kind == VideoOptionKind::Select
            && !spec.choices.iter().any(|choice| choice.value == value)
        {
            return Err(format!(
                "unsupported value {value:?} for video option {key:?}"
            ));
        }

        if spec.kind == VideoOptionKind::Number {
            let number = value
                .parse::<f64>()
                .map_err(|_| format!("video option {key:?} must be a number"))?;
            if !number.is_finite()
                || spec.minimum.is_some_and(|minimum| number < minimum)
                || spec.maximum.is_some_and(|maximum| number > maximum)
            {
                return Err(format!("video option {key:?} is outside its allowed range"));
            }
        }
    }

    for spec in specs {
        if video_option_is_visible(spec, options) && !options.contains_key(spec.key) {
            return Err(format!(
                "missing video option {:?} for codec {codec:?}",
                spec.key
            ));
        }
    }

    Ok(())
}

fn video_option_is_visible(spec: &VideoOptionSpec, options: &VideoOptions) -> bool {
    spec.visible_when.is_none_or(|condition| {
        options
            .get(condition.key)
            .is_some_and(|value| value == condition.value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vp9_options_include_realtime_controls() {
        let options = video_option_defaults("libvpx-vp9");

        assert_eq!(options.get("rate_control").map(String::as_str), Some("crf"));
        assert_eq!(options.get("deadline").map(String::as_str), Some("good"));
        assert_eq!(options.get("cpu-used").map(String::as_str), Some("4"));
        assert_eq!(options.get("row-mt").map(String::as_str), Some("auto"));
        assert!(validate_video_options("libvpx-vp9", &options).is_ok());
    }

    #[test]
    fn svt_av1_options_include_preset_and_quality() {
        let options = video_option_defaults("libsvtav1");

        assert_eq!(options.get("preset").map(String::as_str), Some("8"));
        assert_eq!(options.get("quality").map(String::as_str), Some("30"));
        assert!(validate_video_options("libsvtav1", &options).is_ok());
    }

    #[test]
    fn uncompressed_codecs_do_not_use_bitrate_settings() {
        assert!(video_option_defaults("rawvideo").is_empty());
        assert!(!video_codec_uses_bitrate("rawvideo"));
        assert!(!audio_codec_uses_bitrate("pcm_s16le"));
        assert!(!audio_codec_uses_bitrate("flac"));
        assert!(audio_codec_uses_bitrate("aac"));
    }

    #[test]
    fn audio_validation_checks_native_rate_and_bitrate_even_without_options() {
        let options = Default::default();
        assert!(validate_audio_options("aac", &options, 48_000, 128_000).is_ok());
        assert!(validate_audio_options("aac", &options, 0, 128_000).is_err());
        assert!(validate_audio_options("aac", &options, 48_000, 0).is_err());
        assert!(validate_audio_options("missing_encoder", &options, 48_000, 128_000).is_err());
    }

    #[test]
    fn native_audio_rejects_a_rate_outside_encoder_caps() {
        assert!(validate_audio_options("libopus", &Default::default(), 44_100, 128_000).is_err());
        assert!(validate_audio_options("libopus", &Default::default(), 48_000, 128_000).is_ok());
    }

    #[test]
    fn accepts_valid_aac_audio_options() {
        let options = [
            ("aac_coder".to_string(), "fast".to_string()),
            ("cutoff".to_string(), "18000".to_string()),
        ]
        .into_iter()
        .collect();

        assert!(validate_audio_options("aac", &options, 48_000, 128_000).is_ok());
    }

    #[test]
    fn accepts_valid_libopus_audio_options_when_available() {
        crate::gstreamer::init().unwrap();
        // "libopus" is encoded by GStreamer's opusenc.
        if ::gstreamer::ElementFactory::find("opusenc").is_none() {
            return;
        }

        let options = [("application".to_string(), "lowdelay".to_string())]
            .into_iter()
            .collect();

        assert!(validate_audio_options("libopus", &options, 48_000, 128_000).is_ok());
    }

    #[test]
    fn rejects_audio_options_managed_by_noar() {
        let options = [("ar".to_string(), "44100".to_string())]
            .into_iter()
            .collect();

        assert!(
            validate_audio_options("aac", &options, 48_000, 128_000)
                .unwrap_err()
                .contains("managed by NoAr")
        );
    }

    #[test]
    fn rejects_unknown_and_invalid_audio_encoder_options() {
        assert!(
            validate_audio_options(
                "aac",
                &[("aac_codre".to_string(), "fast".to_string())]
                    .into_iter()
                    .collect(),
                48_000,
                128_000,
            )
            .unwrap_err()
            .contains("audio encoder options")
        );
        assert!(
            validate_audio_options(
                "aac",
                &[("aac_coder".to_string(), "not-a-coder".to_string())]
                    .into_iter()
                    .collect(),
                48_000,
                128_000,
            )
            .is_err()
        );
    }

    #[test]
    fn qsv_icq_uses_global_quality() {
        let mut options = video_option_defaults("h264_qsv");
        options.insert("rate_control".to_string(), "icq".to_string());

        assert_eq!(
            options.get("global_quality").map(String::as_str),
            Some("23")
        );
        assert!(validate_video_options("h264_qsv", &options).is_ok());
    }

    #[test]
    fn vaapi_cqp_uses_constant_qp() {
        let mut options = video_option_defaults("h264_vaapi");
        options.insert("rate_control".to_string(), "cqp".to_string());

        assert_eq!(options.get("quality").map(String::as_str), Some("23"));
        assert!(validate_video_options("h264_vaapi", &options).is_ok());
    }
}
