//! Frames waiting for a local output (NDI, SDI) that falls behind for a
//! moment: a short stall drops frames instead of taking the channel off air,
//! and only an output that stops taking frames for [`STALL_LIMIT`] fails.

use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};

use crate::utils::config::OutputConfig;

/// How long a local output may take no frames before the playout fails and
/// restarts it.
pub(super) const STALL_LIMIT: Duration = Duration::from_secs(5);

/// Video a local output may fall behind by before frames are dropped: one
/// second of I420 frames, and never less than the former fixed 32 MiB.
pub(super) fn video_backlog_bytes(cfg: &OutputConfig) -> u64 {
    let frame = u64::from(cfg.width) * u64::from(cfg.height) * 3 / 2;

    (frame * u64::from(cfg.fps.max(1))).max(32 * 1024 * 1024)
}

/// What a local output has queued and not taken yet, for one kind of buffer.
pub(super) struct Backlog {
    output: &'static str,
    kind: &'static str,
    limit: u64,
    /// Since when the output is behind, and the buffers dropped since.
    behind: Mutex<Option<(Instant, u64)>>,
}

impl Backlog {
    pub(super) fn new(output: &'static str, kind: &'static str, limit: u64) -> Self {
        Self {
            output,
            kind,
            limit,
            behind: Mutex::new(None),
        }
    }

    pub(super) fn limit(&self) -> u64 {
        self.limit
    }

    /// Whether to queue the next buffer, given the bytes already queued.
    /// `false` drops it while the output catches up; an error means it has
    /// taken nothing for [`STALL_LIMIT`].
    pub(super) fn admit(&self, queued: u64) -> Result<bool> {
        let mut behind = self.behind.lock().unwrap_or_else(PoisonError::into_inner);

        if queued < self.limit {
            if let Some((since, dropped)) = behind.take() {
                log::info!(
                    "{} output caught up after {:.1} s; {dropped} {} buffers dropped",
                    self.output,
                    since.elapsed().as_secs_f64(),
                    self.kind
                );
            }
            return Ok(true);
        }

        let (since, dropped) = behind.get_or_insert_with(|| {
            log::warn!(
                "{} output is falling behind; dropping {} until it catches up",
                self.output,
                self.kind
            );
            (Instant::now(), 0)
        });
        ensure!(
            since.elapsed() < STALL_LIMIT,
            "{} output stopped consuming {}",
            self.output,
            self.kind
        );
        *dropped += 1;

        Ok(false)
    }
}

/// A real pipeline whose output stopped taking frames, for the tests of the
/// encoders and of the NDI and SDI sinks: paused, its sink holds the first
/// frame and takes no more, so the frames pushed after it wait in the source.
#[cfg(test)]
pub(super) mod stalled {
    use ::gstreamer::{self as gst, prelude::*};
    use gstreamer_app::AppSrc;

    pub(in crate::output) const FRAME: u64 = 64 * 36 * 3 / 2;

    pub(in crate::output) struct StalledOutput {
        pub pipeline: gst::Pipeline,
        pub source: AppSrc,
        pub sink: gst::Element,
    }

    impl StalledOutput {
        /// The source caps its queue at `limit` bytes, like the outputs do.
        pub(in crate::output) fn new(limit: u64) -> Self {
            crate::gstreamer::init().unwrap();
            let pipeline = gst::Pipeline::new();
            let source = AppSrc::builder()
                .format(gst::Format::Time)
                .max_bytes(limit)
                .caps(
                    &gst::Caps::builder("video/x-raw")
                        .field("format", "I420")
                        .field("width", 64)
                        .field("height", 36)
                        .field("framerate", gst::Fraction::new(25, 1))
                        .build(),
                )
                .build();
            let sink = gst::ElementFactory::make("fakesink")
                .property("sync", true)
                .build()
                .unwrap();
            pipeline.add_many([source.upcast_ref(), &sink]).unwrap();
            source.link(&sink).unwrap();
            pipeline.set_state(gst::State::Paused).unwrap();
            // The sink holds this frame: from now on it takes no other.
            source.push_buffer(frame(0)).unwrap();
            let (prerolled, _, _) = pipeline.state(gst::ClockTime::from_seconds(5));
            prerolled.expect("the stalled output did not hold its first frame");

            Self {
                pipeline,
                source,
                sink,
            }
        }

        /// The output takes frames again; waits until the queue is empty.
        pub(in crate::output) fn resume(&self) {
            self.sink.set_property("sync", false);
            self.pipeline.set_state(gst::State::Playing).unwrap();
            let started = std::time::Instant::now();
            while self.source.current_level_bytes() > 0 {
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(5),
                    "the output did not catch up"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }

    impl Drop for StalledOutput {
        fn drop(&mut self) {
            let _ = self.pipeline.set_state(gst::State::Null);
        }
    }

    pub(in crate::output) fn frame(index: u64) -> gst::Buffer {
        let mut buffer = gst::Buffer::from_mut_slice(vec![16_u8; FRAME as usize]);
        let writable = buffer.get_mut().unwrap();
        writable.set_pts(gst::ClockTime::from_mseconds(index * 40));
        writable.set_duration(gst::ClockTime::from_mseconds(40));
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::{
        stalled::{FRAME, StalledOutput, frame},
        *,
    };

    /// The NDI and SDI sinks queue a frame only when [`Backlog::admit`]
    /// says so; here against a real output that stopped taking frames.
    #[test]
    fn a_stalled_local_output_drops_frames_without_blocking_and_catches_up() {
        let backlog = Backlog::new("NDI", "video", 10 * FRAME);
        let output = StalledOutput::new(backlog.limit());
        let started = Instant::now();
        let mut queued = 0;

        for index in 1..=100 {
            if backlog.admit(output.source.current_level_bytes()).unwrap() {
                output.source.push_buffer(frame(index)).unwrap();
                queued += 1;
            }
        }

        // No frame waited for the output, the queue stayed at the limit and
        // the rest were dropped.
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(output.source.current_level_bytes() <= backlog.limit());
        assert_eq!(queued, 10, "frames queued");
        assert!(backlog.behind.lock().unwrap().is_some());

        // Taking frames again, the output gets them and is no longer behind.
        output.resume();
        assert!(backlog.admit(output.source.current_level_bytes()).unwrap());
        assert!(backlog.behind.lock().unwrap().is_none());
    }

    #[test]
    fn a_short_stall_drops_buffers_and_recovers() {
        let backlog = Backlog::new("NDI", "video", 100);

        assert!(backlog.admit(10).unwrap());
        assert!(!backlog.admit(100).unwrap());
        assert!(!backlog.admit(150).unwrap());
        assert!(backlog.admit(50).unwrap());
        assert!(backlog.behind.lock().unwrap().is_none());
    }

    #[test]
    fn an_output_that_takes_nothing_for_the_limit_fails() {
        let backlog = Backlog::new("NDI", "video", 100);

        assert!(!backlog.admit(100).unwrap());
        *backlog.behind.lock().unwrap() = Some((Instant::now() - STALL_LIMIT, 150));
        let error = backlog.admit(100).unwrap_err();

        assert_eq!(error.to_string(), "NDI output stopped consuming video");
    }

    #[test]
    fn the_video_backlog_holds_a_second_of_frames() {
        let mut cfg = OutputConfig::new(1920, 1080, 30, 48_000);

        assert_eq!(video_backlog_bytes(&cfg), 1920 * 1080 * 3 / 2 * 30);
        cfg = OutputConfig::new(64, 36, 25, 48_000);
        assert_eq!(video_backlog_bytes(&cfg), 32 * 1024 * 1024);
    }
}
