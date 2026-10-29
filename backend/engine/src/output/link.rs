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
    fmt, io,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use ::gstreamer as gst;
use anyhow::{Context, Result};
use gst::prelude::*;
use gstreamer_app as gst_app;
use url::Url;

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
    /// Allows shutdown to cancel a native connect/write that is waiting.
    pipeline: Mutex<Option<gst::Pipeline>>,
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
        crate::gstreamer::init()?;
        let settings =
            TransportSettings::parse(url, protocol_options).map_err(anyhow::Error::msg)?;
        settings
            .validate_format(format)
            .map_err(anyhow::Error::msg)?;

        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            queued_bytes: AtomicUsize::new(0),
            gap: AtomicBool::new(false),
            preamble: Mutex::new(Preamble::default()),
            pipeline: Mutex::new(None),
        });
        let (sender, receiver) = mpsc::channel();
        let (done_sender, done) = mpsc::channel();
        let listener = settings.listener;
        link.update(|status| {
            status.state = LinkState::Connecting;
            status.destination = Some(public_address(url));
            status.error = None;
            status.connections = 0;
        });

        let connection = Connection {
            settings,
            format,
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

impl NetworkSink {
    fn cancel_native_pipeline(&self) {
        let pipeline = lock(&self.shared.pipeline).clone();

        if let Some(pipeline) = pipeline {
            let _ = pipeline.send_event(gst::event::FlushStart::new());
            let _ = pipeline.set_state(gst::State::Null);
        }
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
            self.cancel_native_pipeline();
        }

        if let Some(done) = self.done.take()
            && done.recv_timeout(CLOSE_GRACE).is_err()
        {
            self.shared.stop.store(true, Ordering::Release);
            self.cancel_native_pipeline();
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
    settings: TransportSettings,
    format: LinkFormat,
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

    fn connect(&self) -> std::result::Result<NativeTransport, String> {
        NativeTransport::open(&self.settings, self.format, self.shared.clone())
            .map_err(|error| error.to_string())
    }

    /// Sends until the muxer closes the output or the connection fails.
    fn send(&self, mut io: NativeTransport) -> Ended {
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
            if self.stopped() {
                return Ended::Closed;
            }

            if let Err(error) = io.poll() {
                return Ended::Lost(error);
            }

            if io.connected() && self.link.status().state != LinkState::Connected {
                log::info!(channel = self.channel; "Output connected to {}", self.destination);
                self.link.set(LinkState::Connected, None);
            }

            match self.receiver.recv_timeout(Duration::from_millis(25)) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransportKind {
    Srt,
    Udp,
    Rtmp,
    Tcp,
}

struct TransportSettings {
    kind: TransportKind,
    uri: Url,
    properties: BTreeMap<String, String>,
    listener: bool,
    timeout: Duration,
    packet_size: usize,
}

impl TransportSettings {
    fn parse(address: &str, configured: &BTreeMap<String, String>) -> Result<Self, String> {
        if address.chars().any(char::is_control) {
            return Err("output address contains control characters".to_string());
        }

        let address = address.replacen("srt://:", "srt://0.0.0.0:", 1);
        let mut uri = Url::parse(&address).map_err(|_| "invalid output URL".to_string())?;
        let kind = match uri.scheme() {
            "srt" => TransportKind::Srt,
            "udp" => TransportKind::Udp,
            "rtmp" | "rtmps" => TransportKind::Rtmp,
            "tcp" => TransportKind::Tcp,
            _ => return Err("GStreamer output requires SRT, UDP, RTMP(S) or TCP".to_string()),
        };
        let mut options = BTreeMap::new();
        let mut timeout = Duration::from_secs(5);
        let pairs = if kind == TransportKind::Rtmp {
            // An RTMP URL query is part of its stream key/authentication, not
            // an FFmpeg/GStreamer transport-options dictionary.
            Vec::new()
        } else {
            uri.query_pairs()
                .map(|(key, value)| (key.into_owned(), value.into_owned()))
                .collect()
        };

        for (key, value) in pairs.into_iter().chain(
            configured
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        ) {
            if key.chars().any(char::is_control) || value.chars().any(char::is_control) {
                return Err(format!(
                    "protocol option {key:?} contains control characters"
                ));
            }

            if matches!(key.as_str(), "rw_timeout" | "timeout") {
                let micros = parse_u64(&key, &value)?;
                if !(1..=60_000_000).contains(&micros) {
                    return Err(format!(
                        "protocol option {key:?} must be between 1 and 60000000 microseconds"
                    ));
                }
                timeout = Duration::from_micros(micros);
                continue;
            }

            let (name, value) = normalize_option(kind, &key, &value)?;
            if options
                .insert(name.clone(), value.clone())
                .is_some_and(|old| old != value)
            {
                return Err(format!(
                    "protocol option {name:?} conflicts with the output URL or another configured alias"
                ));
            }
        }

        let listener = options.get("mode").is_some_and(|mode| mode == "listener")
            || options.get("listen").is_some_and(|listen| listen == "true");
        let packet_size = options
            .get("packet-size")
            .map(|value| value.parse::<usize>())
            .transpose()
            .map_err(|_| "invalid output packet size".to_string())?
            .unwrap_or(7 * TS_PACKET);

        if kind == TransportKind::Srt {
            options
                .entry("conntimeo".to_string())
                .or_insert_with(|| "3000".to_string());
            options
                .entry("poll-timeout".to_string())
                .or_insert_with(|| "100".to_string());
            options
                .entry("peeridletimeo".to_string())
                .or_insert_with(|| timeout.as_millis().max(1).to_string());
            uri.set_query(None);
            uri.query_pairs_mut().extend_pairs(options.iter());
        } else if kind != TransportKind::Rtmp {
            uri.set_query(None);
        }

        Ok(Self {
            kind,
            uri,
            properties: options,
            listener,
            timeout,
            packet_size,
        })
    }

    fn validate_format(&self, format: LinkFormat) -> Result<(), String> {
        match (self.kind, format) {
            (TransportKind::Rtmp, LinkFormat::Flv)
            | (TransportKind::Srt | TransportKind::Udp, LinkFormat::TransportStream)
            | (TransportKind::Tcp, _) => Ok(()),
            _ => Err("RTMP requires FLV; SRT and UDP require MPEG-TS".to_string()),
        }
    }
}

/// Normalizes legacy SRT option names/units before GStreamer opens a socket.
/// Used by ingest too, so preloaded and directly played sources agree.
pub(crate) fn normalize_srt_uri(
    address: &str,
    options: &BTreeMap<String, String>,
) -> Result<String, String> {
    let settings = TransportSettings::parse(address, options)?;
    if settings.kind != TransportKind::Srt {
        return Err("expected an SRT address".to_string());
    }

    Ok(settings.uri.into())
}

/// Validates configuration without connecting to the destination.
pub(crate) fn validate_transport_options(
    address: &str,
    options: &BTreeMap<String, String>,
) -> Result<(), String> {
    TransportSettings::parse(address, options).map(|_| ())
}

fn parse_u64(name: &str, value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("protocol option {name:?} requires a nonnegative integer"))
}

fn parse_i32(name: &str, value: &str) -> Result<i32, String> {
    value
        .parse()
        .map_err(|_| format!("protocol option {name:?} requires a 32-bit integer"))
}

fn bool_string(name: &str, value: &str) -> Result<String, String> {
    match value {
        "1" | "true" | "yes" | "on" => Ok("true".to_string()),
        "0" | "false" | "no" | "off" => Ok("false".to_string()),
        _ => Err(format!("protocol option {name:?} requires a boolean")),
    }
}

fn normalize_option(
    kind: TransportKind,
    name: &str,
    value: &str,
) -> Result<(String, String), String> {
    let mut value = value.to_string();
    let mapped = match kind {
        TransportKind::Srt => match name {
            "srt_streamid" => "streamid",
            "connect_timeout" => "conntimeo",
            "pkt_size" => "payloadsize",
            "enforced_encryption" => "enforcedencryption",
            "latency" | "rcvlatency" | "peerlatency" => {
                // Existing NoAr/FFmpeg configuration uses microseconds.
                let micros = parse_u64(name, &value)?;
                let millis = micros.div_ceil(1000);
                i32::try_from(millis)
                    .map_err(|_| format!("protocol option {name:?} is too large"))?;
                return Ok((name.to_string(), millis.to_string()));
            }
            "mode" => {
                value = match value.as_str() {
                    "caller" | "0" => "caller",
                    "listener" | "1" => "listener",
                    "rendezvous" | "rendez-vous" | "2" => "rendezvous",
                    _ => return Err("invalid SRT connection mode".to_string()),
                }
                .to_string();
                "mode"
            }
            "mss" | "fc" | "sndbuf" | "rcvbuf" | "maxbw" | "tsbpdmode" | "inputbw" | "oheadbw"
            | "passphrase" | "pbkeylen" | "ipttl" | "iptos" | "tlpktdrop" | "snddropdelay"
            | "nakreport" | "conntimeo" | "drifttracer" | "lossmaxttl" | "minversion"
            | "streamid" | "congestion" | "messageapi" | "payloadsize" | "transtype"
            | "kmrefreshrate" | "kmpreannounce" | "enforcedencryption" | "ipv6only"
            | "peeridletimeo" | "bindtodevice" | "packetfilter" | "retransmitalgo"
            | "localaddress" | "localport" | "poll-timeout" => name,
            "linger" if value == "0" => return Ok(("conntimeo".to_string(), "3000".to_string())),
            _ => return Err(format!("unsupported GStreamer SRT option {name:?}")),
        },
        TransportKind::Udp => match name {
            "ttl" | "ttl-mc" | "qos-dscp" => name,
            "buffer_size" | "buffer-size" => "buffer-size",
            "localaddr" | "bind-address" => "bind-address",
            "localport" | "bind-port" => "bind-port",
            "pkt_size" | "packet-size" => "packet-size",
            "broadcast" | "auto-multicast" | "loop" | "multicast-iface" => name,
            _ => return Err(format!("unsupported GStreamer UDP option {name:?}")),
        },
        TransportKind::Rtmp => match name {
            "rtmp_app" | "application" => "application",
            "rtmp_playpath" | "stream" => "stream",
            "rtmp_conn" | "extra-connect-args" => "extra-connect-args",
            "rtmp_flashver" | "flash-version" => "flash-version",
            "username" | "password" | "secure-token" | "chunk-size" | "peak-kbps" => name,
            "rtmp_live" if value == "live" => {
                return Ok(("async-connect".to_string(), "true".to_string()));
            }
            _ => return Err(format!("unsupported GStreamer RTMP option {name:?}")),
        },
        TransportKind::Tcp => match name {
            "listen" => name,
            _ => return Err(format!("unsupported GStreamer TCP option {name:?}")),
        },
    };

    match mapped {
        "mode" | "streamid" | "passphrase" | "congestion" | "localaddress" | "bindtodevice"
        | "packetfilter" | "bind-address" | "multicast-iface" | "application" | "stream"
        | "extra-connect-args" | "flash-version" | "username" | "password" | "secure-token" => {}
        "tsbpdmode" | "tlpktdrop" | "nakreport" | "drifttracer" | "messageapi"
        | "enforcedencryption" | "broadcast" | "auto-multicast" | "loop" | "listen" => {
            value = bool_string(name, &value)?;
        }
        "maxbw" | "inputbw" => {
            value
                .parse::<i64>()
                .map_err(|_| format!("protocol option {name:?} requires a 64-bit integer"))?;
        }
        "localport" | "bind-port" => {
            u16::try_from(parse_u64(name, &value)?)
                .map_err(|_| format!("invalid port in option {name:?}"))?;
        }
        "packet-size" => {
            let size = parse_u64(name, &value)?;
            if !(188..=65507).contains(&size) {
                return Err("UDP packet size must be between 188 and 65507 bytes".to_string());
            }
        }
        "chunk-size" | "peak-kbps" => {
            u32::try_from(parse_u64(name, &value)?)
                .map_err(|_| format!("protocol option {name:?} is too large"))?;
        }
        "pbkeylen" => {
            if !matches!(parse_i32(name, &value)?, 0 | 16 | 24 | 32) {
                return Err("SRT pbkeylen must be 0, 16, 24 or 32".to_string());
            }
        }
        "transtype" => {
            value = match value.as_str() {
                "live" | "0" => "0",
                "file" | "1" => "1",
                _ => return Err("invalid SRT transport type".to_string()),
            }
            .to_string();
        }
        _ => {
            parse_i32(name, &value)?;
        }
    }

    Ok((mapped.to_string(), value))
}

/// A GStreamer transport only: muxed payload is already produced upstream.
struct NativeTransport {
    pipeline: gst::Pipeline,
    source: gst_app::AppSrc,
    sink: gst::Element,
    bus: gst::Bus,
    shared: Arc<Shared>,
    kind: TransportKind,
    listener: bool,
    timeout: Duration,
    last_progress: Instant,
    last_sent: u64,
    wrote: bool,
    packet_size: usize,
}

impl NativeTransport {
    fn open(settings: &TransportSettings, format: LinkFormat, shared: Arc<Shared>) -> Result<Self> {
        let pipeline = gst::Pipeline::new();
        let caps = match format {
            LinkFormat::TransportStream => gst::Caps::builder("video/mpegts")
                .field("systemstream", true)
                .field("packetsize", 188_i32)
                .build(),
            LinkFormat::Flv => gst::Caps::builder("video/x-flv").build(),
        };
        let source = gst_app::AppSrc::builder()
            .caps(&caps)
            .is_live(true)
            .format(gst::Format::Bytes)
            .block(false)
            .max_bytes(MAX_QUEUED_BYTES as u64)
            .build();
        let factory = match settings.kind {
            TransportKind::Srt => "srtsink",
            TransportKind::Udp => "udpsink",
            TransportKind::Rtmp => "rtmp2sink",
            TransportKind::Tcp if settings.listener => "tcpserversink",
            TransportKind::Tcp => "tcpclientsink",
        };
        let sink = gst::ElementFactory::make(factory)
            .build()
            .with_context(|| format!("required GStreamer transport {factory} is unavailable"))?;
        sink.set_property("sync", false);
        sink.set_property("async", false);

        match settings.kind {
            TransportKind::Srt => {
                sink.set_property("uri", settings.uri.as_str());
                sink.set_property("auto-reconnect", false);
                sink.set_property("wait-for-connection", true);
            }
            TransportKind::Rtmp => {
                sink.set_property("location", settings.uri.as_str());
                sink.set_property("timeout", u32::try_from(settings.timeout.as_secs().max(1))?);
                for (name, value) in &settings.properties {
                    match name.as_str() {
                        "chunk-size" | "peak-kbps" => {
                            sink.set_property(name, value.parse::<u32>()?);
                        }
                        "async-connect" => sink.set_property(name, value == "true"),
                        _ => sink.set_property(name, value.as_str()),
                    }
                }
            }
            TransportKind::Udp | TransportKind::Tcp => {
                sink.set_property(
                    "host",
                    settings
                        .uri
                        .host_str()
                        .context("output URL requires a host")?,
                );
                sink.set_property(
                    "port",
                    i32::from(settings.uri.port().context("output URL requires a port")?),
                );
                for (name, value) in &settings.properties {
                    match name.as_str() {
                        "listen" | "packet-size" => {}
                        "bind-address" | "multicast-iface" => {
                            sink.set_property(name, value.as_str());
                        }
                        "broadcast" | "auto-multicast" | "loop" => {
                            anyhow::ensure!(
                                sink.find_property(name).is_some(),
                                "GStreamer transport does not support option {name:?}"
                            );
                            sink.set_property(name, value == "true");
                        }
                        _ => sink.set_property(name, value.parse::<i32>()?),
                    }
                }
            }
        }

        pipeline.add_many([source.upcast_ref::<gst::Element>(), &sink])?;
        source.link(&sink)?;
        let bus = pipeline.bus().context("transport pipeline has no bus")?;
        let transport = Self {
            pipeline,
            source,
            sink,
            bus,
            shared,
            kind: settings.kind,
            listener: settings.listener,
            timeout: settings.timeout,
            last_progress: Instant::now(),
            last_sent: 0,
            wrote: false,
            packet_size: settings.packet_size,
        };
        *lock(&transport.shared.pipeline) = Some(transport.pipeline.clone());
        transport
            .pipeline
            .set_state(gst::State::Playing)
            .context("could not start GStreamer transport")?;

        Ok(transport)
    }

    fn poll(&mut self) -> Result<(), String> {
        for message in self.bus.iter() {
            match message.view() {
                gst::MessageView::Error(error) => return Err(error.error().message().to_string()),
                gst::MessageView::Eos(_) => {
                    return Err("GStreamer transport ended unexpectedly".to_string());
                }
                _ => {}
            }
        }

        let sent = self.sent_bytes();
        if sent > self.last_sent || self.source.current_level_bytes() == 0 {
            self.last_sent = sent;
            self.last_progress = Instant::now();
        }

        if self.wrote
            && self.last_progress.elapsed() > self.timeout
            && (self.source.current_level_bytes() > 0 || !self.connected())
        {
            return Err("GStreamer transport exceeded its output timeout".to_string());
        }

        Ok(())
    }

    fn sent_bytes(&self) -> u64 {
        match self.kind {
            TransportKind::Srt | TransportKind::Rtmp => {
                let stats = self.sink.property::<gst::Structure>("stats");
                stats
                    .get::<u64>(if self.kind == TransportKind::Srt {
                        "bytes-sent-total"
                    } else {
                        "out-bytes-total"
                    })
                    .unwrap_or(0)
            }
            TransportKind::Udp | TransportKind::Tcp => self.last_sent,
        }
    }

    fn connected(&self) -> bool {
        match self.kind {
            TransportKind::Srt | TransportKind::Rtmp => self.sent_bytes() > 0,
            TransportKind::Tcp if self.listener => self.sink.property::<u32>("num-handles") > 0,
            TransportKind::Tcp | TransportKind::Udp => {
                self.pipeline.current_state() == gst::State::Playing
            }
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            return Ok(());
        }

        while self.source.current_level_bytes() >= MAX_QUEUED_BYTES as u64 {
            if self.shared.stop.load(Ordering::Acquire) {
                return Err("output closed".to_string());
            }
            self.poll()?;
            thread::sleep(Duration::from_millis(10));
        }

        if !self.wrote {
            self.last_progress = Instant::now();
            self.wrote = true;
        }

        let chunk_size = if self.kind == TransportKind::Udp {
            self.packet_size
        } else {
            data.len()
        };
        for chunk in data.chunks(chunk_size) {
            self.source
                .push_buffer(gst::Buffer::from_slice(chunk.to_vec()))
                .map_err(|_| "GStreamer transport rejected output data".to_string())?;
        }

        if matches!(self.kind, TransportKind::Udp | TransportKind::Tcp) {
            self.last_progress = Instant::now();
        }

        self.poll()
    }
}

impl Drop for NativeTransport {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
        lock(&self.shared.pipeline).take();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
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
            pipeline: Mutex::new(None),
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
