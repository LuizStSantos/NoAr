//! The network end of a stream output (SRT, RTMP, UDP), kept apart from the
//! playout.
//!
//! The muxer writes into a [`NetworkSink`], which never blocks and never
//! fails: a thread of its own connects to the destination, sends what the
//! muxer produced, and reconnects when the connection drops. So a receiver
//! that is down, an RTMP server that refuses the connection, or an SRT
//! listener that waits for a caller no longer stops the channel: the playout,
//! the Player preview, the recording and the as-run log carry on, and the
//! destination gets the stream again from the next key frame once it is back.

use std::{
    collections::BTreeMap,
    ffi::{CString, c_int, c_void},
    fmt, io, ptr,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use ffmpeg_next::{
    Dictionary, ffi,
    format::{self, context::StreamIo},
};

use crate::utils::helper::network_output_options;

const TS_PACKET: usize = 188;
/// What waits for the network at most; past it, data is dropped and the
/// destination resumes at the next key frame. About ten seconds of a
/// typical channel.
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
const FIRST_RETRY: Duration = Duration::from_secs(1);
const LAST_RETRY: Duration = Duration::from_secs(10);
/// How long closing waits for the last data to go out.
const CLOSE_GRACE: Duration = Duration::from_secs(2);

/// How the output reaches its destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkState {
    /// No network output (HLS, desktop, a file).
    #[default]
    Idle,
    /// Calling the destination.
    Connecting,
    /// An SRT listener waiting for a receiver to call.
    Listening,
    /// Sending.
    Connected,
    /// The last attempt failed; trying again soon.
    Retrying,
}

/// The state of a channel's network output, for the interface.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinkStatus {
    pub state: LinkState,
    /// The address, without any query string (it may hold a passphrase).
    pub destination: Option<String>,
    /// Why the last attempt failed or the connection dropped.
    pub error: Option<String>,
    /// When the current state began.
    pub since: Option<SystemTime>,
    /// Times the destination was reached since the playout started.
    pub connections: u32,
}

/// Shared view of a channel's network output. Clones share the same state,
/// which outlasts a playout restart, like the logo and splice controls.
#[derive(Clone, Default)]
pub struct OutputLink(Arc<Mutex<LinkStatus>>);

impl fmt::Debug for OutputLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OutputLink").field(&self.status()).finish()
    }
}

impl OutputLink {
    pub fn status(&self) -> LinkStatus {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn update(&self, change: impl FnOnce(&mut LinkStatus)) {
        let mut status = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let before = status.state;
        change(&mut status);

        if status.state != before {
            status.since = Some(SystemTime::now());
        }
    }

    fn set(&self, state: LinkState, error: Option<String>) {
        self.update(|status| {
            if state == LinkState::Connected && status.state != LinkState::Connected {
                status.connections += 1;
            }
            status.state = state;
            status.error = error;
        });
    }

    /// The output is not a network one, or the playout stopped.
    pub(crate) fn idle(&self) {
        self.update(|status| {
            status.state = LinkState::Idle;
            status.destination = None;
            status.error = None;
        });
    }
}

/// The container on the wire, which decides where the stream can resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkFormat {
    /// MPEG-TS (SRT, UDP): any transport packet is a place to resume; the
    /// receiver waits for the program tables and a key frame by itself.
    TransportStream,
    /// FLV (RTMP): a new connection needs the file header, the metadata and
    /// the codec headers again, then resumes on a video key frame.
    Flv,
}

impl LinkFormat {
    pub(crate) fn for_muxer(muxer: &str) -> Option<Self> {
        match muxer {
            "mpegts" => Some(Self::TransportStream),
            "flv" => Some(Self::Flv),
            _ => None,
        }
    }
}

/// A piece of the stream that can be sent on its own.
struct Unit {
    data: Vec<u8>,
    /// The destination can start here.
    resume_point: bool,
}

struct Shared {
    /// Abort a connection attempt or a write in progress.
    stop: AtomicBool,
    queued_bytes: AtomicUsize,
    /// Data was dropped: the destination resumes at the next key frame.
    gap: AtomicBool,
    /// What a new connection receives first (FLV header, metadata and
    /// codec headers).
    preamble: Mutex<Preamble>,
}

