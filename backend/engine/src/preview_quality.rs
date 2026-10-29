//! An independently transcoded copy of the program preview for slow clients.
//! Native GStreamer pipelines own demux, decode, scaling, encoding and muxing;
//! a failed or cancelled viewer cannot reach the program's output pipeline.

use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

use ::gstreamer as gst;
use anyhow::{Context, Result, anyhow, ensure};
use gst::prelude::*;
use gstreamer_app as gst_app;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightPreview {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub video_kbps: u32,
}

impl Default for LightPreview {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            fps: 15,
            video_kbps: 500,
        }
    }
}

pub fn transcode_light_preview<F>(
    input: Receiver<Vec<u8>>,
    mut deliver: F,
    settings: LightPreview,
    stop: Arc<AtomicBool>,
) -> Result<()>
where
    F: FnMut(Vec<u8>) -> bool + Send + 'static,
{
    ensure!(
        settings.width >= 2
            && settings.height >= 2
            && settings.width.is_multiple_of(2)
            && settings.height.is_multiple_of(2)
            && settings.fps > 0
            && settings.video_kbps > 0,
        "preview dimensions, frame rate and bitrate must be positive; dimensions must be even"
    );
    crate::gstreamer::init()?;
    let pipeline = gst::Pipeline::new();
    let _shutdown = PipelineShutdown(pipeline.clone());
    let source = gst_app::AppSrc::builder()
        .caps(
            &gst::Caps::builder("video/mpegts")
                .field("systemstream", true)
                .field("packetsize", 188_i32)
                .build(),
        )
        .format(gst::Format::Bytes)
        .max_bytes(1024 * 1024)
        .build();
    let decoder = gst::ElementFactory::make("decodebin").build()?;
    let mux = gst::ElementFactory::make("mpegtsmux")
        .property("alignment", 7_i32)
        .build()?;
    let sink = gst_app::AppSink::builder()
        .max_buffers(8)
        .sync(false)
        .build();
    pipeline.add_many([source.upcast_ref(), &decoder, &mux, sink.upcast_ref()])?;
    source.link(&decoder)?;
    mux.link(&sink)?;

    let source_stop = stop.clone();
    source.set_callbacks(
        gst_app::AppSrcCallbacks::builder()
            .need_data(move |source, _| {
                while !source_stop.load(Ordering::Acquire) {
                    match input.recv_timeout(Duration::from_millis(100)) {
                        Ok(bytes) if bytes.is_empty() => continue,
                        Ok(bytes) => {
                            if source
                                .push_buffer(gst::Buffer::from_mut_slice(bytes))
                                .is_err()
                            {
                                source_stop.store(true, Ordering::Release);
                            }
                            return;
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => {
                            let _ = source.end_of_stream();
                            return;
                        }
                    }
                }
                let _ = source.end_of_stream();
            })
            .build(),
    );

    let link_error = Arc::new(Mutex::new(None));
    let worker_error = link_error.clone();
    let continuity = Arc::new(Mutex::new(Continuity::default()));
    let worker_pipeline = pipeline.downgrade();
    let video_linked = Arc::new(AtomicBool::new(false));
    let video_linked_worker = video_linked.clone();
    let audio_linked = AtomicBool::new(false);
    decoder.connect_pad_added(move |_, pad| {
        let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
        let name = caps.structure(0).map(|structure| structure.name().to_string()).unwrap_or_default();
        let branch = if name.starts_with("video/") && !video_linked_worker.swap(true, Ordering::AcqRel) {
            Some(format!("queue ! identity name=preview-time single-segment=true ! videoconvert ! videoscale add-borders=true ! videorate ! video/x-raw,format=I420,width={},height={},framerate={}/1,pixel-aspect-ratio=1/1 ! x264enc tune=zerolatency speed-preset=veryfast bitrate={} key-int-max={} bframes=0 ! h264parse", settings.width, settings.height, settings.fps, settings.video_kbps, settings.fps))
        } else if name.starts_with("audio/") && !audio_linked.swap(true, Ordering::AcqRel) {
            Some("queue ! identity name=preview-time single-segment=true ! audioconvert ! audioresample ! audio/x-raw,rate=48000,channels=2 ! avenc_aac bitrate=96000 ! aacparse".to_owned())
        } else { None };
        let Some(branch) = branch else { return; };
        let result = (|| -> Result<()> {
            let pipeline = worker_pipeline.upgrade().context("preview pipeline closed")?;
            let branch = gst::parse::bin_from_description(&branch, true)?;
            let time = branch.by_name("preview-time").context("preview continuity element")?;
            let continuity = continuity.clone();
            time.static_pad("src").context("preview continuity pad")?.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
                if let Some(buffer) = info.buffer_mut() {
                    continuity.lock().unwrap_or_else(PoisonError::into_inner).carry_on(buffer.make_mut());
                }
                gst::PadProbeReturn::Ok
            });
            pipeline.add(&branch)?;
            branch.link(&mux)?;
            pad.link(&branch.static_pad("sink").context("preview branch sink")?)?;
            branch.sync_state_with_parent()?;
            Ok(())
        })();
        if let Err(error) = result {
            *worker_error.lock().unwrap_or_else(PoisonError::into_inner) = Some(format!("{error:#}"));
        }
    });
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("preview bus")?;
    while !stop.load(Ordering::Acquire) {
        if let Some(error) = link_error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(anyhow!("linking light preview: {error}"));
        }
        if let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_mseconds(100)) {
            let buffer = sample.buffer().context("preview output buffer")?;
            let map = buffer.map_readable()?;
            if !map.as_slice().is_empty() && !deliver(map.as_slice().to_vec()) {
                stop.store(true, Ordering::Release);
                break;
            }
        } else if sink.is_eos() {
            ensure!(
                video_linked.load(Ordering::Acquire),
                "the preview has no video"
            );
            break;
        }
        for message in bus.iter() {
            if let gst::MessageView::Error(error) = message.view() {
                return Err(anyhow!(
                    "GStreamer preview failed: {} ({:?})",
                    error.error(),
                    error.debug()
                ));
            }
        }
    }
    Ok(())
}

