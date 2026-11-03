//! Video cards: Blackmagic DeckLink/UltraStudio output (SDI or HDMI) and
//! input, and V4L2 capture cards (Magewell, Epiphan and others) as input.
//!
//! The GStreamer `decklink` and `v4l2` plugins ship with NoAr; the card
//! drivers (Blackmagic Desktop Video, or the vendor's V4L2 driver) are
//! installed on the host. Devices are discovered, never assumed.
//!
//! Addresses:
//! - `decklink://N?mode=1080i5994[&connection=sdi]` — DeckLink device N;
//!   input `mode` may be `auto`.
//! - `v4l2:///dev/video0[?audio=hw:1,0]` — a V4L2 capture card, with the ALSA
//!   capture device that carries its audio (silence without it).

use std::time::{Duration, Instant};

use ::gstreamer as gst;
use anyhow::{Context, Result, bail};
use gst::prelude::*;
use gstreamer_app::AppSrc;

use super::backlog::{Backlog, video_backlog_bytes};
use crate::utils::config::OutputConfig;

const DECKLINK_SCHEME: &str = "decklink://";
const V4L2_SCHEME: &str = "v4l2://";
const CLOSE_GRACE: Duration = Duration::from_secs(2);
/// Jitter the real-time playout may have before a frame reaches the card.
const OUTPUT_LATENCY: Duration = Duration::from_millis(200);
/// DeckLink connections an input may select.
const CONNECTIONS: &[&str] = &[
    "auto",
    "sdi",
    "hdmi",
    "optical-sdi",
    "component",
    "composite",
    "svideo",
];

/// A DeckLink video mode: picture size, frame rate and scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoMode {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    /// Frames per second as a fraction (`30000/1001` for 29.97).
    pub fps_num: i32,
    pub fps_den: i32,
    pub interlaced: bool,
}

/// The DeckLink modes NoAr can play and record, by the names the cards use.
pub const VIDEO_MODES: &[VideoMode] = &[
    mode("ntsc", 720, 486, 30_000, 1001, true),
    mode("pal", 720, 576, 25, 1, true),
    mode("720p50", 1280, 720, 50, 1, false),
    mode("720p5994", 1280, 720, 60_000, 1001, false),
    mode("720p60", 1280, 720, 60, 1, false),
    mode("1080p2398", 1920, 1080, 24_000, 1001, false),
    mode("1080p24", 1920, 1080, 24, 1, false),
    mode("1080p25", 1920, 1080, 25, 1, false),
    mode("1080p2997", 1920, 1080, 30_000, 1001, false),
    mode("1080p30", 1920, 1080, 30, 1, false),
    mode("1080i50", 1920, 1080, 25, 1, true),
    mode("1080i5994", 1920, 1080, 30_000, 1001, true),
    mode("1080i60", 1920, 1080, 30, 1, true),
    mode("1080p50", 1920, 1080, 50, 1, false),
    mode("1080p5994", 1920, 1080, 60_000, 1001, false),
    mode("1080p60", 1920, 1080, 60, 1, false),
    mode("2160p2398", 3840, 2160, 24_000, 1001, false),
    mode("2160p24", 3840, 2160, 24, 1, false),
    mode("2160p25", 3840, 2160, 25, 1, false),
    mode("2160p2997", 3840, 2160, 30_000, 1001, false),
    mode("2160p30", 3840, 2160, 30, 1, false),
    mode("2160p50", 3840, 2160, 50, 1, false),
    mode("2160p5994", 3840, 2160, 60_000, 1001, false),
    mode("2160p60", 3840, 2160, 60, 1, false),
];

const fn mode(
    name: &'static str,
    width: u32,
    height: u32,
    fps_num: i32,
    fps_den: i32,
    interlaced: bool,
) -> VideoMode {
    VideoMode {
        name,
        width,
        height,
        fps_num,
        fps_den,
        interlaced,
    }
}

pub fn video_mode(name: &str) -> Option<VideoMode> {
    VIDEO_MODES.iter().copied().find(|mode| mode.name == name)
}

/// A parsed `decklink://` address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckLinkAddress {
    pub device: u32,
    /// `None` detects the input mode.
    pub mode: Option<VideoMode>,
    pub connection: String,
}

/// A parsed `v4l2://` address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4l2Address {
    pub device: String,
    pub audio: Option<String>,
}

fn query(rest: &str) -> (&str, Vec<(String, String)>) {
    match rest.split_once('?') {
        Some((base, query)) => (
            base,
            url::form_urlencoded::parse(query.as_bytes())
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect(),
        ),
        None => (rest, Vec::new()),
    }
}