#[derive(Default)]
struct Preamble {
    file_header: Vec<u8>,
    metadata: Vec<u8>,
    video_config: Vec<u8>,
    audio_config: Vec<u8>,
}

impl Preamble {
    fn bytes(&self) -> Vec<u8> {
        [
            &self.file_header,
            &self.metadata,
            &self.video_config,
            &self.audio_config,
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect()
    }
}

/// Opens a muxer of `muxer` whose output goes to `url` through a
/// [`NetworkSink`].
pub(crate) fn open_output(
    url: &str,
    muxer: &str,
    format: LinkFormat,
    protocol_options: &BTreeMap<String, String>,
    link: OutputLink,
    channel_id: Option<i32>,
) -> Result<format::context::Output> {
    let sink = NetworkSink::start(url, format, protocol_options, link, channel_id)?;
    // Seven transport packets, the size SRT and UDP send at a time.
    let io = StreamIo::from_write_with_capacity(sink, 7 * TS_PACKET)?;

    format::output_to_stream(io, Some(url), Some(muxer))
        .with_context(|| format!("failed to open {muxer} output"))
}

/// Where the muxer writes a network output. Writing only queues the data for
/// the connection thread.
pub(crate) struct NetworkSink {
    sender: Option<Sender<Unit>>,
    shared: Arc<Shared>,
    link: OutputLink,
    framer: Framer,
    thread: Option<JoinHandle<()>>,
    done: Option<Receiver<()>>,
}

impl NetworkSink {
    pub(crate) fn start(
        url: &str,
        format: LinkFormat,
        protocol_options: &BTreeMap<String, String>,
        link: OutputLink,
        channel_id: Option<i32>,
    ) -> Result<Self> {
        let address = CString::new(url)?;
        for (key, value) in protocol_options {
            CString::new(key.as_str())?;
            CString::new(value.as_str())?;
        }

        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            queued_bytes: AtomicUsize::new(0),
            gap: AtomicBool::new(false),
            preamble: Mutex::new(Preamble::default()),
        });
        let (sender, receiver) = mpsc::channel();
        let (done_sender, done) = mpsc::channel();
        let listener = is_listener(url);
        link.update(|status| {
            status.state = LinkState::Connecting;
            status.destination = Some(public_address(url));
            status.error = None;
            status.connections = 0;
        });

        let connection = Connection {
            address,
            options: protocol_options.clone(),
            listener,
            shared: shared.clone(),
            link: link.clone(),
            receiver,
            channel: channel_id.unwrap_or_default(),
            destination: public_address(url),
        };
        let thread = thread::Builder::new()
            .name(format!("noar-output-{}", channel_id.unwrap_or_default()))
            .spawn(move || {
                connection.run();
                let _ = done_sender.send(());
            })
            .context("failed to start the network output thread")?;

        Ok(Self {
            sender: Some(sender),
            shared,
            link,
            framer: Framer::new(format),
            thread: Some(thread),
            done: Some(done),
        })
    }

    fn queue(&mut self, unit: Unit) {
        let Some(sender) = &self.sender else {
            return;
        };
        let size = unit.data.len();

        if self.shared.queued_bytes.load(Ordering::Acquire) + size > MAX_QUEUED_BYTES {
            // The network cannot keep up: drop rather than hold the
            // playout back, and resume cleanly further on.
            self.shared.gap.store(true, Ordering::Release);
            return;
        }

        self.shared.queued_bytes.fetch_add(size, Ordering::AcqRel);
        if sender.send(unit).is_err() {
            self.shared.queued_bytes.fetch_sub(size, Ordering::AcqRel);
        }
    }
}

impl io::Write for NetworkSink {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        for unit in self.framer.push(data, &self.shared) {
            self.queue(unit);
        }

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for NetworkSink {
    fn drop(&mut self) {
        // Closing the queue lets the thread send what is left (the trailer
        // included) and hang up.
        self.sender.take();

        if self.link.status().state != LinkState::Connected {
            // Nothing is going out: stop waiting for a receiver now, so a
            // restarted playout can take the port again.
            self.shared.stop.store(true, Ordering::Release);
        }

        if let Some(done) = self.done.take()
            && done.recv_timeout(CLOSE_GRACE).is_err()
        {
            self.shared.stop.store(true, Ordering::Release);
            let _ = done.recv_timeout(CLOSE_GRACE);
        }

        if let Some(thread) = self.thread.take()
            && thread.is_finished()
        {
            let _ = thread.join();
        }

        self.link.idle();
    }
}

/// Cuts what the muxer writes into units that can be sent on their own.
enum Framer {
    TransportStream { pending: Vec<u8> },
    Flv(FlvFramer),
}

impl Framer {
    fn new(format: LinkFormat) -> Self {
        match format {
            LinkFormat::TransportStream => Self::TransportStream {
                pending: Vec::new(),
            },
            LinkFormat::Flv => Self::Flv(FlvFramer::default()),
        }
    }

