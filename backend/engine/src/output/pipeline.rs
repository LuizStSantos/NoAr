//! Native GStreamer encoder construction and strict persisted-setting migration.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail, ensure};
use gstreamer::{self as gst, glib, prelude::*};

use crate::utils::config::{AudioOptions, VideoOptions};

pub(crate) fn make(factory: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(factory)
        .build()
        .with_context(|| format!("GStreamer element {factory:?} is not installed"))
}

/// Deserialize through the property's actual GType and reject values which
/// GObject would otherwise clamp or ignore with only a console warning.
pub(crate) fn set_property(element: &gst::Element, name: &str, text: &str) -> Result<()> {
    use glib::translate::{ToGlibPtr, ToGlibPtrMut};

    let property = element.find_property(name).ok_or_else(|| {
        anyhow!(
            "GStreamer element {} has no property {name:?}",
            element.name()
        )
    })?;
    ensure!(
        property.flags().contains(glib::ParamFlags::WRITABLE)
            && !property.flags().contains(glib::ParamFlags::CONSTRUCT_ONLY),
        "GStreamer property {name:?} is not writable"
    );
    let mut value = if property.value_type() == String::static_type() {
        text.to_value()
    } else {
        glib::Value::deserialize_with_pspec(text, &property)
            .with_context(|| format!("invalid GStreamer property {name}={text:?}"))?
    };
    // SAFETY: both owned GLib wrappers remain live; validation only mutates
    // this temporary value. A changed value means the original was invalid.
    let clamped = unsafe {
        glib::gobject_ffi::g_param_value_validate(
            property.to_glib_none().0,
            value.to_glib_none_mut().0,
        ) != 0
    };
    ensure!(
        !clamped,
        "GStreamer property {name}={text:?} is outside its valid range"
    );
    element.set_property_from_value(name, &value);

    Ok(())
}

fn factory(codec: &str, aliases: &[&str], media: &str) -> Result<String> {
    crate::gstreamer::init()?;

    for candidate in aliases.iter().copied().chain(std::iter::once(codec)) {
        if let Some(factory) = gst::ElementFactory::find(candidate) {
            let class = factory.metadata("klass").unwrap_or_default();

            if candidate == "identity" || (class.contains("Encoder") && class.contains(media)) {
                return Ok(candidate.to_string());
            }
        }
    }

    Err(anyhow!(
        "GStreamer {media} encoder for {codec:?} is not installed"
    ))
}

pub(crate) fn video_codec_factory(codec: &str) -> Result<String> {
    let aliases: &[&str] = match codec {
        "libx264" | "h264" => &["x264enc"],
        "libx265" | "hevc" => &["x265enc"],
        "h264_nvenc" => &["nvh264enc", "nvh264sldevice0enc"],
        "hevc_nvenc" => &["nvh265enc", "nvh265sldevice0enc"],
        "h264_qsv" => &["qsvh264enc"],
        "hevc_qsv" => &["qsvh265enc"],
        "h264_vaapi" => &["vah264enc", "vaapih264enc"],
        "hevc_vaapi" => &["vah265enc", "vaapih265enc"],
        "libvpx-vp9" => &["vp9enc"],
        "libvpx" => &["vp8enc"],
        "libsvtav1" => &["svtav1enc"],
        "libaom-av1" => &["av1enc"],
        "mpeg2video" => &["avenc_mpeg2video"],
        "mpeg4" => &["avenc_mpeg4"],
        "ffv1" => &["avenc_ffv1"],
        "rawvideo" => &["identity"],
        _ => &[],
    };

    factory(codec, aliases, "Video")
}

