//! Video and audio frames owned by NoAr.
//!
//! GStreamer decodes and encodes; between the two, the compositor, the audio
//! mixer, the meters and the desktop window work on these frames. They keep
//! the shape the engine already uses (planes with a stride, planar `f32`
//! audio, an optional presentation timestamp) so modules can move to them
//! one at a time.
//!
//! Planes are shared until written: cloning a frame is cheap, and the first
//! write to a shared plane copies it (the behaviour the engine had with
//! `av_frame_ref` and `av_frame_make_writable`).
//!
//! While modules still take FFmpeg frames, [`VideoFrame::to_ffmpeg`],
//! [`AudioFrame::to_ffmpeg`] and the `From` conversions carry frames across;
//! they go away with the FFmpeg dependency.

use std::sync::Arc;

use anyhow::{Result, bail};
use ffmpeg_next::{ChannelLayout, format, frame, util::color};

/// Row starts are aligned like FFmpeg's default frame buffers.
const ROW_ALIGNMENT: usize = 32;

/// Pixel layouts the engine handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit Y, U and V planes, chroma at half width and height.
    Yuv420p,
    /// [`PixelFormat::Yuv420p`] plus a full-size alpha plane (overlays).
    Yuva420p,
    /// One plane of 8-bit blue, green, red and an unused byte (desktop).
    Bgrx,
}

impl PixelFormat {
    pub fn planes(self) -> usize {
        match self {
            Self::Yuv420p => 3,
            Self::Yuva420p => 4,
            Self::Bgrx => 1,
        }
    }

    /// Width in bytes and height in rows of `plane` for a picture of
    /// `width` x `height`.
    fn plane_size(self, plane: usize, width: u32, height: u32) -> (usize, usize) {
        let (width, height) = (width as usize, height as usize);

        match (self, plane) {
            (Self::Bgrx, _) => (width * 4, height),
            (Self::Yuv420p | Self::Yuva420p, 1 | 2) => (width.div_ceil(2), height.div_ceil(2)),
            (Self::Yuv420p | Self::Yuva420p, _) => (width, height),
        }
    }

    fn to_ffmpeg(self) -> format::Pixel {
        match self {
            Self::Yuv420p => format::Pixel::YUV420P,
            Self::Yuva420p => format::Pixel::YUVA420P,
            Self::Bgrx => format::Pixel::BGRZ,
        }
    }

    fn from_ffmpeg(format: format::Pixel) -> Result<Self> {
        Ok(match format {
            format::Pixel::YUV420P => Self::Yuv420p,
            format::Pixel::YUVA420P => Self::Yuva420p,
            format::Pixel::BGRZ => Self::Bgrx,
            other => bail!("pixel format {other:?} is not used by the engine"),
        })
    }
}

/// The YUV matrix a picture was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSpace {
    #[default]
    Unspecified,
    Bt709,
    Bt2020Ncl,
    Bt2020Cl,
    Other,
}

/// Limited (TV) or full (PC) range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorRange {
    #[default]
    Unspecified,
    /// 16–235, broadcast video.
    Limited,
    /// 0–255.
    Full,
}

impl ColorSpace {
    fn from_ffmpeg(space: color::Space) -> Self {
        match space {
            color::Space::Unspecified => Self::Unspecified,
            color::Space::BT709 => Self::Bt709,
            color::Space::BT2020NCL => Self::Bt2020Ncl,
            color::Space::BT2020CL => Self::Bt2020Cl,
            _ => Self::Other,
        }
    }

    fn to_ffmpeg(self) -> color::Space {
        match self {
            Self::Unspecified | Self::Other => color::Space::Unspecified,
            Self::Bt709 => color::Space::BT709,
            Self::Bt2020Ncl => color::Space::BT2020NCL,
            Self::Bt2020Cl => color::Space::BT2020CL,
        }
    }
}

impl ColorRange {
    fn from_ffmpeg(range: color::Range) -> Self {
        match range {
            color::Range::MPEG => Self::Limited,
            color::Range::JPEG => Self::Full,
            _ => Self::Unspecified,
        }
    }

    fn to_ffmpeg(self) -> color::Range {
        match self {
            Self::Unspecified => color::Range::Unspecified,
            Self::Limited => color::Range::MPEG,
            Self::Full => color::Range::JPEG,
        }
    }
}

