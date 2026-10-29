//! Opens native GStreamer sources ahead of their turn.
//!
//! Files remain paused after negotiation. Live sources keep decoding into a
//! bounded recent-frame window, so their signal and handover stay at the live
//! edge without keeping an independent FFmpeg input open.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use log::debug;

use crate::utils::{config::OutputConfig, helper::is_live_input};

use super::gstreamer::{FrameEvent, Source};

const MAX_ENTRIES: usize = 3;
const EXPIRY: Duration = Duration::from_secs(10 * 60);
const PREROLL_SPAN: Duration = Duration::from_millis(300);
const PREROLL_MAX_FRAMES: usize = 128;
const PREROLL_MAX_BYTES: usize = 32 * 1024 * 1024;
const STALL_AFTER: Duration = Duration::from_secs(3);
const IDLE_POLL: Duration = Duration::from_millis(20);

const READING: u8 = 0;
const HANDOVER: u8 = 1;
const CANCELLED: u8 = 2;
const HANDED: u8 = 3;

/// An open native source and the recent decoded frames of a live source.
pub(crate) struct OpenedInput {
    pub(crate) input: Source,
    pub(crate) preroll: VecDeque<FrameEvent>,
}

impl From<Source> for OpenedInput {
    fn from(input: Source) -> Self {
        Self {
            input,
            preroll: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadSignal {
    Opening,
    Ready,
    Stalled,
    Failed(String),
}

#[derive(Debug, Clone)]
enum Status {
    Opening,
    Open { last_frame: Option<Instant> },
    Failed(String),
}

struct Control {
    state: AtomicU8,
    created: Instant,
    live: bool,
    status: Mutex<Status>,
    cancelled: Arc<AtomicBool>,
}

impl Control {
    fn new(live: bool) -> Self {
        Self {
            state: AtomicU8::new(READING),
            created: Instant::now(),
            live,
            status: Mutex::new(Status::Opening),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        if self
            .state
            .compare_exchange(READING, CANCELLED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.cancelled.store(true, Ordering::Release);
        }
    }

    fn request_handover(&self) {
        let _ = self
            .state
            .compare_exchange(READING, HANDOVER, Ordering::AcqRel, Ordering::Acquire);
    }

    fn set_status(&self, status: Status) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    fn signal(&self) -> PreloadSignal {
        match &*self.status.lock().unwrap_or_else(PoisonError::into_inner) {
            Status::Opening => PreloadSignal::Opening,
            Status::Open { .. } if !self.live => PreloadSignal::Ready,
            Status::Open { last_frame } => {
                if last_frame.is_some_and(|at| at.elapsed() < STALL_AFTER) {
                    PreloadSignal::Ready
                } else {
                    PreloadSignal::Stalled
                }
            }
            Status::Failed(error) => PreloadSignal::Failed(error.clone()),
        }
    }

    fn failed(&self) -> bool {
        matches!(self.signal(), PreloadSignal::Failed(_))
    }
}

struct Entry {
    path: String,
    control: Arc<Control>,
    receiver: Receiver<Result<OpenedInput, String>>,
}

#[derive(Default)]
struct Entries(Vec<Entry>);

impl Drop for Entries {
    fn drop(&mut self) {
        for entry in &self.0 {
            entry.control.cancel();
        }
    }
}

pub(crate) struct Preloaded {
    control: Arc<Control>,
    receiver: Receiver<Result<OpenedInput, String>>,
}

impl Preloaded {
    pub(crate) fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.control.cancelled)
    }

    pub(crate) fn hand_over(self) -> Receiver<Result<OpenedInput, String>> {
        self.control.request_handover();

        self.receiver
    }
}

/// The application's shared preloads; configuration follows the owning playout.
#[derive(Clone, Default)]
pub struct Preloader {
    entries: Arc<Mutex<Entries>>,
    config: Arc<Mutex<Option<OutputConfig>>>,
}

impl fmt::Debug for Preloader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        formatter
            .debug_list()
            .entries(entries.0.iter().map(|entry| &entry.path))
            .finish()
    }
}