    fn push(&mut self, data: &[u8], shared: &Shared) -> Vec<Unit> {
        match self {
            Self::TransportStream { pending } => {
                pending.extend_from_slice(data);
                let whole = pending.len() / TS_PACKET * TS_PACKET;

                if whole == 0 {
                    return Vec::new();
                }

                let rest = pending.split_off(whole);
                let data = std::mem::replace(pending, rest);

                vec![Unit {
                    data,
                    resume_point: true,
                }]
            }
            Self::Flv(framer) => framer.push(data, shared),
        }
    }
}

const FLV_FILE_HEADER: usize = 9 + 4;
const FLV_TAG_HEADER: usize = 11;
const FLV_AUDIO: u8 = 8;
const FLV_VIDEO: u8 = 9;
const FLV_SCRIPT: u8 = 18;

/// Splits an FLV byte stream into whole tags, and keeps what a new
/// connection needs before them.
#[derive(Default)]
struct FlvFramer {
    pending: Vec<u8>,
    header_seen: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum FlvTag {
    Metadata,
    VideoConfig,
    AudioConfig,
    KeyFrame,
    Other,
}

fn classify_flv_tag(tag: &[u8]) -> FlvTag {
    let kind = tag[0] & 0x1F;
    let body = &tag[FLV_TAG_HEADER..tag.len() - 4];
    let (Some(&first), second) = (body.first(), body.get(1).copied()) else {
        return FlvTag::Other;
    };

    match kind {
        FLV_SCRIPT => FlvTag::Metadata,
        FLV_VIDEO => {
            let frame_type = (first >> 4) & 0x07;

            let config = if first & 0x80 != 0 {
                // Enhanced FLV: the packet type is in the low bits;
                // 0 is the sequence start.
                first & 0x0F == 0
            } else {
                // H.264 (7) and HEVC (12): packet type 0 is the decoder
                // configuration.
                matches!(first & 0x0F, 7 | 12) && second == Some(0)
            };

            if config {
                FlvTag::VideoConfig
            } else if frame_type == 1 {
                FlvTag::KeyFrame
            } else {
                FlvTag::Other
            }
        }
        // AAC (10) with packet type 0 is the decoder configuration.
        FLV_AUDIO if first >> 4 == 10 && second == Some(0) => FlvTag::AudioConfig,
        _ => FlvTag::Other,
    }
}

impl FlvFramer {
    fn push(&mut self, data: &[u8], shared: &Shared) -> Vec<Unit> {
        self.pending.extend_from_slice(data);
        let mut units = Vec::new();

        if !self.header_seen {
            if self.pending.len() < FLV_FILE_HEADER {
                return units;
            }

            let header: Vec<u8> = self.pending.drain(..FLV_FILE_HEADER).collect();
            lock(&shared.preamble).file_header.clone_from(&header);
            self.header_seen = true;
        }

        while self.pending.len() >= FLV_TAG_HEADER {
            let size = (usize::from(self.pending[1]) << 16)
                | (usize::from(self.pending[2]) << 8)
                | usize::from(self.pending[3]);
            let total = FLV_TAG_HEADER + size + 4;

            if self.pending.len() < total {
                break;
            }

            let tag: Vec<u8> = self.pending.drain(..total).collect();
            let kind = classify_flv_tag(&tag);
            {
                let mut preamble = lock(&shared.preamble);

                match kind {
                    FlvTag::Metadata => preamble.metadata.clone_from(&tag),
                    FlvTag::VideoConfig => preamble.video_config.clone_from(&tag),
                    FlvTag::AudioConfig => preamble.audio_config.clone_from(&tag),
                    FlvTag::KeyFrame | FlvTag::Other => {}
                }
            }

            units.push(Unit {
                data: tag,
                resume_point: kind == FlvTag::KeyFrame,
            });
        }

        units
    }
}

/// The connection thread.
struct Connection {
    address: CString,
    options: BTreeMap<String, String>,
    listener: bool,
    shared: Arc<Shared>,
    link: OutputLink,
    receiver: Receiver<Unit>,
    channel: i32,
    destination: String,
}

enum Ended {
    /// The muxer closed the output.
    Closed,
    /// The connection failed; try again.
    Lost(String),
}

impl Connection {
    fn run(self) {
        let mut retry = FIRST_RETRY;
        let mut warned = false;

        loop {
            if self.stopped() {
                return;
            }

            self.link.set(
                if self.listener {
                    LinkState::Listening
                } else {
                    LinkState::Connecting
                },
                None,
            );

            let io = match self.connect() {
                Ok(io) => io,
                Err(error) if self.stopped() => {
                    log::debug!(channel = self.channel; "Output to {}: {error}", self.destination);
                    return;
                }
                Err(error) => {
                    if !warned {
                        log::error!(channel = self.channel;
                            "Output cannot reach {}: {error}. The playout and the Player preview go on; NoAr keeps trying",
                            self.destination
                        );
                        warned = true;
                    }
                    self.link.set(LinkState::Retrying, Some(error));

                    if self.wait(retry) {
                        return;
                    }
                    retry = (retry * 2).min(LAST_RETRY);
                    continue;
                }
            };

            log::info!(channel = self.channel; "Output connected to {}", self.destination);
            self.link.set(LinkState::Connected, None);
            retry = FIRST_RETRY;
            warned = false;

            match self.send(io) {
                Ended::Closed => return,
                Ended::Lost(error) => {
                    log::error!(channel = self.channel;
                        "Output to {} lost: {error}. The playout goes on; NoAr reconnects",
                        self.destination
                    );
                    self.link.set(LinkState::Retrying, Some(error));

                    if self.wait(FIRST_RETRY) {
                        return;
                    }
                }
            }
        }
    }

    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Acquire)
    }

