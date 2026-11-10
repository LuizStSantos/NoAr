//! Bounded desktop recording worker. Encoding runs outside the UI/decode
//! threads; channel capacity and shutdown behavior stay owned together.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    thread as std_thread,
};

use anyhow::{Result, anyhow};

use crate::{
    compositor::graphics::logo::{LogoOverlay, blend_logo},
    frame::{AudioFrame, VideoFrame},
    utils::config::OutputConfig,
};

use super::super::EncodedOutput;

pub(super) const RECORDING_CHANNEL_CAPACITY: usize = 64;

pub(super) struct DesktopRecording {
    pub(super) sender: SyncSender<DesktopRecordingMessage>,
    worker: std_thread::JoinHandle<()>,
    pub(super) active: Arc<AtomicBool>,
    pub(super) queue_depth: Arc<AtomicUsize>,
    channel_id: i32,
}

pub(super) enum DesktopRecordingMessage {
    Video {
        frame: VideoFrame,
        logo: Option<Arc<LogoOverlay>>,
        logo_opacity: f64,
    },
    Audio(AudioFrame),
    AudioPadding {
        pts: i64,
        samples: u64,
    },
    Finish,
}

pub(super) fn recording_silence(
    pts: i64,
    samples: u64,
    rate: u32,
) -> impl Iterator<Item = AudioFrame> {
    (0..samples).step_by(1_024).map(move |offset| {
        let count = (samples - offset).min(1_024) as usize;
        let mut frame = AudioFrame::stereo(count);
        frame.set_rate(rate);
        frame.set_pts(Some(pts + offset as i64));

        for channel in 0..2 {
            frame.plane_mut(channel).fill(0.0);
        }
        frame
    })
}

impl DesktopRecording {
    pub(super) fn open(
        cfg: &OutputConfig,
        recording_config: &crate::RecordingConfig,
    ) -> Result<Self> {
        let (sender, receiver) = sync_channel(RECORDING_CHANNEL_CAPACITY);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let cfg = cfg.clone();
        let recording_config = recording_config.clone();
        let channel_id = cfg.channel_id.unwrap_or_default();
        let active = Arc::new(AtomicBool::new(true));
        let worker_active = Arc::clone(&active);
        let queue_depth = Arc::new(AtomicUsize::new(0));
        let worker_queue_depth = Arc::clone(&queue_depth);
        let worker = std_thread::Builder::new()
            .name("noar-desktop-recording".to_string())
            .spawn(move || {
                let mut output = match EncodedOutput::open_recording(&cfg, &recording_config) {
                    Ok(output) => {
                        let _ = ready_sender.send(Ok(()));
                        output
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.to_string()));

                        return;
                    }
                };

                while let Ok(message) = receiver.recv() {
                    let result = match message {
                        DesktopRecordingMessage::Video {
                            mut frame,
                            logo,
                            logo_opacity,
                        } => {
                            worker_queue_depth.fetch_sub(1, Ordering::AcqRel);

                            if let Some(logo) = logo {
                                // A referenced frame is read-only across threads. Make the
                                // full copy in this worker only when compositing is required.
                                let mut composited = frame.clone();
                                blend_logo(&mut composited, &logo, logo_opacity);
                                frame = composited;
                            }
                            output.encode_video(&frame)
                        }
                        DesktopRecordingMessage::Audio(frame) => {
                            worker_queue_depth.fetch_sub(1, Ordering::AcqRel);
                            output.encode_processed_audio(&frame)
                        }
                        DesktopRecordingMessage::AudioPadding { pts, samples } => {
                            worker_queue_depth.fetch_sub(1, Ordering::AcqRel);
                            recording_silence(pts, samples, cfg.sample_rate)
                                .try_for_each(|frame| output.encode_processed_audio(&frame))
                        }
                        DesktopRecordingMessage::Finish => break,
                    };

                    if let Err(error) = result {
                        worker_active.store(false, Ordering::Release);
                        log::error!(channel = channel_id; "Recording disabled: {error}");

                        return;
                    }

                    if !worker_active.load(Ordering::Acquire) {
                        break;
                    }
                }

                if let Err(error) = output.finish() {
                    log::error!(channel = channel_id; "Recording disabled: {error}");
                }
            })
            .map_err(|error| anyhow!("failed to start desktop recording worker: {error}"))?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                sender,
                worker,
                active,
                queue_depth,
                channel_id,
            }),
            Ok(Err(error)) => {
                let _ = worker.join();

                Err(anyhow!(error))
            }
            Err(_) => {
                let _ = worker.join();

                Err(anyhow!(
                    "desktop recording worker stopped during initialization"
                ))
            }
        }
    }

    pub(super) fn finish(self) {
        let Self {
            sender,
            worker,
            active,
            queue_depth: _,
            channel_id,
        } = self;
        // Do not make an output switch wait for decoded frames which have not
        // reached the recording encoder yet. The worker finishes its current
        // message and then writes the trailer.
        active.store(false, Ordering::Release);

        match sender.try_send(DesktopRecordingMessage::Finish) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(_)) => {
                // Dropping the final sender lets the worker drain queued frames
                // before it closes the muxer and writes the trailer.
            }
        }
        drop(sender);

        if worker.join().is_err() {
            log::warn!(channel = channel_id; "desktop recording worker panicked");
        }
    }
}