pub(crate) fn audio_codec_factory(codec: &str) -> Result<String> {
    let aliases: &[&str] = match codec {
        "aac" => &["avenc_aac", "voaacenc", "fdkaacenc"],
        "libfdk_aac" => &["fdkaacenc"],
        "libmp3lame" | "mp3" => &["lamemp3enc"],
        "libopus" | "opus" => &["opusenc"],
        "libvorbis" | "vorbis" => &["vorbisenc"],
        "flac" => &["flacenc"],
        "ac3" => &["avenc_ac3"],
        "mp2" => &["twolamemp2enc", "avenc_mp2"],
        "pcm_s16le" | "pcm_f32le" => &["identity"],
        _ => &[],
    };

    factory(codec, aliases, "Audio")
}

pub(crate) fn video_encoder(
    codec: &str,
    options: &VideoOptions,
    fps: u32,
    width: u32,
    height: u32,
) -> Result<gst::Element> {
    ensure!(
        fps > 0 && width > 0 && height > 0,
        "invalid encoder video dimensions/rate"
    );
    let factory = video_codec_factory(codec)?;
    let encoder = make(&factory)?;
    let x264 = factory == "x264enc";
    let x265 = factory == "x265enc";

    if x264 || x265 {
        set_property(&encoder, "tune", "zerolatency")?;
        set_property(&encoder, "key-int-max", &fps.saturating_mul(2).to_string())?;
    }

    if x264 {
        set_property(&encoder, "bframes", "0")?;
        set_property(&encoder, "byte-stream", "true")?;
    }

    let rate_control = options
        .get("rate_control")
        .map(String::as_str)
        .unwrap_or("crf");
    let mut x26x_options = Vec::new();

    for (key, value) in options {
        match key.as_str() {
            "preset" if x264 || x265 => set_property(&encoder, "speed-preset", value)?,
            "rate_control" if x264 => {
                let pass = match value.as_str() {
                    "crf" => "qual",
                    "cbr" | "vbr" => "cbr",
                    _ => return Err(anyhow!("unsupported x264 rate control {value:?}")),
                };
                set_property(&encoder, "pass", pass)?;
            }
            "rate_control" if x265 => {
                ensure!(
                    matches!(value.as_str(), "crf" | "cbr" | "vbr"),
                    "unsupported x265 rate control {value:?}"
                );

                if value == "cbr" {
                    x26x_options.push("strict-cbr=1".to_string());
                }
            }
            "rate_control" if factory == "vp8enc" || factory == "vp9enc" => {
                let usage = match value.as_str() {
                    "crf" => "cq",
                    "cbr" => "cbr",
                    "vbr" => "vbr",
                    _ => bail!("unsupported VPx rate control {value:?}"),
                };
                set_property(&encoder, "end-usage", usage)?;
            }
            "quality" if x264 => {
                if rate_control == "crf" {
                    set_property(&encoder, "quantizer", value)?;
                }
            }
            "quality" if x265 => {
                if rate_control == "crf" {
                    let quality: f64 = value.parse().context("x265 quality must be numeric")?;
                    ensure!(
                        quality.is_finite() && (0.0..=51.0).contains(&quality),
                        "invalid x265 quality"
                    );
                    x26x_options.push(format!("crf={quality}"));
                }
            }
            "maxrate" => {
                let kbps: u64 = value
                    .parse()
                    .context("video maximum bitrate must be an integer in kbit/s")?;
                ensure!(kbps > 0, "video maximum bitrate must be greater than zero");
                let property = if encoder.find_property("target-bitrate").is_some() {
                    "target-bitrate"
                } else {
                    "bitrate"
                };
                let bitrate = if (property == "target-bitrate" && factory != "svtav1enc")
                    || factory.starts_with("avenc_")
                    || factory == "openh264enc"
                {
                    kbps.checked_mul(1000).context("video bitrate overflow")?
                } else {
                    kbps
                };
                set_property(&encoder, property, &bitrate.to_string())?;

                if x264 || x265 {
                    x26x_options.push(format!("vbv-maxrate={kbps}"));
                    x26x_options.push(format!("vbv-bufsize={}", kbps.saturating_mul(2)));
                }
            }
            "deadline" if factory.starts_with("vp") => {
                let deadline = match value.as_str() {
                    "realtime" => "1",
                    "good" => "1000000",
                    _ => value,
                };
                set_property(&encoder, "deadline", deadline)?;
            }
            "rate_control" => set_property(&encoder, "rc-mode", value)?,
            "quality" => {
                let property = ["cq-level", "crf", "icq-quality", "qp", "quality"]
                    .into_iter()
                    .find(|name| encoder.find_property(name).is_some())
                    .context("this GStreamer encoder has no compatible quality property")?;
                set_property(&encoder, property, value)?;
            }
            "cpu_used" => set_property(&encoder, "cpu-used", value)?,
            "row_mt" | "row-mt" if value == "auto" => {}
            _ => set_property(&encoder, &key.replace('_', "-"), value)?,
        }
    }

    if !x26x_options.is_empty() {
        set_property(&encoder, "option-string", &x26x_options.join(":"))?;
    }

    Ok(encoder)
}