    /// Opens the connection; an SRT listener waits here for a caller.
    fn connect(&self) -> std::result::Result<Avio, String> {
        let mut io = ptr::null_mut();
        let callback = ffi::AVIOInterruptCB {
            callback: Some(interrupted),
            opaque: Arc::as_ptr(&self.shared).cast_mut().cast::<c_void>(),
        };
        let (result, unused) = unsafe {
            let mut dictionary = network_output_options(&self.options).disown();
            let result = ffi::avio_open2(
                &mut io,
                self.address.as_ptr(),
                ffi::AVIO_FLAG_WRITE,
                &callback,
                &mut dictionary,
            );
            let remaining = Dictionary::own(dictionary);
            let unused: Vec<String> = remaining
                .iter()
                .filter(|(key, _)| self.options.contains_key(*key))
                .map(|(key, _)| key.to_string())
                .collect();

            (result, unused)
        };

        if result < 0 {
            return Err(ffmpeg_next::Error::from(result).to_string());
        }

        if !unused.is_empty() {
            log::warn!(channel = self.channel;
                "Unused output protocol options: {}", unused.join(", ")
            );
        }

        Ok(Avio(io))
    }

    /// Sends until the muxer closes the output or the connection fails.
    fn send(&self, mut io: Avio) -> Ended {
        // What waited while connecting is stale: start from now.
        let mut need_resume = true;
        loop {
            match self.receiver.try_recv() {
                Ok(unit) => self.release(unit.data.len()),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ended::Closed,
            }
        }

        self.shared.gap.store(false, Ordering::Release);
        let preamble = lock(&self.shared.preamble).bytes();
        if let Err(error) = io.write(&preamble) {
            return Ended::Lost(error);
        }

        loop {
            match self.receiver.recv_timeout(Duration::from_millis(250)) {
                Ok(unit) => {
                    self.release(unit.data.len());

                    if self.shared.gap.swap(false, Ordering::AcqRel) {
                        need_resume = true;
                    }

                    if need_resume && !unit.resume_point {
                        continue;
                    }
                    need_resume = false;

                    if let Err(error) = io.write(&unit.data) {
                        return Ended::Lost(error);
                    }
                }
                Err(RecvTimeoutError::Timeout) if self.stopped() => return Ended::Closed,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ended::Closed,
            }
        }
    }