fn strip_scheme<'a>(address: &'a str, scheme: &str) -> Option<&'a str> {
    address
        .get(..scheme.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(scheme))
        .map(|_| &address[scheme.len()..])
}

/// Parses a `decklink://` address; `Ok(None)` for other addresses. Input may
/// leave the mode to detection (`mode=auto`); output needs a fixed mode.
pub fn parse_decklink(address: &str, output: bool) -> Result<Option<DeckLinkAddress>, String> {
    let Some(rest) = strip_scheme(address.trim(), DECKLINK_SCHEME) else {
        return Ok(None);
    };
    let (device, pairs) = query(rest);
    let device = device
        .trim_end_matches('/')
        .parse::<u32>()
        .map_err(|_| "a DeckLink address is decklink://<device number>?mode=<mode>".to_owned())?;
    let mut mode = None;
    let mut connection = "auto".to_owned();
    for (key, value) in pairs {
        match key.as_str() {
            "mode" if value == "auto" => mode = None,
            "mode" => {
                mode = Some(
                    video_mode(&value)
                        .ok_or_else(|| format!("unsupported DeckLink mode {value:?}"))?,
                );
            }
            "connection" if CONNECTIONS.contains(&value.as_str()) => connection = value,
            "connection" => return Err(format!("unsupported DeckLink connection {value:?}")),
            _ => return Err(format!("unsupported DeckLink option {key:?}")),
        }
    }
    if output && mode.is_none() {
        return Err("a DeckLink output needs a video mode, such as mode=1080i5994".to_owned());
    }
    if output && connection != "auto" {
        return Err("DeckLink outputs play on all their connectors; remove connection".to_owned());
    }
    Ok(Some(DeckLinkAddress {
        device,
        mode,
        connection,
    }))
}

/// Parses a `v4l2://` address; `Ok(None)` for other addresses.
pub fn parse_v4l2(address: &str) -> Result<Option<V4l2Address>, String> {
    let Some(rest) = strip_scheme(address.trim(), V4L2_SCHEME) else {
        return Ok(None);
    };
    let (device, pairs) = query(rest);
    let valid_device = device.starts_with("/dev/")
        && !device.contains("..")
        && device.len() > "/dev/".len()
        && !device.chars().any(|c| c.is_control() || c.is_whitespace());
    if !valid_device {
        return Err("a V4L2 address is v4l2:///dev/videoN".to_owned());
    }
    let mut audio = None;
    for (key, value) in pairs {
        match key.as_str() {
            "audio"
                if !value.is_empty()
                    && value.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, ':' | ',' | '_' | '-' | '=')
                    }) =>
            {
                audio = Some(value);
            }
            "audio" => return Err("audio names an ALSA capture device, such as hw:1,0".to_owned()),
            _ => return Err(format!("unsupported V4L2 option {key:?}")),
        }
    }
    Ok(Some(V4l2Address {
        device: device.to_owned(),
        audio,
    }))
}

/// A video card port found on this server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoDevice {
    /// `decklink` or `v4l2`.
    pub kind: &'static str,
    /// `input` or `output`.
    pub direction: &'static str,
    /// Name shown by the driver, such as `DeckLink Duo (2)`.
    pub name: String,
    /// Address to use, such as `decklink://1` or `v4l2:///dev/video0`.
    pub address: String,
}

/// DeckLink inputs and outputs and V4L2 capture devices this server sees.
/// An empty list means no driver or no card visible to NoAr.
pub fn discover_video_devices() -> Result<Vec<VideoDevice>> {
    crate::gstreamer::init()?;
    let monitor = gst::DeviceMonitor::new();
    monitor.add_filter(Some("Video/Source"), None);
    monitor.add_filter(Some("Video/Sink"), None);
    monitor.start().context("starting video device discovery")?;
    let devices = monitor.devices();
    monitor.stop();

    let mut found = Vec::new();
    for device in devices {
        let class = device.device_class();
        let direction = if class.contains("Sink") {
            "output"
        } else {
            "input"
        };
        let Some(properties) = device.properties() else {
            continue;
        };
        let name = device.display_name().to_string();
        if let Ok(number) = properties.get::<i32>("device-number") {
            found.push(VideoDevice {
                kind: "decklink",
                direction,
                name,
                address: format!("decklink://{number}"),
            });
        } else if direction == "input"
            && properties
                .get::<String>("device.api")
                .is_ok_and(|api| api == "v4l2")
            && let Ok(path) = properties.get::<String>("device.path")
        {
            found.push(VideoDevice {
                kind: "v4l2",
                direction,
                name,
                address: format!("v4l2://{path}"),
            });
        }
    }
    found.sort_by(|a, b| (a.kind, a.direction, &a.address).cmp(&(b.kind, b.direction, &b.address)));
    found.dedup();
    Ok(found)
}

