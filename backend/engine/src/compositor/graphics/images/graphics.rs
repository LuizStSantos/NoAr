//! Bounded GIF playback: decode off the frame thread, then select cached frames.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, PoisonError, Weak},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, ensure};
use gstreamer::{self as gst, prelude::*};
use gstreamer_app::AppSink;
use gstreamer_video::VideoInfo;

use crate::{
    compositor::{graphics::cut_corners, overlay::chroma_alpha},
    frame::{PixelFormat, VideoFrame},
    gstreamer::{FrameConverter, copy_sample_frame},
};

const MAX_FRAMES: usize = 256;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_DURATION: Duration = Duration::from_secs(60);

pub(crate) struct ImageFrame {
    pub frame: VideoFrame,
    pub alpha: Vec<u8>,
    end: Duration,
}

pub(crate) struct ImageAnimation {
    frames: Vec<ImageFrame>,
    started: Instant,
    duration: Duration,
    pub bytes: usize,
}

impl ImageAnimation {
    pub fn current(&self) -> &ImageFrame {
        self.at(self.started.elapsed())
    }

    fn at(&self, elapsed: Duration) -> &ImageFrame {
        let phase = elapsed.as_nanos() % self.duration.as_nanos();
        let index = self
            .frames
            .partition_point(|frame| frame.end.as_nanos() <= phase);

        &self.frames[index.min(self.frames.len() - 1)]
    }
}

#[derive(Hash, PartialEq, Eq)]
struct CacheKey {
    path: PathBuf,
    modified: Option<SystemTime>,
    bytes: u64,
    width: u32,
    height: u32,
    radius: u64,
}

/// Weak entries share decoded frames across models/refreshes without retaining
/// unused artwork. File identity and target geometry invalidate old entries.
pub(crate) fn load_animation(
    path: &Path,
    width: u32,
    height: u32,
    radius: f64,
) -> Result<Option<Arc<ImageAnimation>>> {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
    {
        return Ok(None);
    }

    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Weak<ImageAnimation>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let metadata = path.metadata()?;
    ensure!(metadata.len() <= 10 * 1024 * 1024, "GIF exceeds 10 MiB");
    let key = CacheKey {
        path: path.to_owned(),
        modified: metadata.modified().ok(),
        bytes: metadata.len(),
        width,
        height,
        radius: radius.to_bits(),
    };

    if let Some(animation) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .and_then(Weak::upgrade)
    {
        return Ok(Some(animation));
    }

    ensure!(
        (width as u64) * (height as u64) * 3 <= MAX_BYTES as u64,
        "GIF layer frame exceeds 32 MiB"
    );
    let header = std::fs::read(path)?;
    ensure!(
        header.len() >= 13 && matches!(&header[..6], b"GIF87a" | b"GIF89a"),
        "invalid GIF header"
    );
    let source_width = u16::from_le_bytes([header[6], header[7]]) as u64;
    let source_height = u16::from_le_bytes([header[8], header[9]]) as u64;
    ensure!(
        source_width * source_height * 4 <= MAX_BYTES as u64,
        "GIF source frame exceeds 32 MiB"
    );
    let animation = Arc::new(decode(path, width, height, radius)?);
    let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
    entries.retain(|_, animation| animation.strong_count() > 0);
    entries.insert(key, Arc::downgrade(&animation));

    Ok(Some(animation))
}