pub(crate) fn audio_encoder(
    codec: &str,
    options: &AudioOptions,
    bitrate: u64,
) -> Result<gst::Element> {
    let factory = audio_codec_factory(codec)?;
    let encoder = make(&factory)?;

    if factory != "identity" && encoder.find_property("bitrate").is_some() {
        let bitrate = if matches!(factory.as_str(), "lamemp3enc" | "twolamemp2enc") {
            if factory == "twolamemp2enc" {
                ensure!(
                    bitrate.is_multiple_of(1000)
                        && [
                            8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 192, 224,
                            256, 320, 384
                        ]
                        .contains(&(bitrate / 1000)),
                    "MP2 audio encoder options require a standard MPEG audio bitrate"
                );
            }
            bitrate / 1000
        } else {
            bitrate
        };
        set_property(&encoder, "bitrate", &bitrate.to_string())?;

        if factory == "lamemp3enc" {
            set_property(&encoder, "target", "bitrate")?;
        }
    }

    for (key, value) in options {
        match (factory.as_str(), key.as_str()) {
            ("twolamemp2enc", "cutoff") if value == "0" => {
                // The persisted automatic cutoff delegates filtering to TwoLAME.
            }
            ("opusenc", "application") => {
                let kind = match value.as_str() {
                    "audio" => "generic",
                    "voip" => "voice",
                    "lowdelay" => "restricted-lowdelay",
                    _ => bail!("unsupported Opus application {value:?}"),
                };
                set_property(&encoder, "audio-type", kind)?;
            }
            ("opusenc", "vbr") => {
                let mode = match value.as_str() {
                    "on" | "1" => "vbr",
                    "off" | "0" => "cbr",
                    "constrained" => "constrained-vbr",
                    _ => bail!("unsupported Opus VBR setting {value:?}"),
                };
                set_property(&encoder, "bitrate-type", mode)?;
            }
            ("opusenc", "frame_duration") => set_property(&encoder, "frame-size", value)?,
            ("opusenc", "compression_level") => set_property(&encoder, "complexity", value)?,
            _ => set_property(&encoder, &key.replace('_', "-"), value)?,
        }
    }

    Ok(encoder)
}

pub(super) fn parser(encoder: &gst::Element, video: bool) -> Result<gst::Element> {
    let name = encoder
        .factory()
        .context("encoder factory is missing")?
        .name();
    parser_for_factory(&name, video)
}

pub(super) fn parser_for_codec(codec: &str, video: bool) -> Result<gst::Element> {
    let factory = if video {
        video_codec_factory(codec)?
    } else {
        audio_codec_factory(codec)?
    };
    parser_for_factory(&factory, video)
}