impl Preloader {
    pub(crate) fn configure(&self, config: &OutputConfig) {
        self.clear();
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = Some(config.clone());
    }

    pub fn preload(&self, path: &str) {
        let path = path.trim();

        if path.is_empty() {
            return;
        }

        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        expire(&mut entries.0);

        if entries
            .0
            .iter()
            .any(|entry| entry.path == path && !entry.control.failed())
        {
            return;
        }

        remove(&mut entries.0, |entry| entry.path == path);

        while entries.0.len() >= MAX_ENTRIES {
            entries.0.remove(0).control.cancel();
        }

        let config = self
            .config
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .unwrap_or_else(|| OutputConfig::new(1280, 720, 25, 48_000));
        debug!("Preloading {path} with GStreamer");
        entries.0.push(spawn(path.to_string(), config));
    }

    pub fn signal(&self, path: &str) -> Option<PreloadSignal> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        entries
            .0
            .iter()
            .find(|entry| entry.path == path.trim())
            .map(|entry| entry.control.signal())
    }

    pub fn forget(&self, path: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        remove(&mut entries.0, |entry| entry.path == path.trim());
    }

    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        remove(&mut entries.0, |_| true);
    }

    pub(crate) fn take(&self, path: &str) -> Option<Preloaded> {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let index = entries
            .0
            .iter()
            .position(|entry| entry.path == path.trim())?;
        let entry = entries.0.remove(index);

        if entry.control.failed() {
            entry.control.cancel();

            return None;
        }

        Some(Preloaded {
            control: entry.control,
            receiver: entry.receiver,
        })
    }
}

fn remove(entries: &mut Vec<Entry>, matches: impl Fn(&Entry) -> bool) {
    entries.retain(|entry| {
        let keep = !matches(entry);

        if !keep {
            entry.control.cancel();
        }

        keep
    });
}

fn expire(entries: &mut Vec<Entry>) {
    remove(entries, |entry| entry.control.created.elapsed() > EXPIRY);
}

fn spawn(path: String, config: OutputConfig) -> Entry {
    let control = Arc::new(Control::new(is_live_input(&path)));
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker_control = Arc::clone(&control);
    let worker_path = path.clone();
    let spawned = thread::Builder::new()
        .name("gst-preload".to_string())
        .spawn(move || run(&worker_path, &config, &worker_control, &sender));

    if let Err(error) = spawned {
        control.set_status(Status::Failed(format!("cannot start the preload: {error}")));
    }

    Entry {
        path,
        control,
        receiver,
    }
}

fn run(
    path: &str,
    config: &OutputConfig,
    control: &Control,
    sender: &SyncSender<Result<OpenedInput, String>>,
) {
    let mut source = match Source::open_with_options_and_cancel(
        path,
        config,
        &BTreeMap::new(),
        &BTreeMap::new(),
        Arc::clone(&control.cancelled),
    ) {
        Ok(source) => source,
        Err(error) => {
            let error = format!("cannot open {path}: {error:#}");
            control.set_status(Status::Failed(error.clone()));
            let _ = sender.send(Err(error));

            return;
        }
    };
    if control.live {
        if let Err(error) = source.start(None) {
            let error = format!("cannot start {path}: {error:#}");
            control.set_status(Status::Failed(error.clone()));
            let _ = sender.send(Err(error));

            return;
        }

        keep_live(path, source, control, sender);
    } else {
        if let Err(error) = source.pause() {
            let error = format!("cannot pause preloaded source: {error:#}");
            control.set_status(Status::Failed(error.clone()));
            let _ = sender.send(Err(error));

            return;
        }
        control.set_status(Status::Open { last_frame: None });
        wait_for_turn(source, control, sender);
    }
}