#[derive(Debug, Clone)]
struct VideoPlane {
    /// Bytes from one row start to the next.
    stride: usize,
    data: Arc<Vec<u8>>,
}

/// A picture: its planes, size, timestamp and colour description.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    format: PixelFormat,
    width: u32,
    height: u32,
    planes: Vec<VideoPlane>,
    pts: Option<i64>,
    color_space: ColorSpace,
    color_range: ColorRange,
}

impl VideoFrame {
    /// A black-initialised (all zero bytes) picture of `width` x `height`.
    pub fn new(format: PixelFormat, width: u32, height: u32) -> Self {
        let planes = (0..format.planes())
            .map(|plane| {
                let (row_bytes, rows) = format.plane_size(plane, width, height);
                let stride = row_bytes.next_multiple_of(ROW_ALIGNMENT);

                VideoPlane {
                    stride,
                    data: Arc::new(vec![0; stride * rows]),
                }
            })
            .collect();

        Self {
            format,
            width,
            height,
            planes,
            pts: None,
            color_space: ColorSpace::default(),
            color_range: ColorRange::default(),
        }
    }

    /// A frame with no picture yet.
    pub fn empty() -> Self {
        Self::new(PixelFormat::Yuv420p, 0, 0)
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn planes(&self) -> usize {
        self.planes.len()
    }

    pub fn stride(&self, plane: usize) -> usize {
        self.planes[plane].stride
    }

    /// Width in bytes of the visible part of each row of `plane`.
    pub fn plane_row_bytes(&self, plane: usize) -> usize {
        self.format.plane_size(plane, self.width, self.height).0
    }

    /// Rows in `plane`.
    pub fn plane_height(&self, plane: usize) -> usize {
        self.format.plane_size(plane, self.width, self.height).1
    }

    /// All rows of `plane`, `stride` bytes each.
    pub fn data(&self, plane: usize) -> &[u8] {
        &self.planes[plane].data
    }

    /// All rows of `plane` for writing; a plane shared with a clone is copied
    /// first, so the clone keeps its pixels.
    pub fn data_mut(&mut self, plane: usize) -> &mut [u8] {
        Arc::make_mut(&mut self.planes[plane].data).as_mut_slice()
    }

    pub fn pts(&self) -> Option<i64> {
        self.pts
    }

    pub fn set_pts(&mut self, pts: Option<i64>) {
        self.pts = pts;
    }

    pub fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    pub fn set_color_space(&mut self, space: ColorSpace) {
        self.color_space = space;
    }

    pub fn color_range(&self) -> ColorRange {
        self.color_range
    }

    pub fn set_color_range(&mut self, range: ColorRange) {
        self.color_range = range;
    }

    /// Whether `plane` is shared with another frame (no copy made yet).
    pub fn is_shared(&self, plane: usize) -> bool {
        Arc::strong_count(&self.planes[plane].data) > 1
    }

    /// The same picture as an FFmpeg frame, for modules not moved yet.
    pub fn to_ffmpeg(&self) -> frame::Video {
        let mut target = frame::Video::new(self.format.to_ffmpeg(), self.width, self.height);

        for plane in 0..self.planes() {
            let row_bytes = self.plane_row_bytes(plane);
            let source_stride = self.stride(plane);
            let target_stride = target.stride(plane);
            let source = self.data(plane);
            let destination = target.data_mut(plane);

            for row in 0..self.plane_height(plane) {
                destination[row * target_stride..row * target_stride + row_bytes]
                    .copy_from_slice(&source[row * source_stride..row * source_stride + row_bytes]);
            }
        }

        target.set_pts(self.pts);
        target.set_color_space(self.color_space.to_ffmpeg());
        target.set_color_range(self.color_range.to_ffmpeg());

        target
    }
}

impl TryFrom<&frame::Video> for VideoFrame {
    type Error = anyhow::Error;

