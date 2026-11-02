//! NDI program output: the composited picture and the processed program audio,
//! uncompressed, published as an NDI source on the local network.
//!
//! The GStreamer NDI elements are compiled into NoAr, but the NDI runtime
//! (`libndi`) is proprietary and installed on the host. NoAr checks for it the
//! same way the plugin loads it, and offers NDI only when it is present.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use ::gstreamer as gst;
use anyhow::{Context, Result, bail, ensure};
use gst::prelude::*;
use gstreamer_app::AppSrc;

use crate::utils::config::OutputConfig;

/// Library names and directory variables the NDI plugin searches, in order.
const LIBRARY_NAMES: &[&str] = &["libndi.so.6", "libndi.so.5"];
const RUNTIME_DIRECTORIES: &[&str] = &["NDI_RUNTIME_DIR_V6", "NDI_RUNTIME_DIR_V5"];
/// Longest NDI source name accepted; receivers show it in their lists.
const MAX_NAME_LENGTH: usize = 128;
const CLOSE_GRACE: Duration = Duration::from_secs(2);

/// Why NDI cannot be used on this server, or `None` when it can. Checked once:
/// installing the runtime takes a NoAr restart.
pub fn ndi_unavailable_reason() -> Option<&'static str> {
    static REASON: OnceLock<Option<&'static str>> = OnceLock::new();

    *REASON.get_or_init(|| {
        if crate::gstreamer::init().is_err() {
            return Some("GStreamer is not available");
        }
        if gst::ElementFactory::find("ndisink").is_none()
            || gst::ElementFactory::find("ndisinkcombiner").is_none()
        {
            return Some("the GStreamer NDI elements are not available");
        }
        if !runtime_loadable() {
            return Some("the NDI runtime (libndi.so.6) is not installed or not visible to NoAr");
        }
        None
    })
}

fn runtime_loadable() -> bool {
    let mut candidates = Vec::new();
    for variable in RUNTIME_DIRECTORIES {
        if let Some(directory) = std::env::var_os(variable) {
            for name in LIBRARY_NAMES {
                candidates.push(std::path::Path::new(&directory).join(name));
            }
        }
    }
    candidates.extend(LIBRARY_NAMES.iter().map(std::path::PathBuf::from));

    candidates.iter().any(|path| {
        // SAFETY: the NDI runtime has no library constructors with
        // preconditions; the plugin loads the same library the same way.
        unsafe { libloading::Library::new(path) }.is_ok()
    })
}

/// Checks an NDI source name before it is saved or published.
pub fn validate_ndi_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("NDI source name must not be empty".to_owned());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(format!(
            "NDI source name must be at most {MAX_NAME_LENGTH} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("NDI source name contains control characters".to_owned());
    }
    Ok(())
}

pub(super) struct NdiSink {
    pipeline: gst::Pipeline,
    video: AppSrc,
    audio: AppSrc,
    bus: gst::Bus,
}

