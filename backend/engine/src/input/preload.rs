//! Opens the next source ahead of its turn, so it starts at once.
//!
//! A file or an HTTP stream is opened and probed, then waits. A live transport
//! (SRT, RTMP, UDP...) is connected and kept reading at its live edge: only the
//! packets since its last key frame stay in memory, so the takeover starts
//! with a picture and adds no delay. An open live preload also tells whether
//! the source is sending, which makes it the signal check of a scheduled live.

use std::{
    collections::VecDeque,
    fmt,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU8, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use ffmpeg_next::{Error as FfmpegError, Packet, error::EAGAIN, format, media};
use log::debug;

use crate::utils::helper::{is_live_input, is_network_url, network_io_options};

/// Preloads kept at once: the next clip, an upcoming event and one spare.
const MAX_ENTRIES: usize = 3;
/// A preload nobody takes is dropped after this long.
const EXPIRY: Duration = Duration::from_secs(10 * 60);
/// A live preload keeps the packets since its last key frame, starting over
/// at the first key frame after its oldest packet is this old.
const PREROLL_SPAN: Duration = Duration::from_millis(1500);
const PREROLL_MAX_PACKETS: usize = 4096;
const PREROLL_MAX_BYTES: usize = 32 * 1024 * 1024;
/// A live source quiet for this long counts as stalled.
const STALL_AFTER: Duration = Duration::from_secs(3);
/// How long a handover waits for a packet boundary before it cuts a blocked
/// read short.
const HANDOVER_GRACE: Duration = Duration::from_millis(300);
/// How often an open file looks for its handover.
const IDLE_POLL: Duration = Duration::from_millis(20);

const READING: u8 = 0;
const HANDOVER: u8 = 1;
const CANCELLED: u8 = 2;
const HANDED: u8 = 3;

/// An open source and, for a preloaded live one, what it already received.
pub(crate) struct OpenedInput {
    pub(crate) input: format::context::Input,
    /// Packets to play before reading on, with their stream index.
    pub(crate) preroll: VecDeque<(usize, Packet)>,
}

impl From<format::context::Input> for OpenedInput {
    fn from(input: format::context::Input) -> Self {
        Self {
            input,
            preroll: VecDeque::new(),
        }
    }
}

/// How a preloaded source is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadSignal {
    Opening,
    /// Open; a live source is also sending.
    Ready,
    /// A live source that is open but stopped sending.
    Stalled,
    Failed(String),
}

#[derive(Debug, Clone)]
enum Status {
    Opening,
    Open { last_packet: Option<Instant> },
    Failed(String),
}

struct Control {
    state: AtomicU8,
    created: Instant,
    /// Milliseconds after `created` when the handover was asked for.
    handover_at: AtomicU64,
    live: bool,
    status: Mutex<Status>,
}

impl Control {
    fn new(live: bool) -> Self {
        Self {
            state: AtomicU8::new(READING),
            created: Instant::now(),
            handover_at: AtomicU64::new(0),
            live,
            status: Mutex::new(Status::Opening),
        }
    }

    fn state(&self) -> u8 {
        self.state.load(Ordering::Acquire)
    }

    /// The FFmpeg interrupt callback: stop opening or reading when cancelled,
    /// or when a handover waited long enough for a packet boundary.
    fn interrupted(&self) -> bool {
        match self.state() {
            CANCELLED => true,
            HANDOVER => {
                self.created.elapsed().as_millis() as u64
                    >= self.handover_at.load(Ordering::Acquire) + HANDOVER_GRACE.as_millis() as u64
            }
            _ => false,
        }
    }