fn decode(path: &Path, width: u32, height: u32, radius: f64) -> Result<ImageAnimation> {
    crate::gstreamer::init()?;
    let pipeline = gst::Pipeline::new();
    let source = gst::ElementFactory::make("filesrc")
        .property("location", path.to_string_lossy().as_ref())
        .build()?;
    // The generic image decoder may emit one still frame. GIF's native demuxer
    // and decoder preserve disposal/transparency and each frame's duration.
    let demux = gst::ElementFactory::make("avdemux_gif")
        .build()
        .context("GIF demuxer unavailable")?;
    let decoder = gst::ElementFactory::make("avdec_gif")
        .build()
        .context("GIF decoder unavailable")?;
    let convert = gst::ElementFactory::make("videoconvert").build()?;
    let sink = AppSink::builder()
        .caps(
            &gst::Caps::builder("video/x-raw")
                .field("format", "A420")
                .build(),
        )
        .sync(false)
        .async_(false)
        .max_buffers(1)
        .drop(false)
        .build();
    pipeline.add_many([&source, &demux, &decoder, &convert, sink.upcast_ref()])?;
    source.link(&demux)?;
    gst::Element::link_many([&decoder, &convert, sink.upcast_ref()])?;
    let target = decoder
        .static_pad("sink")
        .context("GIF decoder input unavailable")?;
    demux.connect_pad_added(move |_, pad| {
        if !target.is_linked() && pad.name().starts_with("video_") {
            let _ = pad.link(&target);
        }
    });
    let result = (|| {
        pipeline.set_state(gst::State::Playing)?;
        let mut frames = Vec::new();
        let mut duration = Duration::ZERO;
        let mut bytes = 0;
        let deadline = Instant::now() + Duration::from_secs(10);

        loop {
            ensure!(Instant::now() < deadline, "GIF decoding timed out");
            let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(100)) else {
                if sink.is_eos() {
                    break;
                }

                if let Some(message) = pipeline
                    .bus()
                    .and_then(|bus| bus.pop_filtered(&[gst::MessageType::Error]))
                    && let gst::MessageView::Error(error) = message.view()
                {
                    anyhow::bail!("GIF decoding failed: {}", error.error());
                }

                continue;
            };
            ensure!(frames.len() < MAX_FRAMES, "GIF exceeds 256 frames");
            let info = VideoInfo::from_caps(sample.caps().context("GIF frame has no caps")?)?;
            ensure!(
                (info.width() as u64) * (info.height() as u64) * 4 <= MAX_BYTES as u64,
                "GIF frame exceeds 32 MiB"
            );
            let buffer = sample.buffer().context("GIF frame has no pixels")?;
            let decoded = copy_sample_frame(buffer, &info, PixelFormat::Yuva420p)?;
            let mut frame = FrameConverter::new(&decoded, PixelFormat::Yuva420p, width, height)?
                .run(&decoded)?;
            let alpha_stride = frame.stride(3);
            if radius > 0.0 {
                let mut mask = vec![255; width as usize * height as usize * 4];
                cut_corners(&mut mask, width, height, radius);
                for row in 0..height as usize {
                    for column in 0..width as usize {
                        if mask[(row * width as usize + column) * 4 + 3] == 0 {
                            frame.data_mut(3)[row * alpha_stride + column] = 0;
                        }
                    }
                }
            }

            let alpha = chroma_alpha(&frame, width, height);
            bytes += (0..4).map(|plane| frame.data(plane).len()).sum::<usize>() + alpha.len();
            ensure!(
                bytes <= MAX_BYTES,
                "decoded GIF exceeds 32 MiB at this layer size"
            );
            let delay = Duration::from_nanos(
                buffer
                    .duration()
                    .map_or(100_000_000, gst::ClockTime::nseconds),
            )
            .max(Duration::from_millis(10));
            duration += delay;
            ensure!(duration <= MAX_DURATION, "GIF exceeds 60 seconds");
            frames.push(ImageFrame {
                frame,
                alpha,
                end: duration,
            });
        }

        ensure!(!frames.is_empty(), "GIF contains no decoded frames");

        Ok(ImageAnimation {
            frames,
            started: Instant::now(),
            duration,
            bytes,
        })
    })();
    let _ = pipeline.set_state(gst::State::Null);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gif_frames_preserve_delays_transparency_and_loop_without_decoding_on_air() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/graphics.gif");
        let animation = load_animation(&path, 64, 48, 0.0).unwrap().unwrap();
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(animation.frames[0].end, Duration::from_millis(200));
        assert_eq!(animation.duration, Duration::from_millis(600));
        assert_eq!(animation.at(Duration::ZERO).frame.data(3)[0], 0);
        assert_eq!(animation.at(Duration::ZERO).frame.data(3)[20], 0);
        assert_ne!(
            animation.at(Duration::from_millis(199)).frame.data(0),
            animation.at(Duration::from_millis(200)).frame.data(0)
        );
        assert_eq!(
            animation.at(Duration::ZERO).frame.data(0),
            animation.at(Duration::from_millis(600)).frame.data(0)
        );
        assert!(Arc::ptr_eq(
            &animation,
            &load_animation(&path, 64, 48, 0.0).unwrap().unwrap()
        ));
        assert!(load_animation(&path, 8192, 8192, 0.0).is_err());
    }
}