fn wait_for_turn(
    source: Source,
    control: &Control,
    sender: &SyncSender<Result<OpenedInput, String>>,
) {
    loop {
        match control.state() {
            HANDOVER => {
                control.state.store(HANDED, Ordering::Release);
                let _ = sender.send(Ok(source.into()));

                return;
            }
            CANCELLED => return,
            _ => thread::sleep(IDLE_POLL),
        }
    }
}

fn keep_live(
    path: &str,
    mut source: Source,
    control: &Control,
    sender: &SyncSender<Result<OpenedInput, String>>,
) {
    let mut preroll = Preroll::default();

    loop {
        match control.state() {
            HANDOVER => {
                control.state.store(HANDED, Ordering::Release);
                let _ = sender.send(Ok(OpenedInput {
                    input: source,
                    preroll: preroll.into_frames(),
                }));

                return;
            }
            CANCELLED => return,
            _ => {}
        }

        match source.next(IDLE_POLL) {
            Ok(Some(frame)) => {
                control.set_status(Status::Open {
                    last_frame: Some(Instant::now()),
                });
                preroll.push(frame);
            }
            Ok(None) if !source.is_eos() => {}
            result => {
                let detail = result
                    .err()
                    .map(|error| format!("{error:#}"))
                    .unwrap_or_else(|| "end of stream".to_string());
                let error = format!("{path} stopped sending: {detail}");
                control.set_status(Status::Failed(error.clone()));
                let _ = sender.send(Err(error));

                return;
            }
        }
    }
}

#[derive(Default)]
struct Preroll {
    frames: VecDeque<(Instant, FrameEvent)>,
    bytes: usize,
}

impl Preroll {
    fn push(&mut self, frame: FrameEvent) {
        self.bytes = self.bytes.saturating_add(frame_bytes(&frame));
        self.frames.push_back((Instant::now(), frame));

        while self.frames.len() > 1
            && (self.frames.len() > PREROLL_MAX_FRAMES
                || self.bytes > PREROLL_MAX_BYTES
                || self
                    .frames
                    .front()
                    .is_some_and(|(at, _)| at.elapsed() > PREROLL_SPAN))
        {
            if let Some((_, dropped)) = self.frames.pop_front() {
                self.bytes = self.bytes.saturating_sub(frame_bytes(&dropped));
            }
        }
    }

    fn into_frames(self) -> VecDeque<FrameEvent> {
        self.frames.into_iter().map(|(_, frame)| frame).collect()
    }
}

fn frame_bytes(frame: &FrameEvent) -> usize {
    match frame {
        FrameEvent::Video(frame) => {
            (frame.width() as usize)
                .saturating_mul(frame.height() as usize)
                .saturating_mul(3)
                / 2
        }
        FrameEvent::Audio(frame) => frame
            .samples()
            .saturating_mul(frame.channels() as usize)
            .saturating_mul(size_of::<f32>()),
    }
}

#[cfg(test)]
mod tests {
    use ffmpeg_next::{frame, util::format::pixel::Pixel};

    use super::*;

    fn video() -> FrameEvent {
        FrameEvent::Video(frame::Video::new(Pixel::YUV420P, 16, 16))
    }

