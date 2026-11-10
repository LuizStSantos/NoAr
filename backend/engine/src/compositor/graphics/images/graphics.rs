//! Bounded GIF and animated WebP playback: decode off the frame thread, then
//! select cached frames. A still WebP keeps the ordinary GStreamer image path.

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
    compositor::{graphics::geometry::cut_corners, overlay::chroma_alpha},
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
    let Some(kind) = animated_kind(path) else {
        return Ok(None);
    };

    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Weak<ImageAnimation>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let metadata = path.metadata()?;
    ensure!(
        metadata.len() <= 10 * 1024 * 1024,
        "{} exceeds 10 MiB",
        kind.name()
    );
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
        "{} layer frame exceeds 32 MiB",
        kind.name()
    );
    let animation = Arc::new(match kind {
        AnimatedKind::Gif => {
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
            decode(path, width, height, radius)?
        }
        AnimatedKind::WebP => decode_webp(path, width, height, radius)?,
    });
    let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
    entries.retain(|_, animation| animation.strong_count() > 0);
    entries.insert(key, Arc::downgrade(&animation));

    Ok(Some(animation))
}

#[derive(Clone, Copy)]
enum AnimatedKind {
    Gif,
    WebP,
}

impl AnimatedKind {
    fn name(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::WebP => "WebP animation",
        }
    }
}

/// GIF by extension; WebP only when its header marks an animation, so a still
/// WebP keeps the GStreamer decoder.
fn animated_kind(path: &Path) -> Option<AnimatedKind> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();

    match extension.as_str() {
        "gif" => Some(AnimatedKind::Gif),
        "webp" if is_animated_webp(path) => Some(AnimatedKind::WebP),
        _ => None,
    }
}

/// RIFF/WEBP with an extended (`VP8X`) header whose animation flag is set.
fn is_animated_webp(path: &Path) -> bool {
    use std::io::Read;

    let mut header = [0_u8; 21];

    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header[..4] == b"RIFF"
        && &header[8..12] == b"WEBP"
        && &header[12..16] == b"VP8X"
        && header[20] & 0x02 != 0
}

/// Scale one decoded picture to the layer, cut rounded corners and cache its
/// alpha; `end` is when the frame stops showing in the loop.
fn layer_frame(
    decoded: &VideoFrame,
    width: u32,
    height: u32,
    radius: f64,
    end: Duration,
) -> Result<ImageFrame> {
    let mut frame =
        FrameConverter::new(decoded, PixelFormat::Yuva420p, width, height)?.run(decoded)?;
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

    Ok(ImageFrame { frame, alpha, end })
}

fn frame_bytes(frame: &ImageFrame) -> usize {
    (0..4)
        .map(|plane| frame.frame.data(plane).len())
        .sum::<usize>()
        + frame.alpha.len()
}

/// The first frame of an animated WebP at its own size, for callers that need
/// a still picture (sizes, L-shape art); `None` for any other file.
pub(crate) fn animated_webp_still(path: &Path) -> Result<Option<VideoFrame>> {
    if !matches!(animated_kind(path), Some(AnimatedKind::WebP)) {
        return Ok(None);
    }

    let (decoder, source_width, source_height) = webp_decoder(path)?;
    let mut frames = WebPFrames::new(decoder, source_width, source_height)?;
    let (rgba, width, height, _) = frames
        .next()?
        .context("WebP animation contains no frames")?;

    Ok(Some(crate::compositor::text::rgba_to_yuva420p(
        rgba, width, height,
    )?))
}

fn webp_decoder(
    path: &Path,
) -> Result<(
    image_webp::WebPDecoder<std::io::BufReader<std::fs::File>>,
    u32,
    u32,
)> {
    ensure!(
        std::fs::metadata(path)?.len() <= 10 * 1024 * 1024,
        "WebP animation exceeds 10 MiB"
    );
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut decoder = image_webp::WebPDecoder::new(file).context("invalid WebP animation")?;
    ensure!(decoder.is_animated(), "WebP is not animated");
    let (source_width, source_height) = decoder.dimensions();
    ensure!(
        (source_width as u64) * (source_height as u64) * 4 <= MAX_BYTES as u64,
        "WebP animation source frame exceeds 32 MiB"
    );
    ensure!(
        (decoder.num_frames() as usize) <= MAX_FRAMES,
        "WebP animation exceeds 256 frames"
    );
    decoder.set_memory_limit(MAX_BYTES * 2);
    // Overlays show what is under them where the animation is empty, not the
    // file's suggested background colour.
    decoder
        .set_background_color([0, 0, 0, 0])
        .context("invalid WebP animation")?;

    Ok((decoder, source_width, source_height))
}

/// One composed RGBA frame, its even width and height, and its delay in ms.
type RgbaFrame<'a> = (&'a [u8], u32, u32, u32);

/// Composed WebP frames as RGBA with even sizes: `rgba_to_yuva420p` samples
/// chroma per 2x2 block, so odd sizes repeat their last column/row.
struct WebPFrames {
    decoder: image_webp::WebPDecoder<std::io::BufReader<std::fs::File>>,
    channels: usize,
    buffer: Vec<u8>,
    rgba: Vec<u8>,
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
}

impl WebPFrames {
    fn new(
        decoder: image_webp::WebPDecoder<std::io::BufReader<std::fs::File>>,
        source_width: u32,
        source_height: u32,
    ) -> Result<Self> {
        let width = source_width + source_width % 2;
        let height = source_height + source_height % 2;

        Ok(Self {
            channels: if decoder.has_alpha() { 4 } else { 3 },
            buffer: vec![0; decoder.output_buffer_size().context("WebP too large")?],
            rgba: vec![0; width as usize * height as usize * 4],
            decoder,
            source_width,
            source_height,
            width,
            height,
        })
    }