fn parser_for_factory(name: &str, video: bool) -> Result<gst::Element> {
    let parser = if name.contains("264") {
        "h264parse"
    } else if name.contains("265") || name.contains("hevc") {
        "h265parse"
    } else if name.contains("aac") {
        "aacparse"
    } else if name.contains("mp3") || name.contains("mp2") {
        "mpegaudioparse"
    } else if name.contains("opus") {
        "opusparse"
    } else if name.contains("vorbis") {
        "vorbisparse"
    } else if name.contains("flac") {
        "flacparse"
    } else if name.contains("mpeg2") && video {
        "mpegvideoparse"
    } else {
        "identity"
    };

    make(parser)
}

pub(crate) fn muxer_factory(muxer: &str) -> Result<&'static str> {
    match muxer {
        "mpegts" => Ok("mpegtsmux"),
        "flv" => Ok("flvmux"),
        "matroska" => Ok("matroskamux"),
        "webm" => Ok("webmmux"),
        "mp4" | "mov" => Ok("mp4mux"),
        _ => bail!("GStreamer output format {muxer:?} is unavailable"),
    }
}

pub(crate) fn configure_muxer(
    mux: &gst::Element,
    format: &str,
    options: &BTreeMap<String, String>,
) -> Result<()> {
    if format == "mpegts" {
        set_property(mux, "alignment", "7")?;
    }
    if format == "flv" {
        set_property(mux, "streamable", "true")?;
    }
    // Destinations consume muxed buffers through appsink and cannot seek
    // backwards. Fragmented MP4 remains playable without a seekable sink.
    if matches!(format, "mp4" | "mov") {
        set_property(mux, "fragment-duration", "1000")?;
    }
    for (name, value) in options {
        let property = match name.as_str() {
            "pat_period" => "pat-interval",
            "sdt_period" => "si-interval",
            _ => name,
        };
        let value = if matches!(name.as_str(), "pat_period" | "sdt_period") {
            let seconds: f64 = value.parse()?;
            ensure!(
                seconds.is_finite() && seconds > 0.0,
                "invalid MPEG-TS table interval"
            );
            (seconds * 90_000.0).round().to_string()
        } else {
            value.clone()
        };
        set_property(mux, property, &value)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_x264_settings_use_the_installed_gstreamer_encoder() {
        let options = crate::video_option_defaults("libx264");
        let encoder = video_encoder("libx264", &options, 25, 640, 360).unwrap();
        assert_eq!(encoder.factory().unwrap().name(), "x264enc");
        assert_eq!(encoder.property::<u32>("quantizer"), 23);
    }

    #[test]
    fn unknown_and_out_of_range_properties_are_rejected() {
        crate::gstreamer::init().unwrap();
        let encoder = make("x264enc").unwrap();
        assert!(set_property(&encoder, "missing-option", "1").is_err());
        assert!(set_property(&encoder, "quantizer", "999999").is_err());
        assert!(set_property(&encoder, "speed-preset", "not-a-preset").is_err());
    }

    #[test]
    fn persisted_vp9_settings_map_to_native_rate_control() {
        let options = crate::video_option_defaults("libvpx-vp9");
        let encoder = video_encoder("libvpx-vp9", &options, 25, 640, 360).unwrap();
        assert_eq!(encoder.factory().unwrap().name(), "vp9enc");
        assert_eq!(encoder.property::<i32>("cq-level"), 31);
    }

    #[test]
    fn persisted_svt_av1_settings_preserve_quality_and_kilobit_units() {
        let mut options = crate::video_option_defaults("libsvtav1");
        options.insert("maxrate".to_owned(), "3500".to_owned());
        let encoder = video_encoder("libsvtav1", &options, 25, 640, 360).unwrap();

        assert_eq!(encoder.property::<i32>("crf"), 30);
        assert_eq!(encoder.property::<u32>("target-bitrate"), 3500);
    }

    #[test]
    fn saved_opus_application_uses_native_audio_type() {
        let options = [("application".to_owned(), "lowdelay".to_owned())].into();
        let encoder = audio_encoder("libopus", &options, 128_000).unwrap();
        assert_eq!(encoder.factory().unwrap().name(), "opusenc");
    }
}