    fn try_from(source: &frame::Video) -> Result<Self> {
        let format = PixelFormat::from_ffmpeg(source.format())?;
        let mut target = Self::new(format, source.width(), source.height());

        for plane in 0..target.planes() {
            let row_bytes = target.plane_row_bytes(plane);
            let rows = target.plane_height(plane);
            let source_stride = source.stride(plane);
            let target_stride = target.stride(plane);
            let pixels = source.data(plane);
            let destination = target.data_mut(plane);

            for row in 0..rows {
                destination[row * target_stride..row * target_stride + row_bytes]
                    .copy_from_slice(&pixels[row * source_stride..row * source_stride + row_bytes]);
            }
        }

        target.pts = source.pts();
        target.color_space = ColorSpace::from_ffmpeg(source.color_space());
        target.color_range = ColorRange::from_ffmpeg(source.color_range());

        Ok(target)
    }
}

/// Planar 32-bit float audio: one plane per channel.
#[derive(Debug, Clone)]
pub struct AudioFrame {
    rate: u32,
    samples: usize,
    planes: Vec<Arc<Vec<f32>>>,
    pts: Option<i64>,
}

impl AudioFrame {
    /// `samples` of silence on each of `channels` channels.
    pub fn new(samples: usize, channels: usize) -> Self {
        Self {
            rate: 0,
            samples,
            planes: (0..channels)
                .map(|_| Arc::new(vec![0.0; samples]))
                .collect(),
            pts: None,
        }
    }

    /// Stereo, the layout of the whole engine.
    pub fn stereo(samples: usize) -> Self {
        Self::new(samples, 2)
    }

    pub fn empty() -> Self {
        Self::new(0, 0)
    }

    pub fn samples(&self) -> usize {
        self.samples
    }

    pub fn channels(&self) -> usize {
        self.planes.len()
    }

    /// One plane per channel.
    pub fn planes(&self) -> usize {
        self.planes.len()
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: u32) {
        self.rate = rate;
    }

    pub fn plane(&self, channel: usize) -> &[f32] {
        &self.planes[channel]
    }

    /// `channel` for writing; a plane shared with a clone is copied first.
    pub fn plane_mut(&mut self, channel: usize) -> &mut [f32] {
        Arc::make_mut(&mut self.planes[channel]).as_mut_slice()
    }

    pub fn pts(&self) -> Option<i64> {
        self.pts
    }

    pub fn set_pts(&mut self, pts: Option<i64>) {
        self.pts = pts;
    }

    pub fn is_shared(&self, channel: usize) -> bool {
        Arc::strong_count(&self.planes[channel]) > 1
    }

    /// The same audio as an FFmpeg frame, for modules not moved yet.
    pub fn to_ffmpeg(&self) -> Result<frame::Audio> {
        let layout = match self.channels() {
            1 => ChannelLayout::MONO,
            2 => ChannelLayout::STEREO,
            other => bail!("{other} audio channels have no FFmpeg layout here"),
        };
        let mut target = frame::Audio::new(
            format::Sample::F32(format::sample::Type::Planar),
            self.samples,
            layout,
        );

        for channel in 0..self.channels() {
            target
                .plane_mut::<f32>(channel)
                .copy_from_slice(self.plane(channel));
        }

        target.set_rate(self.rate);
        target.set_pts(self.pts);

        Ok(target)
    }
}

impl TryFrom<&frame::Audio> for AudioFrame {
    type Error = anyhow::Error;