    fn release(&self, size: usize) {
        self.shared.queued_bytes.fetch_sub(size, Ordering::AcqRel);
    }

    /// Discards what the muxer sends for `pause`; true when the output closed.
    fn wait(&self, pause: Duration) -> bool {
        let until = Instant::now() + pause;

        loop {
            if self.stopped() {
                return true;
            }

            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }

            match self
                .receiver
                .recv_timeout(left.min(Duration::from_millis(250)))
            {
                Ok(unit) => self.release(unit.data.len()),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
    }
}

/// An open AVIO connection, closed when dropped.
struct Avio(*mut ffi::AVIOContext);

impl Avio {
    fn write(&mut self, data: &[u8]) -> std::result::Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }

        let size = c_int::try_from(data.len()).map_err(|error| error.to_string())?;
        unsafe {
            ffi::avio_write(self.0, data.as_ptr(), size);
            ffi::avio_flush(self.0);
        }
        let error = unsafe { (*self.0).error };

        if error < 0 {
            Err(ffmpeg_next::Error::from(error).to_string())
        } else {
            Ok(())
        }
    }
}

impl Drop for Avio {
    fn drop(&mut self) {
        unsafe { ffi::avio_closep(&mut self.0) };
    }
}

unsafe extern "C" fn interrupted(opaque: *mut c_void) -> c_int {
    // SAFETY: `opaque` points into the `Arc<Shared>` the connection thread
    // holds for as long as any AVIO context it opened is alive.
    let shared = unsafe { &*opaque.cast::<Shared>() };

    c_int::from(shared.stop.load(Ordering::Acquire))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn is_listener(url: &str) -> bool {
    url.split_once('?').is_some_and(|(_, query)| {
        query
            .split('&')
            .any(|pair| pair.eq_ignore_ascii_case("mode=listener"))
    }) || url.starts_with("rtmp://") && url.contains("listen=1")
}

/// The address without credentials or query string, for logs and the
/// interface.
fn public_address(url: &str) -> String {
    let base = url.split_once('?').map_or(url, |(base, _)| base);

    match base.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
            // An RTMP stream key is a secret too.
            let host = if scheme.starts_with("rtmp") {
                host.split_once('/')
                    .map_or(host.to_string(), |(server, _)| format!("{server}/…"))
            } else {
                host.to_string()
            };

            format!("{scheme}://{host}")
        }
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Write, net::UdpSocket};

    use super::*;

    fn shared() -> Shared {
        Shared {
            stop: AtomicBool::new(false),
            queued_bytes: AtomicUsize::new(0),
            gap: AtomicBool::new(false),
            preamble: Mutex::new(Preamble::default()),
        }
    }

    fn flv_tag(kind: u8, body: &[u8]) -> Vec<u8> {
        let size = body.len();
        let mut tag = vec![
            kind,
            (size >> 16) as u8,
            (size >> 8) as u8,
            size as u8,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        tag.extend_from_slice(body);
        tag.extend_from_slice(&((FLV_TAG_HEADER + size) as u32).to_be_bytes());
        tag
    }

    #[test]
    fn flv_is_cut_into_tags_whatever_the_writes() {
        let header = b"FLV\x01\x05\x00\x00\x00\x09\x00\x00\x00\x00".to_vec();
        let metadata = flv_tag(FLV_SCRIPT, b"\x02\x00\x0aonMetaData");
        let video_config = flv_tag(FLV_VIDEO, &[0x17, 0x00, 0, 0, 0, 1, 2]);
        let audio_config = flv_tag(FLV_AUDIO, &[0xAF, 0x00, 0x12, 0x10]);
        let key = flv_tag(FLV_VIDEO, &[0x17, 0x01, 0, 0, 0, 9, 9, 9]);
        let inter = flv_tag(FLV_VIDEO, &[0x27, 0x01, 0, 0, 0, 8]);
        let audio = flv_tag(FLV_AUDIO, &[0xAF, 0x01, 7, 7]);
        let stream = [
            header.clone(),
            metadata.clone(),
            video_config.clone(),
            audio_config.clone(),
            key.clone(),
            inter.clone(),
            audio.clone(),
        ]
        .concat();

        let shared = shared();
        let mut framer = FlvFramer::default();
        // Three bytes at a time: tag boundaries never match the writes.
        let units: Vec<Unit> = stream
            .chunks(3)
            .flat_map(|chunk| framer.push(chunk, &shared))
            .collect();

        let tags: Vec<&[u8]> = units.iter().map(|unit| unit.data.as_slice()).collect();
        assert_eq!(
            tags,
            [
                &metadata,
                &video_config,
                &audio_config,
                &key,
                &inter,
                &audio
            ]
            .map(Vec::as_slice)
        );
        assert_eq!(
            units
                .iter()
                .map(|unit| unit.resume_point)
                .collect::<Vec<_>>(),
            [false, false, false, true, false, false]
        );
        assert_eq!(
            lock(&shared.preamble).bytes(),
            [header, metadata, video_config, audio_config].concat()
        );
    }

    #[test]
    fn transport_stream_units_are_whole_packets() {
        let shared = shared();
        let mut framer = Framer::new(LinkFormat::TransportStream);

        assert!(framer.push(&[0x47; 100], &shared).is_empty());
        let units = framer.push(&[0x47; 300], &shared);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].data.len(), 2 * TS_PACKET);
        // 24 bytes were left over; 164 more complete the next packet.
        let units = framer.push(&[0x47; 164], &shared);
        assert_eq!(units[0].data.len(), TS_PACKET);
    }

    #[test]
    fn addresses_hide_secrets() {
        assert_eq!(
            public_address("srt://10.0.0.5:9000?mode=caller&passphrase=secret"),
            "srt://10.0.0.5:9000"
        );
        assert_eq!(
            public_address("rtmp://user:pass@live.example.org/app/streamkey"),
            "rtmp://live.example.org/…"
        );
        assert!(is_listener("srt://0.0.0.0:9000?mode=listener"));
        assert!(!is_listener("srt://10.0.0.5:9000?mode=caller"));
    }

    #[test]
    fn a_closed_destination_does_not_fail_the_writer_and_it_connects_later() {
        let link = OutputLink::default();
        // Nothing listens on this TCP port yet.
        let port = {
            let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            probe.local_addr().unwrap().port()
        };
        let url = format!("tcp://127.0.0.1:{port}");
        let mut sink = NetworkSink::start(
            &url,
            LinkFormat::TransportStream,
            &BTreeMap::new(),
            link.clone(),
            Some(1),
        )
        .unwrap();

        for _ in 0..50 {
            sink.write_all(&[0x47; TS_PACKET * 7]).unwrap();
        }
        thread::sleep(Duration::from_millis(300));
        assert_eq!(link.status().state, LinkState::Retrying);
        assert!(link.status().error.is_some());

        let server = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        server.set_nonblocking(false).unwrap();
        let accepted = thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut received = vec![0; TS_PACKET * 7];
            std::io::Read::read_exact(&mut socket, &mut received).map(|()| received)
        });

        let deadline = Instant::now() + Duration::from_secs(8);
        while link.status().state != LinkState::Connected && Instant::now() < deadline {
            sink.write_all(&[0x47; TS_PACKET * 7]).unwrap();
            thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(link.status().state, LinkState::Connected);
        assert_eq!(link.status().connections, 1);

        for _ in 0..20 {
            sink.write_all(&[0x47; TS_PACKET * 7]).unwrap();
            thread::sleep(Duration::from_millis(20));
        }
        let received = accepted.join().unwrap().unwrap();
        assert!(received.iter().all(|byte| *byte == 0x47));

        drop(sink);
        assert_eq!(link.status().state, LinkState::Idle);
    }

    #[test]
    fn a_waiting_listener_is_released_when_the_output_closes() {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let link = OutputLink::default();
        let url = format!("tcp://127.0.0.1:{port}?listen=1");
        let sink = NetworkSink::start(
            &url,
            LinkFormat::TransportStream,
            &BTreeMap::new(),
            link.clone(),
            Some(1),
        )
        .unwrap();
        thread::sleep(Duration::from_millis(200));

        let started = Instant::now();
        drop(sink);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "closing does not hang"
        );
        assert_eq!(link.status().state, LinkState::Idle);
    }
}
