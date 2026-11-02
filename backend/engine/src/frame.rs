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

use std::sync::Arc;

/// Row starts are aligned to 32 bytes, for vectorised loops.
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

/// The colour primaries a picture was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorPrimaries {
    #[default]
    Unspecified,
    Bt709,
    Bt2020,
    Other,
}

/// The transfer function a picture was made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorTransfer {
    #[default]
    Unspecified,
    /// SMPTE ST 2084 (PQ), HDR10.
    Pq,
    /// ARIB STD-B67 (HLG).
    Hlg,
    Linear,
    /// IEC 61966-2-1 (sRGB).
    Srgb,
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
    color_primaries: ColorPrimaries,
    color_transfer: ColorTransfer,
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
            color_primaries: ColorPrimaries::default(),
            color_transfer: ColorTransfer::default(),
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

    /// Pixels across `plane` (half the width for chroma planes).
    pub fn plane_width(&self, plane: usize) -> u32 {
        let row_bytes = self.plane_row_bytes(plane);
        let pixels = if self.format == PixelFormat::Bgrx {
            row_bytes / 4
        } else {
            row_bytes
        };

        pixels as u32
    }

    /// Rows in `plane`.
    pub fn plane_height(&self, plane: usize) -> u32 {
        self.format.plane_size(plane, self.width, self.height).1 as u32
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

    pub fn color_primaries(&self) -> ColorPrimaries {
        self.color_primaries
    }

    pub fn set_color_primaries(&mut self, primaries: ColorPrimaries) {
        self.color_primaries = primaries;
    }

    pub fn color_transfer(&self) -> ColorTransfer {
        self.color_transfer
    }

    pub fn set_color_transfer(&mut self, transfer: ColorTransfer) {
        self.color_transfer = transfer;
    }

    /// Whether `plane` is shared with another frame (no copy made yet).
    pub fn is_shared(&self, plane: usize) -> bool {
        Arc::strong_count(&self.planes[plane].data) > 1
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterned(format: PixelFormat, width: u32, height: u32) -> VideoFrame {
        let mut frame = VideoFrame::new(format, width, height);

        for plane in 0..frame.planes() {
            let stride = frame.stride(plane);
            let row_bytes = frame.plane_row_bytes(plane);

            for row in 0..frame.plane_height(plane) as usize {
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

        (0..frame.plane_height(plane) as usize)
            .map(|row| frame.data(plane)[row * stride..row * stride + row_bytes].to_vec())
            .collect()
    }

    #[test]
    fn plane_sizes_follow_each_format() {
        // Width x height of each plane, in pixels, for a 1919x1081 picture.
        for (format, planes) in [
            (
                PixelFormat::Yuv420p,
                vec![(1919, 1081), (960, 541), (960, 541)],
            ),
            (
                PixelFormat::Yuva420p,
                vec![(1919, 1081), (960, 541), (960, 541), (1919, 1081)],
            ),
            (PixelFormat::Bgrx, vec![(1919, 1081)]),
        ] {
            let frame = VideoFrame::new(format, 1919, 1081);

            assert_eq!(frame.planes(), planes.len(), "{format:?}");

            for (plane, (width, height)) in planes.into_iter().enumerate() {
                assert_eq!(
                    (frame.plane_width(plane), frame.plane_height(plane)),
                    (width, height),
                    "{format:?} plane {plane}"
                );
                assert!(frame.stride(plane) >= frame.plane_row_bytes(plane));
                assert_eq!(frame.stride(plane) % ROW_ALIGNMENT, 0);
                assert_eq!(
                    frame.data(plane).len(),
                    frame.stride(plane) * height as usize
                );
            }
        }

        assert_eq!(
            VideoFrame::new(PixelFormat::Bgrx, 5, 3).plane_row_bytes(0),
            20
        );
    }

    #[test]
    fn a_frame_keeps_its_timestamp_and_colour_description() {
        let mut frame = patterned(PixelFormat::Yuv420p, 7, 5);
        frame.set_color_space(ColorSpace::Bt709);
        frame.set_color_range(ColorRange::Limited);
        frame.set_color_primaries(ColorPrimaries::Bt2020);
        frame.set_color_transfer(ColorTransfer::Pq);
        let copy = frame.clone();

        assert_eq!(copy.pts(), Some(1234));
        assert_eq!(copy.color_space(), ColorSpace::Bt709);
        assert_eq!(copy.color_range(), ColorRange::Limited);
        assert_eq!(copy.color_primaries(), ColorPrimaries::Bt2020);
        assert_eq!(copy.color_transfer(), ColorTransfer::Pq);

        for plane in 0..frame.planes() {
            assert_eq!(visible_rows(&copy, plane), visible_rows(&frame, plane));
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
    fn audio_keeps_its_rate_timestamp_and_samples() {
        let mut audio = AudioFrame::stereo(1920);
        for channel in 0..2 {
            for (index, sample) in audio.plane_mut(channel).iter_mut().enumerate() {
                *sample = (index as f32 * 0.001 + channel as f32).sin();
            }
        }
        audio.set_rate(48_000);
        audio.set_pts(Some(96_000));
        let copy = audio.clone();

        assert_eq!((copy.samples(), copy.channels()), (1920, 2));
        assert_eq!(copy.rate(), 48_000);
        assert_eq!(copy.pts(), Some(96_000));
        assert_eq!(copy.plane(1), audio.plane(1));
        assert_eq!(AudioFrame::new(16, 1).channels(), 1);
        assert_eq!(AudioFrame::empty().samples(), 0);
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
}