impl NdiSink {
    pub(super) fn open(name: &str, cfg: &OutputConfig) -> Result<Self> {
        crate::gstreamer::init()?;
        if let Some(reason) = ndi_unavailable_reason() {
            bail!("NDI output is unavailable: {reason}");
        }
        validate_ndi_name(name).map_err(anyhow::Error::msg)?;

        let pipeline = gst::Pipeline::new();
        // The playout paces frames in real time and stamps them on its own
        // timeline, so the elements neither wait on a clock nor run live.
        let video = AppSrc::builder()
            .format(gst::Format::Time)
            .max_bytes(32 * 1024 * 1024)
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "I420")
                    .field("width", cfg.width as i32)
                    .field("height", cfg.height as i32)
                    .field("framerate", gst::Fraction::new(cfg.fps as i32, 1))
                    .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                    .build(),
            )
            .build();
        let audio = AppSrc::builder()
            .format(gst::Format::Time)
            .max_bytes(4 * 1024 * 1024)
            .caps(
                &gst::Caps::builder("audio/x-raw")
                    .field("format", "F32LE")
                    .field("layout", "interleaved")
                    .field("rate", cfg.sample_rate as i32)
                    .field("channels", 2_i32)
                    .build(),
            )
            .build();
        let make = |factory: &str| {
            gst::ElementFactory::make(factory)
                .build()
                .with_context(|| format!("GStreamer element {factory} is unavailable"))
        };
        let video_convert = make("videoconvert")?;
        // UYVY is NDI's native format; receivers take it without converting.
        let video_caps = make("capsfilter")?;
        video_caps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "UYVY")
                .build(),
        );
        let audio_convert = make("audioconvert")?;
        let combiner = make("ndisinkcombiner")?;
        let sink = make("ndisink")?;
        sink.set_property("ndi-name", name.trim());
        sink.set_property("sync", false);
        sink.set_property("async", false);

        pipeline.add_many([
            video.upcast_ref(),
            audio.upcast_ref(),
            &video_convert,
            &video_caps,
            &audio_convert,
            &combiner,
            &sink,
        ])?;
        gst::Element::link_many([video.upcast_ref(), &video_convert, &video_caps])?;
        video_caps
            .link_pads(None, &combiner, Some("video"))
            .context("linking NDI video")?;
        audio.link(&audio_convert)?;
        let audio_pad = combiner
            .request_pad_simple("audio")
            .context("NDI combiner has no audio input")?;
        audio_convert
            .static_pad("src")
            .context("audioconvert has no source pad")?
            .link(&audio_pad)
            .context("linking NDI audio")?;
        combiner.link(&sink)?;

        let bus = pipeline.bus().context("NDI pipeline has no bus")?;
        pipeline
            .set_state(gst::State::Playing)
            .context("starting the NDI sender")?;
        log::info!("NDI output publishing as {:?}", name.trim());

        Ok(Self {
            pipeline,
            video,
            audio,
            bus,
        })
    }

    pub(super) fn poll(&self) -> Result<()> {
        while let Some(message) = self.bus.pop_filtered(&[gst::MessageType::Error]) {
            if let gst::MessageView::Error(error) = message.view() {
                bail!("NDI output failed: {}", error.error());
            }
        }
        Ok(())
    }

    pub(super) fn push_video(&self, buffer: gst::Buffer) -> Result<()> {
        ensure!(
            self.video.current_level_bytes() < 32 * 1024 * 1024,
            "NDI output stopped consuming video"
        );
        self.video.push_buffer(buffer)?;
        Ok(())
    }

    pub(super) fn push_audio(&self, buffer: gst::Buffer) -> Result<()> {
        ensure!(
            self.audio.current_level_bytes() < 4 * 1024 * 1024,
            "NDI output stopped consuming audio"
        );
        self.audio.push_buffer(buffer)?;
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<()> {
        let _ = self.video.end_of_stream();
        let _ = self.audio.end_of_stream();
        let deadline = Instant::now() + CLOSE_GRACE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Some(message) = self.bus.timed_pop_filtered(
                gst::ClockTime::from_nseconds(left.as_nanos() as u64),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            ) else {
                break;
            };
            if matches!(
                message.view(),
                gst::MessageView::Eos(_) | gst::MessageView::Error(_)
            ) {
                break;
            }
        }
        self.pipeline.set_state(gst::State::Null)?;
        Ok(())
    }
}

impl Drop for NdiSink {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndi_names_are_checked() {
        assert!(validate_ndi_name("NoAr Canal 1").is_ok());
        assert!(validate_ndi_name("   ").is_err());
        assert!(validate_ndi_name("a\nb").is_err());
        assert!(validate_ndi_name(&"x".repeat(MAX_NAME_LENGTH + 1)).is_err());
    }

    #[test]
    fn ndi_elements_are_compiled_in() {
        crate::gstreamer::init().unwrap();
        assert!(gst::ElementFactory::find("ndisink").is_some());
        assert!(gst::ElementFactory::find("ndisinkcombiner").is_some());
        assert!(gst::ElementFactory::find("ndisrc").is_some());
    }

    #[test]
    fn without_the_runtime_ndi_is_refused_with_a_reason() {
        if ndi_unavailable_reason().is_none() {
            return; // A server with the NDI runtime installed.
        }
        let cfg = OutputConfig::new(64, 36, 25, 48_000);
        let error = NdiSink::open("NoAr test", &cfg).err().unwrap();
        assert!(format!("{error:#}").contains("NDI"), "{error:#}");
    }
}
