//! Native GStreamer capabilities. Public legacy names preserve the API contract.

use std::{collections::BTreeMap, sync::OnceLock};

use ::gstreamer::{self as gst, prelude::*};

use crate::output::pipeline;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCapabilities {
    pub features: MediaFeatureSet,
    pub encoders: Vec<MediaCodec>,
    pub muxers: Vec<MediaMuxer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFeatureSet {
    pub hls_subtitle_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaCodec {
    pub name: String,
    pub display_name: String,
    pub codec_id: String,
    pub media_type: MediaKind,
    pub hardware: bool,
    pub wrapper: Option<String>,
    factory: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaMuxer {
    pub name: String,
    pub long_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTarget {
    Hls,
    Rtmp,
    Srt,
    Udp,
    Matroska,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    Video,
    Subtitle,
}

static CAPABILITIES: OnceLock<MediaCapabilities> = OnceLock::new();

pub fn media_capabilities() -> &'static MediaCapabilities {
    CAPABILITIES.get_or_init(MediaCapabilities::detect)
}

impl MediaCapabilities {
    pub fn detect() -> Self {
        if let Err(error) = crate::gstreamer::init() {
            log::error!("GStreamer capabilities unavailable: {error:#}");
            return Self {
                features: MediaFeatureSet {
                    hls_subtitle_name: false,
                },
                encoders: Vec::new(),
                muxers: Vec::new(),
            };
        }
        let mut encoders = Vec::new();
        for factory in gst::ElementFactory::factories_with_type(
            gst::ElementFactoryType::ENCODER,
            gst::Rank::NONE,
        ) {
            let class = factory
                .metadata(gst::ELEMENT_METADATA_KLASS)
                .unwrap_or_default();
            let media_type = if class.contains("Video") || class.contains("Image") {
                MediaKind::Video
            } else if class.contains("Audio") {
                MediaKind::Audio
            } else {
                continue;
            };
            let raw = gst::Caps::builder(match media_type {
                MediaKind::Video => "video/x-raw",
                _ => "audio/x-raw",
            })
            .build();
            if !factory.static_pad_templates().iter().any(|template| {
                template.direction() == gst::PadDirection::Sink
                    && template.caps().can_intersect(&raw)
            }) {
                continue;
            }
            let caps = factory_caps(&factory, gst::PadDirection::Src);
            let Some(codec_id) = super::media_info::codec_name(Some(caps.as_ref())) else {
                continue;
            };
            let native_name = factory.name().to_string();
            let name = persisted_codec_name(&native_name).to_owned();
            encoders.push(MediaCodec {
                display_name: factory
                    .metadata(gst::ELEMENT_METADATA_LONGNAME)
                    .unwrap_or_default()
                    .to_string(),
                codec_id,
                media_type,
                hardware: class.contains("Hardware"),
                wrapper: factory.plugin_name().map(|name| name.to_string()),
                factory: native_name,
                name,
            });
        }
        // Raw PCM uses audioconvert and an identity handoff, without a codec.
        if gst::ElementFactory::find("identity").is_some() {
            for (name, display) in [
                ("pcm_s16le", "PCM signed 16-bit little-endian"),
                ("pcm_f32le", "PCM float 32-bit little-endian"),
            ] {
                encoders.push(MediaCodec {
                    name: name.into(),
                    display_name: display.into(),
                    codec_id: name.into(),
                    media_type: MediaKind::Audio,
                    hardware: false,
                    wrapper: None,
                    factory: "identity".into(),
                });
            }
        }
        encoders.sort_by(|left, right| left.name.cmp(&right.name));
        encoders.dedup_by(|left, right| left.name == right.name);
        let muxers = ["hls", "flv", "mpegts", "matroska", "mp4"]
            .into_iter()
            .filter_map(|name| {
                let factory = mux_factory(name)?;
                Some(MediaMuxer {
                    name: name.to_owned(),
                    long_name: factory
                        .metadata(gst::ELEMENT_METADATA_LONGNAME)
                        .unwrap_or_default()
                        .to_string(),
                })
            })
            .collect();

        Self {
            features: MediaFeatureSet {
                hls_subtitle_name: true,
            },
            encoders,
            muxers,
        }
    }

    pub fn audio_codecs_for(&self, target: OutputTarget) -> Vec<MediaCodec> {
        self.codecs_for(target, MediaKind::Audio)
    }

    pub fn video_codecs_for(&self, target: OutputTarget) -> Vec<MediaCodec> {
        self.codecs_for(target, MediaKind::Video)
    }

    pub fn codecs_for(&self, target: OutputTarget, media_type: MediaKind) -> Vec<MediaCodec> {
        let muxer = match target {
            OutputTarget::Hls | OutputTarget::Srt | OutputTarget::Udp => "mpegts",
            OutputTarget::Rtmp => "flv",
            OutputTarget::Matroska => "matroska",
        };
        let Some(muxer) = mux_factory(muxer) else {
            return Vec::new();
        };
        let sink_caps = factory_caps(&muxer, gst::PadDirection::Sink);

        self.encoders
            .iter()
            .filter(|codec| codec.media_type == media_type)
            .filter(|codec| {
                gst::ElementFactory::find(&codec.factory).is_some_and(|factory| {
                    let caps = match codec.codec_id.as_str() {
                        "pcm_s16le" => gst::Caps::builder("audio/x-raw")
                            .field("format", "S16LE")
                            .build(),
                        "pcm_f32le" => gst::Caps::builder("audio/x-raw")
                            .field("format", "F32LE")
                            .build(),
                        _ => factory_caps(&factory, gst::PadDirection::Src),
                    };
                    caps.can_intersect(&sink_caps)
                })
            })
            .filter(|codec| {
                target != OutputTarget::Rtmp
                    || match codec.media_type {
                        MediaKind::Video => {
                            matches!(codec.codec_id.as_str(), "h264" | "hevc" | "av1")
                        }
                        MediaKind::Audio => matches!(codec.codec_id.as_str(), "aac" | "mp3"),
                        MediaKind::Subtitle => false,
                    }
            })
            .cloned()
            .collect()
    }

    pub fn usable_codecs(&self, media_type: MediaKind) -> Vec<MediaCodec> {
        self.encoders
            .iter()
            .filter(|codec| codec.media_type == media_type)
            .cloned()
            .collect()
    }

    pub fn has_muxer(&self, target: OutputTarget) -> bool {
        self.has_muxer_named(match target {
            OutputTarget::Hls => "hls",
            OutputTarget::Rtmp => "flv",
            OutputTarget::Srt | OutputTarget::Udp => "mpegts",
            OutputTarget::Matroska => "matroska",
        })
    }

    pub fn has_muxer_named(&self, name: &str) -> bool {
        mux_factory(name).is_some()
    }
}

fn persisted_codec_name(factory: &str) -> &str {
    match factory {
        "x264enc" => "libx264",
        "x265enc" => "libx265",
        "vp8enc" => "libvpx",
        "vp9enc" => "libvpx-vp9",
        "svtav1enc" => "libsvtav1",
        "avenc_aac" => "aac",
        "lamemp3enc" => "libmp3lame",
        "opusenc" => "libopus",
        "vorbisenc" => "libvorbis",
        "fdkaacenc" => "libfdk_aac",
        "avenc_ac3" => "ac3",
        "twolamemp2enc" => "mp2",
        _ => factory,
    }
}

fn factory_caps(factory: &gst::ElementFactory, direction: gst::PadDirection) -> gst::Caps {
    let mut caps = gst::Caps::new_empty();
    for template in factory.static_pad_templates() {
        if template.direction() == direction {
            caps.make_mut().append(template.caps());
        }
    }
    caps
}

fn mux_factory(name: &str) -> Option<gst::ElementFactory> {
    if name == "hls" {
        gst::ElementFactory::find("splitmuxsink")?;
        return gst::ElementFactory::find("mpegtsmux");
    }
    gst::ElementFactory::find(pipeline::muxer_factory(name).ok()?)
}

/// Validate exactly the properties used by the native output builder.
pub fn validate_muxer_options(
    name: &str,
    options: &BTreeMap<String, String>,
) -> Result<(), String> {
    crate::gstreamer::init().map_err(|error| error.to_string())?;
    if name == "hls" {
        for (key, value) in options {
            if key != "hls_flags"
                || value
                    .split('+')
                    .filter(|flag| !flag.is_empty())
                    .any(|flag| !matches!(flag, "independent_segments" | "program_date_time"))
            {
                return Err(format!("unsupported native HLS setting {key:?}"));
            }
        }
        return Ok(());
    }
    let factory = pipeline::muxer_factory(name).map_err(|error| error.to_string())?;
    let mux = pipeline::make(factory).map_err(|error| error.to_string())?;
    pipeline::configure_muxer(&mux, name, options).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_registry_advertises_encoders_usable_by_transport_muxers() {
        let capabilities = MediaCapabilities::detect();
        for target in [
            OutputTarget::Hls,
            OutputTarget::Srt,
            OutputTarget::Udp,
            OutputTarget::Rtmp,
            OutputTarget::Matroska,
        ] {
            assert!(capabilities.has_muxer(target));
            assert!(
                capabilities
                    .video_codecs_for(target)
                    .iter()
                    .any(|codec| codec.codec_id == "h264")
            );
            assert!(
                capabilities
                    .audio_codecs_for(target)
                    .iter()
                    .any(|codec| codec.codec_id == "aac")
            );
        }
        for codec in capabilities.encoders {
            assert!(gst::ElementFactory::find(&codec.factory).is_some());
        }
    }

    #[test]
    fn native_muxer_validation_rejects_unknown_and_managed_options() {
        assert!(
            validate_muxer_options(
                "hls",
                &BTreeMap::from([(
                    "hls_flags".into(),
                    "program_date_time+independent_segments".into()
                )])
            )
            .is_ok()
        );
        for (format, key, value) in [
            ("mpegts", "misspelled", "1"),
            ("hls", "hls_flags", "single_file"),
            ("hls", "hls_delete_threshold", "3"),
        ] {
            assert!(
                validate_muxer_options(format, &BTreeMap::from([(key.into(), value.into())]))
                    .is_err()
            );
        }
    }
}