    fn cancel(&self) {
        let _ =
            self.state
                .compare_exchange(READING, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
    }

    fn request_handover(&self) {
        self.handover_at
            .store(self.created.elapsed().as_millis() as u64, Ordering::Release);
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
            Status::Open { last_packet } => {
                if last_packet.is_some_and(|at| at.elapsed() < STALL_AFTER) {
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

/// A source opened ahead, taken for its turn.
pub(crate) struct Preloaded {
    control: Arc<Control>,
    receiver: Receiver<Result<OpenedInput, String>>,
}

impl Preloaded {
    /// Ask for the source; it arrives on the receiver, at once when it is
    /// open, or when it finishes opening.
    pub(crate) fn hand_over(self) -> Receiver<Result<OpenedInput, String>> {
        self.control.request_handover();

        self.receiver
    }
}

/// The preloads of one playout, shared by the application and the worker.
#[derive(Clone, Default)]
pub struct Preloader {
    entries: Arc<Mutex<Entries>>,
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
    /// Start opening `path`, unless it is already open or opening.
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
            let oldest = entries.0.remove(0);

            oldest.control.cancel();
        }

        debug!("Preloading {path}");
        entries.0.push(spawn(path.to_string()));
    }

    /// How the preload of `path` is doing, if there is one.
    pub fn signal(&self, path: &str) -> Option<PreloadSignal> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        entries
            .0
            .iter()
            .find(|entry| entry.path == path.trim())
            .map(|entry| entry.control.signal())
    }

    /// Drop the preload of `path`: its turn will not come.
    pub fn forget(&self, path: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        remove(&mut entries.0, |entry| entry.path == path.trim());
    }

    /// Drop every preload, when the playout ends.
    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        remove(&mut entries.0, |_| true);
    }

    /// The preload of `path`, for its turn; `None` when there is none or it
    /// failed, so the source opens the usual way.
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

fn spawn(path: String) -> Entry {
    let control = Arc::new(Control::new(is_live_input(&path)));
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker_control = control.clone();
    let worker_path = path.clone();
    let spawned = thread::Builder::new()
        .name("ff-preload".to_string())
        .spawn(move || run(&worker_path, &worker_control, &sender));

    if let Err(error) = spawned {
        control.set_status(Status::Failed(format!("cannot start the preload: {error}")));
    }

    Entry {
        path,
        control,
        receiver,
    }
}

fn run(path: &str, control: &Arc<Control>, sender: &SyncSender<Result<OpenedInput, String>>) {
    let input = match open(path, control) {
        Ok(input) => input,
        Err(error) => {
            control.set_status(Status::Failed(error.clone()));
            let _ = sender.send(Err(error));

            return;
        }
    };

    control.set_status(Status::Open { last_packet: None });

    if control.live {
        keep_live(path, input, control, sender);
    } else {
        wait_for_turn(input, control, sender);
    }
}

fn open(path: &str, control: &Arc<Control>) -> Result<format::context::Input, String> {
    let interrupt_control = control.clone();
    let interrupt = move || interrupt_control.interrupted();
    let opened = if is_network_url(path) {
        format::input_with_interrupt_and_dictionary(&path, interrupt, network_io_options())
    } else {
        format::input_with_interrupt(&path, interrupt)
    };

    opened.map_err(|error| format!("cannot open {path}: {error}"))
}

/// A file waits, open, for its turn.
fn wait_for_turn(
    input: format::context::Input,
    control: &Control,
    sender: &SyncSender<Result<OpenedInput, String>>,
) {
    loop {
        match control.state() {
            HANDOVER => {
                control.state.store(HANDED, Ordering::Release);
                let _ = sender.send(Ok(input.into()));

                return;
            }
            CANCELLED => return,
            _ => thread::sleep(IDLE_POLL),
        }
    }
}

/// A live source keeps reading at its edge until its turn.
fn keep_live(
    path: &str,
    mut input: format::context::Input,
    control: &Control,
    sender: &SyncSender<Result<OpenedInput, String>>,
) {
    let video = input
        .streams()
        .best(media::Type::Video)
        .map(|stream| stream.index());
    let mut preroll = Preroll::new(video);

    loop {
        match control.state() {
            HANDOVER => {
                // A read cut short leaves an exit code latched in the input.
                input.clear_interrupt();
                control.state.store(HANDED, Ordering::Release);
                let _ = sender.send(Ok(OpenedInput {
                    input,
                    preroll: preroll.packets,
                }));

                return;
            }
            CANCELLED => return,
            _ => {}
        }

        let mut packet = Packet::empty();

        match packet.read(&mut input) {
            Ok(()) => {
                control.set_status(Status::Open {
                    last_packet: Some(Instant::now()),
                });
                preroll.push(packet.stream(), packet);
            }
            Err(FfmpegError::Other { errno }) if errno == EAGAIN => {}
            Err(_) if control.state() != READING => {}
            Err(error) => {
                let error = format!("{path} stopped sending: {error}");

                control.set_status(Status::Failed(error.clone()));
                let _ = sender.send(Err(error));

                return;
            }
        }
    }
}

/// The packets of a live source since its last key frame.
struct Preroll {
    video: Option<usize>,
    packets: VecDeque<(usize, Packet)>,
    bytes: usize,
    /// When the oldest kept packet arrived.
    started: Option<Instant>,
}

impl Preroll {
    fn new(video: Option<usize>) -> Self {
        Self {
            video,
            packets: VecDeque::new(),
            bytes: 0,
            started: None,
        }
    }

    fn push(&mut self, stream: usize, packet: Packet) {
        let key_frame = Some(stream) == self.video && packet.is_key();
        let too_big = self.packets.len() >= PREROLL_MAX_PACKETS || self.bytes >= PREROLL_MAX_BYTES;
        let old = self
            .started
            .is_some_and(|started| started.elapsed() >= PREROLL_SPAN);

        // Start over at a key frame once the kept packets are old enough, so
        // the takeover begins with a whole picture and little delay.
        if too_big || (old && (key_frame || self.video.is_none())) {
            self.packets.clear();
            self.bytes = 0;
            self.started = None;
        }

        // With video, nothing before the first key frame can be shown.
        if self.packets.is_empty() && self.video.is_some() && !key_frame {
            return;
        }

        self.started.get_or_insert_with(Instant::now);
        self.bytes += packet.size();
        self.packets.push_back((stream, packet));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(key: bool) -> Packet {
        let mut packet = Packet::copy(&[0_u8; 100]);

        if key {
            packet.set_flags(ffmpeg_next::packet::Flags::KEY);
        }

        packet
    }

    #[test]
    fn a_live_preroll_starts_at_a_key_frame() {
        let mut preroll = Preroll::new(Some(0));

        preroll.push(1, packet(false));
        preroll.push(0, packet(false));
        assert!(preroll.packets.is_empty());

        preroll.push(0, packet(true));
        preroll.push(1, packet(false));
        preroll.push(0, packet(false));
        assert_eq!(preroll.packets.len(), 3);
        assert!(preroll.packets[0].1.is_key());
    }

    #[test]
    fn an_old_preroll_starts_over_at_the_next_key_frame() {
        let mut preroll = Preroll::new(Some(0));

        preroll.push(0, packet(true));
        preroll.push(0, packet(false));
        preroll.started = Some(Instant::now() - PREROLL_SPAN);

        // Not a key frame: keep going.
        preroll.push(0, packet(false));
        assert_eq!(preroll.packets.len(), 3);

        preroll.push(0, packet(true));
        assert_eq!(preroll.packets.len(), 1);
        assert!(preroll.packets[0].1.is_key());
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
        ffmpeg_next::init().ok();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/storage/media_mix/av_sync.mp4");
        let path = path.to_string_lossy();
        let preloader = Preloader::default();

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

        assert!(opened.input.streams().best(media::Type::Video).is_some());
        assert!(opened.preroll.is_empty());
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
