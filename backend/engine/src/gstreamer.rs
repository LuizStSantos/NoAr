//! Shared initialization of the single native media runtime.

use std::sync::OnceLock;

use ::gstreamer::{self as gst, prelude::*};
use anyhow::{Context, Result, anyhow, ensure};
use ffmpeg_next::{frame, util::format::pixel::Pixel};
use gstreamer_app::AppSink;
use gstreamer_video::{VideoConverter, VideoFormat, VideoFrameExt, VideoFrameRef, VideoInfo};

/// Initialize once before constructing native pipelines on any worker thread.
pub(crate) fn init() -> Result<()> {
    static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

    INITIALIZED
        .get_or_init(|| ::gstreamer::init().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|error| anyhow!("initializing GStreamer: {error}"))?;

    Ok(())
}

fn video_info(format: Pixel, width: u32, height: u32) -> Result<VideoInfo> {
    let format = match format {
        Pixel::YUV420P => VideoFormat::I420,
        Pixel::YUVA420P => VideoFormat::A420,
        Pixel::BGRZ => VideoFormat::Bgrx,
        _ => return Err(anyhow!("unsupported CPU frame format {format:?}")),
    };

    Ok(VideoInfo::builder(format, width, height).build()?)
}

/// Native color conversion/scaling for the existing CPU graphics interchange.
pub(crate) struct FrameConverter {
    input: VideoInfo,
    output: VideoInfo,
    converter: VideoConverter,
    format: Pixel,
}

impl FrameConverter {
    #[cfg(feature = "desktop-base")]
    pub(crate) fn accepts(&self, frame: &frame::Video) -> bool {
        video_info(frame.format(), frame.width(), frame.height())
            .is_ok_and(|info| info == self.input)
    }

    pub(crate) fn new(
        input: &frame::Video,
        format: Pixel,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        init()?;
        let source = video_info(input.format(), input.width(), input.height())?;
        let output = video_info(format, width, height)?;
        let converter = VideoConverter::new(&source, &output, None)?;

        Ok(Self {
            input: source,
            output,
            converter,
            format,
        })
    }

    pub(crate) fn run(&self, input: &frame::Video) -> Result<frame::Video> {
        crate::benchmark::measure_success(crate::benchmark::Stage::Scale, || self.convert(input))
    }

    fn convert(&self, input: &frame::Video) -> Result<frame::Video> {
        ensure!(
            input.width() == self.input.width() && input.height() == self.input.height(),
            "CPU graphics input size changed without reconfiguring the converter"
        );
        let mut source = gst::Buffer::with_size(self.input.size())?;
        {
            let writable = source
                .get_mut()
                .context("graphics source buffer is shared")?;
            let mut mapped = VideoFrameRef::from_buffer_ref_writable(writable, &self.input)?;
            let strides = mapped.plane_stride().to_vec();

            for (plane, stride) in strides.iter().enumerate().take(input.planes()) {
                let width = plane_bytes(input, plane);
                let height = input.plane_height(plane) as usize;
                let source_stride = input.stride(plane);
                let target_stride = usize::try_from(*stride)?;
                let target = mapped.plane_data_mut(plane as u32)?;

                for row in 0..height {
                    target[row * target_stride..row * target_stride + width].copy_from_slice(
                        &input.data(plane)[row * source_stride..row * source_stride + width],
                    );
                }
            }
        }
        let mut destination = gst::Buffer::with_size(self.output.size())?;
        {
            let source = VideoFrameRef::from_buffer_ref_readable(&source, &self.input)?;
            let mut target = VideoFrameRef::from_buffer_ref_writable(
                destination
                    .get_mut()
                    .context("graphics destination buffer is shared")?,
                &self.output,
            )?;
            self.converter.frame_ref(&source, &mut target);
        }
        let mut output = copy_sample_frame(&destination, &self.output, self.format)?;
        output.set_pts(input.pts());

        Ok(output)
    }
}

fn copy_sample_frame(
    buffer: &gst::BufferRef,
    info: &VideoInfo,
    format: Pixel,
) -> Result<frame::Video> {
    let mapped = VideoFrameRef::from_buffer_ref_readable(buffer, info)?;
    let mut output = frame::Video::new(format, info.width(), info.height());

    for plane in 0..output.planes() {
        let width = plane_bytes(&output, plane);
        let height = output.plane_height(plane) as usize;
        let source_stride = usize::try_from(mapped.plane_stride()[plane])?;
        let target_stride = output.stride(plane);
        let source = mapped.plane_data(plane as u32)?;
        let target = output.data_mut(plane);

        for row in 0..height {
            target[row * target_stride..row * target_stride + width]
                .copy_from_slice(&source[row * source_stride..row * source_stride + width]);
        }
    }

    Ok(output)
}

fn plane_bytes(frame: &frame::Video, plane: usize) -> usize {
    let pixels = frame.plane_width(plane) as usize;
    if frame.format() == Pixel::BGRZ {
        pixels * 4
    } else {
        pixels
    }
}

/// Decode a graphic through GStreamer, preserving its alpha plane.
pub(crate) fn load_image(path: &str) -> Result<frame::Video> {
    init()?;
    let pipeline = gst::Pipeline::new();
    let decoder = gst::ElementFactory::make("uridecodebin")
        .property("uri", crate::utils::media_info::media_uri(path)?)
        .build()?;
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
        .drop(true)
        .build();
    pipeline.add_many([&decoder, &convert, sink.upcast_ref()])?;
    convert.link(&sink)?;
    let target = convert
        .static_pad("sink")
        .context("image converter has no input")?;
    decoder.connect_pad_added(move |_, pad| {
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));

        if !target.is_linked()
            && caps
                .structure(0)
                .is_some_and(|s| s.name().starts_with("video/"))
        {
            let _ = pad.link(&target);
        }
    });
    let result = (|| {
        pipeline.set_state(gst::State::Playing)?;
        let sample = sink
            .try_pull_sample(gst::ClockTime::from_seconds(10))
            .with_context(|| format!("GStreamer produced no image for {path}"))?;
        let info = VideoInfo::from_caps(sample.caps().context("image sample has no caps")?)?;
        copy_sample_frame(
            sample.buffer().context("image sample has no data")?,
            &info,
            Pixel::YUVA420P,
        )
    })();
    let _ = pipeline.set_state(gst::State::Null);

    result
}