/// Builds `factory`, explaining what to install when it is missing.
pub(crate) fn make(factory: &str) -> Result<gst::Element> {
    gst::ElementFactory::make(factory)
        .build()
        .with_context(|| format!("GStreamer element {factory} is unavailable"))
}

/// The DeckLink program output.
pub(super) struct DeckLinkSink {
    pipeline: gst::Pipeline,
    video: AppSrc,
    audio: AppSrc,
    bus: gst::Bus,
    video_backlog: Backlog,
    audio_backlog: Backlog,
}

impl DeckLinkSink {
    pub(super) fn open(address: &str, cfg: &OutputConfig) -> Result<Self> {
        crate::gstreamer::init()?;
        let target = parse_decklink(address, true)
            .map_err(anyhow::Error::msg)?
            .context("SDI output needs a decklink:// address")?;
        let mode = target.mode.context("SDI output needs a video mode")?;

        let pipeline = gst::Pipeline::new();
        let video_backlog = Backlog::new("SDI", "video", video_backlog_bytes(cfg));
        let audio_backlog = Backlog::new("SDI", "audio", 4 * 1024 * 1024);
        // Frames are stamped when the real-time playout hands them over; the
        // card plays them on its own clock and the pipeline absorbs jitter.
        let video = AppSrc::builder()
            .format(gst::Format::Time)
            .is_live(true)
            .do_timestamp(true)
            .max_bytes(video_backlog.limit())
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
            .is_live(true)
            .do_timestamp(true)
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

        let convert = make("videoconvert")?;
        let scale = make("videoscale")?;
        scale.set_property("add-borders", true);
        // The channel renders at whole frame rates; the card's rate (29.97,
        // 59.94i…) is met by repeating or dropping a frame when needed.
        let rate = make("videorate")?;
        let caps = make("capsfilter")?;
        caps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "UYVY")
                .field("width", mode.width as i32)
                .field("height", mode.height as i32)
                .field("framerate", gst::Fraction::new(mode.fps_num, mode.fps_den))
                .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
                .build(),
        );
        // Interlaced modes carry each progressive picture as both fields.
        let scan = make("capssetter")?;
        scan.set_property(
            "caps",
            if mode.interlaced {
                gst::Caps::builder("video/x-raw")
                    .field("interlace-mode", "interleaved")
                    .field("field-order", "top-field-first")
                    .build()
            } else {
                gst::Caps::builder("video/x-raw")
                    .field("interlace-mode", "progressive")
                    .build()
            },
        );
        let video_sink = make("decklinkvideosink")?;
        video_sink.set_property("device-number", target.device as i32);
        video_sink.set_property_from_str("mode", mode.name);

        let audio_convert = make("audioconvert")?;
        let audio_resample = make("audioresample")?;
        let audio_caps = make("capsfilter")?;
        audio_caps.set_property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("format", "S32LE")
                .field("rate", 48_000_i32)
                .field("channels", 2_i32)
                .build(),
        );
        let audio_sink = make("decklinkaudiosink")?;
        audio_sink.set_property("device-number", target.device as i32);

        pipeline.add_many([
            video.upcast_ref(),
            &convert,
            &scale,
            &rate,
            &caps,
            &scan,
            &video_sink,
            audio.upcast_ref(),
            &audio_convert,
            &audio_resample,
            &audio_caps,
            &audio_sink,
        ])?;
        gst::Element::link_many([
            video.upcast_ref(),
            &convert,
            &scale,
            &rate,
            &caps,
            &scan,
            &video_sink,
        ])?;
        gst::Element::link_many([
            audio.upcast_ref(),
            &audio_convert,
            &audio_resample,
            &audio_caps,
            &audio_sink,
        ])?;
        pipeline.set_latency(gst::ClockTime::from_nseconds(
            OUTPUT_LATENCY.as_nanos() as u64
        ));

        let bus = pipeline.bus().context("SDI pipeline has no bus")?;
        pipeline.set_state(gst::State::Playing).with_context(|| {
            format!(
                "opening DeckLink output {} in {} (driver, device number and mode)",
                target.device, mode.name
            )
        })?;
        log::info!(
            "SDI output on DeckLink device {} in {}",
            target.device,
            mode.name
        );

        Ok(Self {
            pipeline,
            video,
            audio,
            bus,
            video_backlog,
            audio_backlog,
        })
    }

    pub(super) fn poll(&self) -> Result<()> {
        while let Some(message) = self.bus.pop_filtered(&[gst::MessageType::Error]) {
            if let gst::MessageView::Error(error) = message.view() {
                bail!("SDI output failed: {}", error.error());
            }
        }
        Ok(())
    }

    pub(super) fn push_video(&self, buffer: gst::Buffer) -> Result<()> {
        if !self.video_backlog.admit(self.video.current_level_bytes())? {
            return Ok(());
        }
        // The source stamps it on arrival.
        let mut buffer = buffer;
        if let Some(writable) = buffer.get_mut() {
            writable.set_pts(gst::ClockTime::NONE);
            writable.set_dts(gst::ClockTime::NONE);
        }
        self.video.push_buffer(buffer)?;
        Ok(())
    }

    pub(super) fn push_audio(&self, buffer: gst::Buffer) -> Result<()> {
        if !self.audio_backlog.admit(self.audio.current_level_bytes())? {
            return Ok(());
        }
        let mut buffer = buffer;
        if let Some(writable) = buffer.get_mut() {
            writable.set_pts(gst::ClockTime::NONE);
            writable.set_dts(gst::ClockTime::NONE);
        }
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

impl Drop for DeckLinkSink {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_match_the_card_names_and_rates() {
        let ntsc_hd = video_mode("1080i5994").unwrap();
        assert_eq!((ntsc_hd.width, ntsc_hd.height), (1920, 1080));
        assert_eq!((ntsc_hd.fps_num, ntsc_hd.fps_den), (30_000, 1001));
        assert!(ntsc_hd.interlaced);
        let p2997 = video_mode("1080p2997").unwrap();
        assert!(!p2997.interlaced);
        assert_eq!((p2997.fps_num, p2997.fps_den), (30_000, 1001));
        assert_eq!(video_mode("1080p30").unwrap().fps_num, 30);
        assert!(video_mode("1080x").is_none());

        // Every mode exists in the installed decklink plugin.
        crate::gstreamer::init().unwrap();
        let sink = make("decklinkvideosink").unwrap();
        for mode in VIDEO_MODES {
            sink.set_property_from_str("mode", mode.name);
            let set = sink.property_value("mode");
            let nick = glib_enum_nick(&set);
            assert_eq!(nick.as_deref(), Some(mode.name), "{}", mode.name);
        }
    }

    fn glib_enum_nick(value: &gst::glib::Value) -> Option<String> {
        gst::glib::EnumValue::from_value(value).map(|(_, value)| value.nick().to_owned())
    }

    #[test]
    fn decklink_addresses_are_checked() {
        let output = parse_decklink("decklink://1?mode=1080i5994", true)
            .unwrap()
            .unwrap();
        assert_eq!(output.device, 1);
        assert_eq!(output.mode.unwrap().name, "1080i5994");

        let input = parse_decklink("DeckLink://0?mode=auto&connection=sdi", false)
            .unwrap()
            .unwrap();
        assert_eq!(input.mode, None);
        assert_eq!(input.connection, "sdi");

        assert!(parse_decklink("decklink://0", true).is_err());
        assert!(parse_decklink("decklink://x?mode=1080i5994", true).is_err());
        assert!(parse_decklink("decklink://0?mode=1080i5994&connection=sdi", true).is_err());
        assert!(parse_decklink("decklink://0?mode=9999", false).is_err());
        assert!(parse_decklink("decklink://0?key=1", false).is_err());
        assert_eq!(parse_decklink("srt://host:9000", true).unwrap(), None);
    }

    #[test]
    fn v4l2_addresses_are_checked() {
        let card = parse_v4l2("v4l2:///dev/video0?audio=hw:1,0")
            .unwrap()
            .unwrap();
        assert_eq!(card.device, "/dev/video0");
        assert_eq!(card.audio.as_deref(), Some("hw:1,0"));
        assert_eq!(
            parse_v4l2("v4l2:///dev/video2").unwrap().unwrap().audio,
            None
        );
        assert!(parse_v4l2("v4l2://video0").is_err());
        assert!(parse_v4l2("v4l2:///dev/../etc/passwd").is_err());
        assert!(parse_v4l2("v4l2:///dev/video0?audio=$(x)").is_err());
        assert_eq!(parse_v4l2("ndi://A").unwrap(), None);
    }

    #[test]
    fn without_a_card_sdi_output_fails_with_a_reason() {
        let cfg = OutputConfig::new(64, 36, 30, 48_000);
        // No DeckLink device exists in the test environment.
        if discover_video_devices()
            .unwrap()
            .iter()
            .any(|device| device.kind == "decklink")
        {
            return;
        }
        let error = DeckLinkSink::open("decklink://0?mode=1080i5994", &cfg)
            .err()
            .unwrap();
        assert!(format!("{error:#}").contains("DeckLink"), "{error:#}");
    }
}