    #[test]
    fn cancellation_interrupts_a_source_still_opening() {
        let control = Control::new(false);
        control.cancel();

        assert_eq!(control.state(), CANCELLED);
        assert!(control.cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn cleanup_does_not_cancel_a_source_already_handed_to_playout() {
        let control = Control::new(true);
        control.request_handover();
        control.cancel();

        assert_eq!(control.state(), HANDOVER);
        assert!(!control.cancelled.load(Ordering::Acquire));
    }

    #[test]
    fn decoded_preroll_keeps_only_its_recent_window() {
        let mut preroll = Preroll::default();
        preroll.push(video());
        preroll.frames.front_mut().unwrap().0 = Instant::now() - PREROLL_SPAN - IDLE_POLL;
        preroll.push(video());

        assert_eq!(preroll.frames.len(), 1);
        assert_eq!(preroll.bytes, 16 * 16 * 3 / 2);
    }

    #[test]
    fn decoded_preroll_has_a_hard_frame_bound() {
        let mut preroll = Preroll::default();

        for _ in 0..PREROLL_MAX_FRAMES + 20 {
            preroll.push(video());
        }

        assert!(preroll.frames.len() <= PREROLL_MAX_FRAMES);
    }

    #[test]
    fn a_missing_file_fails_and_is_not_taken() {
        let preloader = Preloader::default();
        preloader.preload("/does/not/exist.mp4");
        let deadline = Instant::now() + Duration::from_secs(5);

        while preloader.signal("/does/not/exist.mp4") == Some(PreloadSignal::Opening) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }

        assert!(matches!(
            preloader.signal("/does/not/exist.mp4"),
            Some(PreloadSignal::Failed(_))
        ));
        assert!(preloader.take("/does/not/exist.mp4").is_none());
        assert!(preloader.signal("/does/not/exist.mp4").is_none());
    }

    #[test]
    fn a_file_is_open_before_its_turn() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix/av_sync.mp4");
        let path = path.to_string_lossy();
        let preloader = Preloader::default();
        preloader.configure(&OutputConfig::new(320, 180, 25, 48_000));
        preloader.preload(&path);
        let deadline = Instant::now() + Duration::from_secs(10);

        while preloader.signal(&path) == Some(PreloadSignal::Opening) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }

        assert_eq!(preloader.signal(&path), Some(PreloadSignal::Ready));
        let opened = preloader
            .take(&path)
            .expect("a ready preload is taken")
            .hand_over()
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert!(opened.input.info().has_video);
        assert!(opened.preroll.is_empty());
    }

    #[test]
    fn a_preloaded_file_seeks_without_blocking_the_next_clip() {
        let (sent, received) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests_assets/storage/media_mix/av_sync.mp4");
            let path = path.to_string_lossy();
            let preloader = Preloader::default();
            preloader.configure(&OutputConfig::new(320, 180, 25, 48_000));
            preloader.preload(&path);
            let deadline = Instant::now() + Duration::from_secs(2);
            while preloader.signal(&path) == Some(PreloadSignal::Opening) {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            // Let both bounded appsink queues fill before the cross-thread handover.
            thread::sleep(Duration::from_millis(100));
            let mut opened = preloader
                .take(&path)
                .unwrap()
                .hand_over()
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap();
            opened.input.start(Some(7.0)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            let mut video = None;
            let mut audio = None;
            while video.is_none() || audio.is_none() {
                assert!(Instant::now() < deadline);
                match opened.input.next(Duration::from_millis(10)).unwrap() {
                    Some(FrameEvent::Video(frame)) => {
                        video.get_or_insert(frame.pts().unwrap());
                    }
                    Some(FrameEvent::Audio(frame)) => {
                        audio.get_or_insert(frame.pts().unwrap());
                    }
                    None => {}
                }
            }
            sent.send((video.unwrap(), audio.unwrap())).unwrap();
        });
        let (video, audio) = received
            .recv_timeout(Duration::from_secs(4))
            .expect("preloaded file seek must not block the playout worker");
        assert!((175..=178).contains(&video), "video timestamp {video}");
        assert!(
            (336_000..=340_800).contains(&audio),
            "audio timestamp {audio}"
        );
    }

    #[test]
    fn at_most_three_preloads_are_kept() {
        let preloader = Preloader::default();

        for index in 0..5 {
            preloader.preload(&format!("/missing/{index}.mp4"));
        }

        assert_eq!(preloader.entries.lock().unwrap().0.len(), MAX_ENTRIES);
        assert!(preloader.signal("/missing/0.mp4").is_none());
        assert!(preloader.signal("/missing/4.mp4").is_some());
    }
}