    fn try_from(source: &frame::Audio) -> Result<Self> {
        if source.format() != format::Sample::F32(format::sample::Type::Planar) {
            bail!("audio format {:?} is not planar f32", source.format());
        }

        let mut target = Self::new(source.samples(), usize::from(source.channels()));

        for channel in 0..target.channels() {
            target
                .plane_mut(channel)
                .copy_from_slice(source.plane::<f32>(channel));
        }

        target.rate = source.rate();
        target.pts = source.pts();

        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterned(format: PixelFormat, width: u32, height: u32) -> VideoFrame {
        let mut frame = VideoFrame::new(format, width, height);

        for plane in 0..frame.planes() {
            let stride = frame.stride(plane);
            let row_bytes = frame.plane_row_bytes(plane);

            for row in 0..frame.plane_height(plane) {
                for column in 0..row_bytes {
                    frame.data_mut(plane)[row * stride + column] =
                        (plane * 61 + row * 7 + column * 3) as u8;
                }
            }
        }

        frame.set_pts(Some(1234));

        frame
    }

    fn visible_rows(frame: &VideoFrame, plane: usize) -> Vec<Vec<u8>> {
        let stride = frame.stride(plane);
        let row_bytes = frame.plane_row_bytes(plane);

        (0..frame.plane_height(plane))
            .map(|row| frame.data(plane)[row * stride..row * stride + row_bytes].to_vec())
            .collect()
    }

    #[test]
    fn plane_geometry_matches_ffmpeg_for_odd_and_even_sizes() {
        ffmpeg_next::init().ok();

        for format in [
            PixelFormat::Yuv420p,
            PixelFormat::Yuva420p,
            PixelFormat::Bgrx,
        ] {
            for (width, height) in [(4, 2), (5, 3), (1280, 720), (1919, 1081)] {
                let ours = VideoFrame::new(format, width, height);
                let theirs = frame::Video::new(format.to_ffmpeg(), width, height);

                assert_eq!(ours.planes(), theirs.planes(), "{format:?}");

                for plane in 0..ours.planes() {
                    assert_eq!(
                        ours.plane_height(plane),
                        theirs.plane_height(plane) as usize,
                        "{format:?} {width}x{height} plane {plane} rows"
                    );
                    assert!(ours.stride(plane) >= ours.plane_row_bytes(plane));
                    assert_eq!(
                        ours.data(plane).len(),
                        ours.stride(plane) * ours.plane_height(plane)
                    );
                }
            }
        }
    }

    #[test]
    fn video_survives_a_round_trip_through_ffmpeg_unchanged() {
        ffmpeg_next::init().ok();

        for format in [
            PixelFormat::Yuv420p,
            PixelFormat::Yuva420p,
            PixelFormat::Bgrx,
        ] {
            let mut original = patterned(format, 7, 5);
            original.set_color_space(ColorSpace::Bt709);
            original.set_color_range(ColorRange::Limited);

            let back = VideoFrame::try_from(&original.to_ffmpeg()).unwrap();

            assert_eq!(back.format(), format);
            assert_eq!((back.width(), back.height()), (7, 5));
            assert_eq!(back.pts(), Some(1234));
            assert_eq!(back.color_space(), ColorSpace::Bt709);
            assert_eq!(back.color_range(), ColorRange::Limited);

            for plane in 0..original.planes() {
                assert_eq!(visible_rows(&back, plane), visible_rows(&original, plane));
            }
        }
    }

    #[test]
    fn a_clone_shares_pixels_until_one_side_writes() {
        let source = patterned(PixelFormat::Yuv420p, 4, 4);
        let mut copy = source.clone();

        assert!(copy.is_shared(0));
        assert_eq!(copy.data(0).as_ptr(), source.data(0).as_ptr());

        copy.data_mut(0)[0] = 42;

        assert_ne!(copy.data(0).as_ptr(), source.data(0).as_ptr());
        assert_eq!(source.data(0)[0], 0);
        assert_eq!(copy.data(0)[0], 42);
        // Planes not written stay shared.
        assert!(copy.is_shared(1));
    }

    #[test]
    fn audio_survives_a_round_trip_through_ffmpeg_unchanged() {
        ffmpeg_next::init().ok();

        let mut original = AudioFrame::stereo(1920);
        for channel in 0..2 {
            for (index, sample) in original.plane_mut(channel).iter_mut().enumerate() {
                *sample = (index as f32 * 0.001 + channel as f32).sin();
            }
        }
        original.set_rate(48_000);
        original.set_pts(Some(96_000));

        let back = AudioFrame::try_from(&original.to_ffmpeg().unwrap()).unwrap();

        assert_eq!(back.samples(), 1920);
        assert_eq!(back.channels(), 2);
        assert_eq!(back.rate(), 48_000);
        assert_eq!(back.pts(), Some(96_000));
        assert_eq!(back.plane(0), original.plane(0));
        assert_eq!(back.plane(1), original.plane(1));
    }

    #[test]
    fn an_audio_clone_copies_a_channel_only_when_written() {
        let mut source = AudioFrame::stereo(4);
        source.plane_mut(0).fill(0.5);
        let mut copy = source.clone();

        copy.plane_mut(0)[0] = -1.0;

        assert_eq!(source.plane(0)[0], 0.5);
        assert_eq!(copy.plane(0)[0], -1.0);
        assert!(copy.is_shared(1));
    }

    #[test]
    fn formats_outside_the_engine_are_refused() {
        ffmpeg_next::init().ok();

        let rgb = frame::Video::new(format::Pixel::RGB24, 4, 4);
        assert!(VideoFrame::try_from(&rgb).is_err());

        let packed = frame::Audio::new(
            format::Sample::I16(format::sample::Type::Packed),
            16,
            ChannelLayout::STEREO,
        );
        assert!(AudioFrame::try_from(&packed).is_err());
    }
}