struct PipelineShutdown(gst::Pipeline);

impl Drop for PipelineShutdown {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

/// The shared offset keeps audio and video together across a program restart.
#[derive(Debug, Default)]
struct Continuity {
    shift: u64,
    latest: Option<u64>,
}

impl Continuity {
    const JUMP: u64 = 1_000_000_000;
    const GAP: u64 = 40_000_000;

    fn carry_on(&mut self, buffer: &mut gst::BufferRef) {
        let Some(stamp) = buffer
            .dts()
            .or(buffer.pts())
            .map(gstreamer::ClockTime::nseconds)
        else {
            return;
        };
        let at = stamp.saturating_add(self.shift);
        if let Some(latest) = self.latest
            && at.saturating_add(Self::JUMP) < latest
        {
            self.shift = self
                .shift
                .saturating_add(latest - at)
                .saturating_add(Self::GAP);
        }
        if self.shift != 0 {
            buffer.set_pts(buffer.pts().map(|time| {
                gst::ClockTime::from_nseconds(time.nseconds().saturating_add(self.shift))
            }));
            buffer.set_dts(buffer.dts().map(|time| {
                gst::ClockTime::from_nseconds(time.nseconds().saturating_add(self.shift))
            }));
        }
        let at = stamp.saturating_add(self.shift);
        self.latest = Some(self.latest.map_or(at, |latest| latest.max(at)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(milliseconds: u64) -> gst::Buffer {
        crate::gstreamer::init().unwrap();
        let mut buffer = gst::Buffer::new();
        buffer
            .get_mut()
            .unwrap()
            .set_pts(gst::ClockTime::from_mseconds(milliseconds));
        buffer
    }

    #[test]
    fn a_new_start_follows_on_from_the_last_packet() {
        let mut continuity = Continuity::default();
        let mut last = buffer(600_000);
        continuity.carry_on(last.make_mut());
        assert_eq!(last.pts(), Some(gst::ClockTime::from_seconds(600)));
        let mut reordered = buffer(599_900);
        continuity.carry_on(reordered.make_mut());
        assert_eq!(
            reordered.pts(),
            Some(gst::ClockTime::from_mseconds(599_900))
        );
        let mut sound = buffer(0);
        continuity.carry_on(sound.make_mut());
        let mut picture = buffer(0);
        continuity.carry_on(picture.make_mut());
        assert_eq!(sound.pts(), Some(gst::ClockTime::from_mseconds(600_040)));
        assert_eq!(picture.pts(), sound.pts());
    }
}