    /// The next frame and its delay in milliseconds, or `None` after the last.
    fn next(&mut self) -> Result<Option<RgbaFrame<'_>>> {
        let delay = match self.decoder.read_frame(&mut self.buffer) {
            Ok(delay) => delay,
            Err(image_webp::DecodingError::NoMoreFrames) => return Ok(None),
            Err(error) => anyhow::bail!("WebP animation decoding failed: {error}"),
        };
        let channels = self.channels;

        for row in 0..self.height as usize {
            let source_row = row.min(self.source_height as usize - 1);
            for column in 0..self.width as usize {
                let source_column = column.min(self.source_width as usize - 1);
                let from = (source_row * self.source_width as usize + source_column) * channels;
                let to = (row * self.width as usize + column) * 4;
                self.rgba[to..to + 3].copy_from_slice(&self.buffer[from..from + 3]);
                self.rgba[to + 3] = if channels == 4 {
                    self.buffer[from + 3]
                } else {
                    255
                };
            }
        }

        Ok(Some((&self.rgba, self.width, self.height, delay)))
    }
}

/// Animated WebP: GStreamer's `webpdec` decodes only still pictures, so the
/// frames come from `image-webp`, composed on a transparent canvas.
fn decode_webp(path: &Path, width: u32, height: u32, radius: f64) -> Result<ImageAnimation> {
    // Frames are scaled through GStreamer: start it before the decoding deadline.
    crate::gstreamer::init()?;
    let (decoder, source_width, source_height) = webp_decoder(path)?;
    let mut source = WebPFrames::new(decoder, source_width, source_height)?;
    let mut frames = Vec::new();
    let mut duration = Duration::ZERO;
    let mut bytes = 0;
    let deadline = Instant::now() + Duration::from_secs(10);

    while let Some((rgba, even_width, even_height, delay)) = source.next()? {
        ensure!(
            Instant::now() < deadline,
            "WebP animation decoding timed out"
        );
        // Like browsers, a frame without a usable delay shows for 100 ms.
        let delay = if delay <= 10 { 100 } else { u64::from(delay) };
        duration += Duration::from_millis(delay);
        ensure!(
            duration <= MAX_DURATION,
            "WebP animation exceeds 60 seconds"
        );
        let decoded = crate::compositor::text::rgba_to_yuva420p(rgba, even_width, even_height)?;
        let frame = layer_frame(&decoded, width, height, radius, duration)?;
        bytes += frame_bytes(&frame);
        ensure!(
            bytes <= MAX_BYTES,
            "decoded WebP animation exceeds 32 MiB at this layer size"
        );
        frames.push(frame);
    }

    ensure!(!frames.is_empty(), "WebP animation contains no frames");

    Ok(ImageAnimation {
        frames,
        started: Instant::now(),
        duration,
        bytes,
    })
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
            let delay = Duration::from_nanos(
                buffer
                    .duration()
                    .map_or(100_000_000, gst::ClockTime::nseconds),
            )
            .max(Duration::from_millis(10));
            duration += delay;
            ensure!(duration <= MAX_DURATION, "GIF exceeds 60 seconds");
            let frame = layer_frame(&decoded, width, height, radius, duration)?;
            bytes += frame_bytes(&frame);
            ensure!(
                bytes <= MAX_BYTES,
                "decoded GIF exceeds 32 MiB at this layer size"
            );
            frames.push(frame);
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

    #[test]
    fn animated_webp_plays_its_frames_with_delays_transparency_and_loop() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/graphics.webp");
        let animation = load_animation(&path, 66, 50, 0.0).unwrap().unwrap();
        assert_eq!(animation.frames.len(), 2);
        assert_eq!(animation.frames[0].end, Duration::from_millis(200));
        assert_eq!(animation.duration, Duration::from_millis(600));
        // The transparent corner of the first frame stays transparent; the
        // second frame is opaque.
        let first = &animation.at(Duration::ZERO).frame;
        assert_eq!(first.data(3)[0], 0);
        assert_eq!(first.data(3)[first.stride(3) * 40 + 60], 255);
        assert_eq!(
            animation.at(Duration::from_millis(300)).frame.data(3)[0],
            255
        );
        assert_ne!(
            animation.at(Duration::from_millis(199)).frame.data(1),
            animation.at(Duration::from_millis(200)).frame.data(1)
        );
        assert_eq!(
            animation.at(Duration::ZERO).frame.data(0),
            animation.at(Duration::from_millis(600)).frame.data(0)
        );
        assert!(load_animation(&path, 8192, 8192, 0.0).is_err());
        // Callers needing a still picture get the first frame, padded to even.
        let still = crate::gstreamer::load_image(&path.to_string_lossy()).unwrap();
        assert_eq!((still.width(), still.height()), (34, 26));
        assert_eq!(still.data(3)[0], 0);
    }

    #[test]
    fn a_still_webp_keeps_the_ordinary_image_path() {
        let path = std::env::temp_dir().join(format!("noar-still-{}.webp", std::process::id()));
        // RIFF/WEBP extended header with only the alpha flag set.
        let mut header = *b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\x10\0\0\0";
        std::fs::write(&path, header).unwrap();
        assert!(animated_kind(&path).is_none());
        header[20] |= 0x02;
        std::fs::write(&path, header).unwrap();
        assert!(matches!(animated_kind(&path), Some(AnimatedKind::WebP)));
        std::fs::remove_file(&path).unwrap();
        assert!(animated_kind(Path::new("logo.png")).is_none());
    }
}
