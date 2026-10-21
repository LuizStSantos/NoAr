//! SCTE-35 ad break signalling on MPEG-TS outputs.
//!
//! FFmpeg's `mpegts` muxer cannot write SCTE-35, so NoAr adds it between the
//! muxer and the network. [`TsInjector`] lists an SCTE-35 stream in the PMT
//! and inserts `splice_info_section`s on that PID ahead of each break.
//!
//! NoAr always signals breaks the same way, the form downstream systems
//! accept most widely (SCTE 35 and the SCTE 67 practice): a `splice_insert`
//! out of the network with the break duration and auto return, sent a few
//! seconds ahead and repeated, and a `splice_insert` back into the network
//! at the end. Both splice points are key frames the encoder is told to make.
//! Adapting the signal to each platform is left to what receives the stream.

use std::{
    collections::VecDeque,
    ffi::CString,
    fmt, io, ptr,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime},
};

use anyhow::{Result, bail};
use ffmpeg_next::{
    self as ffmpeg, Dictionary, ffi,
    format::{self, context::StreamIo},
};

use crate::utils::helper::{is_network_url, network_output_options};

/// The PID NoAr carries SCTE-35 on unless set otherwise.
pub const DEFAULT_SCTE35_PID: u16 = 500;
/// How far ahead of a splice point its message is first sent.
pub const DEFAULT_SCTE35_PREROLL_SECONDS: f64 = 6.0;

const TS_PACKET: usize = 188;
const PTS_CLOCK: i64 = 90_000;
const PTS_MODULO: i64 = 1 << 33;
/// Each message is sent this many times, a second apart, so one lost packet
/// does not lose the break.
const COPIES: u8 = 3;
/// Breaks kept for the status after they end.
const HISTORY: usize = 20;

/// SCTE-35 on a stream output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scte35Config {
    /// The PID the splice messages are carried on.
    pub pid: u16,
    /// Seconds between the first message of a break and its splice point.
    pub preroll_seconds: f64,
}

impl Default for Scte35Config {
    fn default() -> Self {
        Self {
            pid: DEFAULT_SCTE35_PID,
            preroll_seconds: DEFAULT_SCTE35_PREROLL_SECONDS,
        }
    }
}

impl Scte35Config {
    pub fn validate(&self) -> Result<(), String> {
        if !(0x20..=0x1FFE).contains(&self.pid) {
            return Err(format!(
                "SCTE-35 PID {} must be between 32 and 8190",
                self.pid
            ));
        }

        if !self.preroll_seconds.is_finite() || !(0.0..=30.0).contains(&self.preroll_seconds) {
            return Err("SCTE-35 pre-roll must be between 0 and 30 seconds".to_string());
        }

        Ok(())
    }
}

/// What made a break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakSource {
    /// A run of commercials in the playlist or in a scheduled event.
    Commercials,
    /// Asked for by hand, from the interface or the API.
    Manual,
}

/// Where a break is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakState {
    /// Known, its first message not sent yet.
    Scheduled,
    /// Its message is out; the splice point has not come yet.
    Announced,
    OnAir,
    Ended,
    /// Called off before it started. A cancel message went out if the break
    /// had already been announced.
    Cancelled,
}

/// A break as the status shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakInfo {
    pub event_id: u32,
    pub source: BreakSource,
    pub state: BreakState,
    /// Planned length in seconds.
    pub duration: f64,
    /// When the break starts and ends, by the wall clock.
    pub starts_at: SystemTime,
    pub ends_at: SystemTime,
    /// Messages sent for the start and for the return.
    pub out_messages: u8,
    pub in_messages: u8,
    /// Whether the frame at the splice point went out as a key frame, once
    /// it has gone out.
    pub out_key_frame: Option<bool>,
    pub in_key_frame: Option<bool>,
}

/// What the SCTE-35 signal is doing.
#[derive(Debug, Clone, PartialEq)]
pub struct SpliceStatus {
    /// An output that carries SCTE-35 is open.
    pub active: bool,
    pub pid: u16,
    pub preroll_seconds: f64,
    /// Why the output carries no SCTE-35 although it should.
    pub error: Option<String>,
    /// Breaks to come and on air, in order.
    pub breaks: Vec<BreakInfo>,
    /// Breaks that ended or were cancelled, newest first.
    pub recent: Vec<BreakInfo>,
}

/// Breaks around the next clip to play, as the playlist knows them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClipBreaks {
    /// A break of this many seconds starts with the clip; it can only be
    /// announced as it starts.
    pub at_start: Option<f64>,
    /// A break of this many seconds starts when the clip ends; it is
    /// announced the pre-roll ahead.
    pub after: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpliceError {
    /// No output carries SCTE-35.
    Inactive,
    InvalidDuration,
    /// Another break overlaps the one asked for.
    Overlaps(u32),
    NotFound(u32),
    /// The break has started and can no longer be called off.
    Started(u32),
}

impl fmt::Display for SpliceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive => f.write_str("no output carries SCTE-35"),
            Self::InvalidDuration => f.write_str("break duration must be a positive number"),
            Self::Overlaps(id) => write!(f, "the break overlaps break {id}"),
            Self::NotFound(id) => write!(f, "break {id} is not scheduled"),
            Self::Started(id) => write!(f, "break {id} has already started"),
        }
    }
}

impl std::error::Error for SpliceError {}

/// The ad breaks a channel signals. Clones share the same state, which
/// outlasts a playout restart, like the logo control.
#[derive(Clone, Default)]
pub struct SpliceControl(Arc<Mutex<SpliceState>>);

impl fmt::Debug for SpliceControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SpliceControl")
            .field(&self.lock().active)
            .finish()
    }
}

#[derive(Default)]
struct SpliceState {
    active: bool,
    error: Option<String>,
    config: Scte35Config,
    fps: u32,
    /// The output video time base, as `(num, den)`.
    time_base: (i64, i64),
    /// What the muxer adds to every timestamp, in 90 kHz: its delay and the
    /// shift that keeps audio priming from going negative. Measured, by
    /// matching the video packets given to the muxer with the PES it writes.
    mux_delay: i64,
    /// Timestamps of video packets given to the muxer and not yet seen in
    /// the transport stream, in 90 kHz.
    muxed_video: VecDeque<i64>,
    /// The newest frame given to the encoder.
    last_frame: Option<i64>,
    next_event_id: u32,
    plan: ClipBreaks,
    breaks: Vec<Break>,
    recent: VecDeque<BreakInfo>,
}

#[derive(Debug, Clone)]
struct Break {
    event_id: u32,
    source: BreakSource,
    /// Splice points, as output video frames.
    start: i64,
    end: i64,
    starts_at: SystemTime,
    ends_at: SystemTime,
    out_sent: u8,
    in_sent: u8,
    cancelled: bool,
    cancel_sent: bool,
    on_air: bool,
    out_key_frame: Option<bool>,
    in_key_frame: Option<bool>,
}

impl SpliceControl {
    fn lock(&self) -> MutexGuard<'_, SpliceState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn status(&self) -> SpliceStatus {
        let state = self.lock();

        SpliceStatus {
            active: state.active,
            pid: state.config.pid,
            preroll_seconds: state.config.preroll_seconds,
            error: state.error.clone(),
            breaks: state.breaks.iter().map(|b| state.info(b)).collect(),
            recent: state.recent.iter().cloned().collect(),
        }
    }

    /// Breaks around the clip the playout plays next. Taken when it starts.
    pub fn plan_next_clip(&self, breaks: ClipBreaks) {
        self.lock().plan = breaks;
    }

    /// Signal a break of `duration` seconds, starting the pre-roll from now.
    pub fn insert_break(&self, duration: f64) -> Result<BreakInfo, SpliceError> {
        let mut state = self.lock();

        if !state.active {
            return Err(SpliceError::Inactive);
        }

        let frames = state.frames(duration).ok_or(SpliceError::InvalidDuration)?;
        let start = state.last_frame.map_or(0, |frame| frame + 1) + state.preroll_frames().max(1);
        let id = state.schedule(start, start + frames, BreakSource::Manual)?;

        Ok(state.info_by_id(id).expect("just scheduled"))
    }

    /// Call off a break that has not started. Downstream gets a cancel if it
    /// was already announced.
    pub fn cancel_break(&self, event_id: u32) -> Result<(), SpliceError> {
        let mut state = self.lock();
        let last_frame = state.last_frame.unwrap_or(-1);
        let found = state
            .breaks
            .iter_mut()
            .find(|b| b.event_id == event_id && !b.cancelled)
            .ok_or(SpliceError::NotFound(event_id))?;

        if found.start <= last_frame {
            return Err(SpliceError::Started(event_id));
        }

        found.cancelled = true;
        state.retire_cancelled();

        Ok(())
    }

    pub(crate) fn open_output(&self, config: Scte35Config, fps: u32, time_base: (i64, i64)) {
        let mut state = self.lock();

        // A new output starts its timeline over; breaks of the last one can
        // no longer happen.
        for b in &mut state.breaks {
            b.cancelled = true;
        }
        state.retire_cancelled();
        state.active = true;
        state.error = None;
        state.config = config;
        state.fps = fps.max(1);
        state.time_base = time_base;
        state.mux_delay = 0;
        state.muxed_video.clear();
        state.last_frame = None;
        state.plan = ClipBreaks::default();

        if state.next_event_id == 0 {
            // Unique across restarts: event IDs are not reused for a while.
            state.next_event_id = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(1, |since| since.as_secs() as u32)
                .max(1);
        }
    }

    /// The output gave the muxer a video packet with this 90 kHz timestamp.
    pub(crate) fn video_packet_muxed(&self, pts: i64) {
        let mut state = self.lock();

        if state.active && state.muxed_video.len() < 1_000 {
            state.muxed_video.push_back(pts);
        }
    }

    pub(crate) fn fail(&self, error: String) {
        let mut state = self.lock();

        if state.error.is_none() {
            log::error!("SCTE-35 is off on this output: {error}");
        }
        state.error = Some(error);
    }

    /// The output cannot carry SCTE-35, and says why.
    pub(crate) fn unavailable(&self, reason: String) {
        let mut state = self.lock();

        for b in &mut state.breaks {
            b.cancelled = true;
        }
        state.retire_cancelled();
        state.active = false;
        state.error = Some(reason);
    }

    pub(crate) fn close_output(&self) {
        let mut state = self.lock();

        for b in &mut state.breaks {
            b.cancelled = true;
        }
        state.retire_cancelled();
        state.active = false;
    }

    /// A clip starts at output frame `start`. Schedules the breaks planned
    /// for it, and returns the one that follows it, to call off if the clip
    /// does not play to its end.
    pub(crate) fn clip_started(&self, start: i64, duration_seconds: Option<f64>) -> Option<u32> {
        let mut state = self.lock();
        let plan = std::mem::take(&mut state.plan);

        if !state.active {
            return None;
        }

        if let Some(frames) = plan.at_start.and_then(|seconds| state.frames(seconds)) {
            match state.schedule(start, start + frames, BreakSource::Commercials) {
                Ok(id) => {
                    log::info!("SCTE-35: break {id} starts with this clip; announced as it starts");
                }
                Err(SpliceError::Overlaps(_)) => {}
                Err(error) => log::warn!("SCTE-35: break not signalled: {error}"),
            }
        }

        let after = plan.after.and_then(|seconds| state.frames(seconds))?;
        let Some(clip_frames) = duration_seconds.and_then(|seconds| state.frames(seconds)) else {
            log::warn!("SCTE-35: the clip before a break has no known length; break not signalled");
            return None;
        };
        let splice = start + clip_frames;

        match state.schedule(splice, splice + after, BreakSource::Commercials) {
            Ok(id) => Some(id),
            Err(error) => {
                log::warn!("SCTE-35: break not signalled: {error}");
                None
            }
        }
    }

    /// The clip a break followed ended. If it did not play to its end, the
    /// break has not come and is called off.
    pub(crate) fn clip_finished(&self, following_break: Option<u32>, played: bool) {
        let Some(id) = following_break else {
            return;
        };

        if !played && self.cancel_break(id).is_ok() {
            log::info!("SCTE-35: break {id} called off; the clip before it was cut short");
        }
    }

    /// The encoder gets the frame with this timestamp next. Answers whether
    /// it must be a key frame, because a break starts or ends on it.
    pub(crate) fn frame_needs_key_frame(&self, pts: i64) -> bool {
        let mut state = self.lock();

        state.last_frame = Some(pts);
        state.active
            && state
                .breaks
                .iter()
                .any(|b| !b.cancelled && (b.start == pts || b.end == pts))
    }

    /// The muxer wrote a video PES with this timestamp. Returns the messages
    /// due before it.
    pub(crate) fn messages_due(&self, now: i64, key_frame: bool) -> Vec<Vec<u8>> {
        let mut state = self.lock();
        let mut messages = Vec::new();

        if !state.active || state.error.is_some() {
            return messages;
        }

        // The muxer writes video PES in the order it was given the packets.
        if let Some(muxed) = state.muxed_video.pop_front() {
            state.mux_delay = pts_diff(now, muxed);
        }

        let preroll = (state.config.preroll_seconds * PTS_CLOCK as f64).round() as i64;
        let pts_of_start: Vec<(i64, i64)> = state
            .breaks
            .iter()
            .map(|b| (state.pts(b.start), state.pts(b.end)))
            .collect();

        for (b, (start, end)) in state.breaks.iter_mut().zip(pts_of_start) {
            let to_start = pts_diff(start, now);
            let to_end = pts_diff(end, now);

            if b.cancelled {
                if b.out_sent > 0 && !b.cancel_sent && to_start > 0 {
                    messages.push(splice_insert_cancel(b.event_id));
                    b.cancel_sent = true;
                }
                continue;
            }

            if to_start == 0 {
                b.out_key_frame = Some(key_frame);
            }
            if to_end == 0 {
                b.in_key_frame = Some(key_frame);
            }

            let duration = pts_diff(end, start);

            if b.out_sent < COPIES
                && to_start >= 0
                && to_start <= preroll - i64::from(b.out_sent) * PTS_CLOCK
            {
                messages.push(splice_insert(&SpliceInsert {
                    event_id: b.event_id,
                    out_of_network: true,
                    splice_time: start,
                    duration: Some(duration),
                }));
                b.out_sent += 1;
            }

            if b.in_sent < COPIES
                && to_end >= 0
                && to_end <= preroll - i64::from(b.in_sent) * PTS_CLOCK
            {
                messages.push(splice_insert(&SpliceInsert {
                    event_id: b.event_id,
                    out_of_network: false,
                    splice_time: end,
                    duration: None,
                }));
                b.in_sent += 1;
            }

            if !b.on_air && to_start <= 0 {
                b.on_air = true;
                log::info!(
                    "SCTE-35: break {} on air for {:.1} s",
                    b.event_id,
                    duration as f64 / PTS_CLOCK as f64
                );
            }
        }

        let ended: Vec<Break> = {
            let (ended, kept): (Vec<Break>, Vec<Break>) = std::mem::take(&mut state.breaks)
                .into_iter()
                .partition(|b| {
                    let start = state.pts(b.start);
                    let end = state.pts(b.end);

                    // A cancel goes out before the splice point or not at all.
                    (b.cancelled && (b.cancel_sent || b.out_sent == 0 || pts_diff(start, now) <= 0))
                        || (!b.cancelled && pts_diff(now, end) > 0)
                });
            state.breaks = kept;
            ended
        };

        for b in ended {
            if !b.cancelled {
                log::info!("SCTE-35: break {} ended", b.event_id);
            }
            state.remember(&b);
        }

        messages
    }
}

impl SpliceState {
    fn frames(&self, seconds: f64) -> Option<i64> {
        (seconds.is_finite() && seconds > 0.0 && self.fps > 0).then(|| {
            let micros = i128::from((seconds * 1_000_000.0).round() as i64);
            ((micros * i128::from(self.fps) + 999_999) / 1_000_000) as i64
        })
    }

    fn preroll_frames(&self) -> i64 {
        self.frames(self.config.preroll_seconds).unwrap_or(0)
    }

    /// An output frame as a 90 kHz timestamp of the transport stream.
    fn pts(&self, frame: i64) -> i64 {
        let (num, den) = self.time_base;
        let pts =
            i128::from(frame) * i128::from(PTS_CLOCK) * i128::from(num) / i128::from(den.max(1));

        (pts as i64 + self.mux_delay).rem_euclid(PTS_MODULO)
    }

    fn wall_time(&self, frame: i64) -> SystemTime {
        let now = SystemTime::now();
        let from = self.last_frame.unwrap_or(0);
        let seconds = (frame - from) as f64 / f64::from(self.fps.max(1));

        if seconds >= 0.0 {
            now + Duration::from_secs_f64(seconds)
        } else {
            now - Duration::from_secs_f64(-seconds)
        }
    }

    fn schedule(&mut self, start: i64, end: i64, source: BreakSource) -> Result<u32, SpliceError> {
        if let Some(other) = self
            .breaks
            .iter()
            .find(|b| !b.cancelled && start < b.end && b.start < end)
        {
            return Err(SpliceError::Overlaps(other.event_id));
        }

        let event_id = self.next_event_id.max(1);
        self.next_event_id = event_id.wrapping_add(1).max(1);

        let duration = (end - start) as f64 / f64::from(self.fps.max(1));
        log::info!(
            "SCTE-35: break {event_id} of {duration:.1} s scheduled, {:.1} s from now",
            (start - self.last_frame.unwrap_or(0)) as f64 / f64::from(self.fps.max(1))
        );

        let b = Break {
            event_id,
            source,
            start,
            end,
            starts_at: self.wall_time(start),
            ends_at: self.wall_time(end),
            out_sent: 0,
            in_sent: 0,
            cancelled: false,
            cancel_sent: false,
            on_air: false,
            out_key_frame: None,
            in_key_frame: None,
        };
        let at = self.breaks.partition_point(|other| other.start <= start);
        self.breaks.insert(at, b);

        Ok(event_id)
    }

    /// Cancelled breaks that were never announced go straight to the history.
    fn retire_cancelled(&mut self) {
        let (gone, kept): (Vec<Break>, Vec<Break>) = std::mem::take(&mut self.breaks)
            .into_iter()
            .partition(|b| b.cancelled && b.out_sent == 0);
        self.breaks = kept;

        for b in gone {
            self.remember(&b);
        }
    }

    fn remember(&mut self, b: &Break) {
        let mut info = self.info(b);
        info.state = if b.cancelled {
            BreakState::Cancelled
        } else {
            BreakState::Ended
        };
        self.recent.push_front(info);
        self.recent.truncate(HISTORY);
    }

    fn info(&self, b: &Break) -> BreakInfo {
        BreakInfo {
            event_id: b.event_id,
            source: b.source,
            state: if b.cancelled {
                BreakState::Cancelled
            } else if b.on_air {
                BreakState::OnAir
            } else if b.out_sent > 0 {
                BreakState::Announced
            } else {
                BreakState::Scheduled
            },
            duration: (b.end - b.start) as f64 / f64::from(self.fps.max(1)),
            starts_at: b.starts_at,
            ends_at: b.ends_at,
            out_messages: b.out_sent,
            in_messages: b.in_sent,
            out_key_frame: b.out_key_frame,
            in_key_frame: b.in_key_frame,
        }
    }

    fn info_by_id(&self, event_id: u32) -> Option<BreakInfo> {
        self.breaks
            .iter()
            .find(|b| b.event_id == event_id)
            .map(|b| self.info(b))
    }
}

/// `a - b` for 33-bit timestamps that wrap.
fn pts_diff(a: i64, b: i64) -> i64 {
    let diff = (a - b).rem_euclid(PTS_MODULO);

    if diff >= PTS_MODULO / 2 {
        diff - PTS_MODULO
    } else {
        diff
    }
}

/// A `splice_insert` command for the whole programme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpliceInsert {
    pub(crate) event_id: u32,
    /// Leaving the network for a break, or returning to it.
    pub(crate) out_of_network: bool,
    /// The 90 kHz PTS of the splice point.
    pub(crate) splice_time: i64,
    /// The break length in 90 kHz, with auto return.
    pub(crate) duration: Option<i64>,
}

/// A `splice_info_section` carrying a `splice_insert`.
pub(crate) fn splice_insert(command: &SpliceInsert) -> Vec<u8> {
    let mut body = Vec::with_capacity(20);

    body.extend_from_slice(&command.event_id.to_be_bytes());
    // splice_event_cancel_indicator 0, then reserved bits (1).
    body.push(0x7F);
    // out_of_network_indicator, program_splice_flag 1, duration_flag,
    // splice_immediate_flag 0, then reserved bits (1).
    body.push(
        (u8::from(command.out_of_network) << 7)
            | 0x40
            | (u8::from(command.duration.is_some()) << 5)
            | 0x0F,
    );
    body.extend_from_slice(&time_field(true, command.splice_time));

    if let Some(duration) = command.duration {
        // break_duration with auto_return set.
        body.extend_from_slice(&time_field(true, duration));
    }

    // unique_program_id, avail_num, avails_expected.
    body.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);

    splice_info_section(0x05, &body)
}

/// A `splice_insert` that calls off an announced event.
pub(crate) fn splice_insert_cancel(event_id: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(5);

    body.extend_from_slice(&event_id.to_be_bytes());
    body.push(0xFF);

    splice_info_section(0x05, &body)
}

/// A flag bit, six reserved bits and a 33-bit time.
fn time_field(flag: bool, value: i64) -> [u8; 5] {
    let value = value.rem_euclid(PTS_MODULO) as u64;

    [
        (u8::from(flag) << 7) | 0x7E | ((value >> 32) as u8 & 0x01),
        (value >> 24) as u8,
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ]
}

fn splice_info_section(command_type: u8, command: &[u8]) -> Vec<u8> {
    // From protocol_version to the end of the CRC: ten bytes of header, the
    // command type and command, the descriptor loop length and the CRC.
    let section_length = 10 + 1 + command.len() + 2 + 4;
    let command_length = command.len() + 1;
    let mut section = Vec::with_capacity(3 + section_length);

    section.push(0xFC);
    // section_syntax_indicator 0, private_indicator 0, sap_type 3 (not
    // specified), section_length.
    section.push(0x30 | ((section_length >> 8) as u8 & 0x0F));
    section.push(section_length as u8);
    // protocol_version.
    section.push(0x00);
    // encrypted_packet 0, encryption_algorithm 0, pts_adjustment 0.
    section.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00]);
    // cw_index.
    section.push(0x00);
    // tier 0xFFF (all), splice_command_length.
    section.push(0xFF);
    section.push(0xF0 | ((command_length >> 8) as u8 & 0x0F));
    section.push(command_length as u8);
    section.push(command_type);
    section.extend_from_slice(command);
    // descriptor_loop_length.
    section.extend_from_slice(&[0x00, 0x00]);

    let crc = crc32_mpeg2(&section);
    section.extend_from_slice(&crc.to_be_bytes());

    section
}

/// The CRC-32 of MPEG-2 sections (polynomial 0x04C11DB7, no reflection).
pub(crate) fn crc32_mpeg2(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;

    for &byte in data {
        crc ^= u32::from(byte) << 24;

        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }

    crc
}

/// Adds SCTE-35 to a transport stream on its way out: lists the stream in
/// the PMT, and inserts the messages the control has due before the video
/// packet that reaches their time.
pub(crate) struct TsInjector {
    control: SpliceControl,
    pid: u16,
    pmt_pid: Option<u16>,
    video_pid: Option<u16>,
    continuity: u8,
    /// The last PMT packet as the muxer wrote it, and as rewritten.
    pmt_cache: Option<([u8; TS_PACKET], [u8; TS_PACKET])>,
    /// Bytes of a packet split across writes.
    partial: Vec<u8>,
    failed: bool,
}

impl TsInjector {
    pub(crate) fn new(control: SpliceControl, pid: u16) -> Self {
        Self {
            control,
            pid,
            pmt_pid: None,
            video_pid: None,
            continuity: 0,
            pmt_cache: None,
            partial: Vec::with_capacity(TS_PACKET),
            failed: false,
        }
    }

    /// Takes what the muxer wrote and returns what goes out.
    pub(crate) fn process(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() + TS_PACKET);

        if self.failed {
            out.extend_from_slice(data);
            return out;
        }

        let mut data = data;

        if !self.partial.is_empty() {
            let needed = TS_PACKET - self.partial.len();
            let take = needed.min(data.len());
            self.partial.extend_from_slice(&data[..take]);
            data = &data[take..];

            if self.partial.len() < TS_PACKET {
                return out;
            }

            let packet: [u8; TS_PACKET] = self.partial[..].try_into().expect("full packet");
            self.partial.clear();
            self.packet(&packet, &mut out);
        }

        let mut chunks = data.chunks_exact(TS_PACKET);

        for chunk in &mut chunks {
            let packet: [u8; TS_PACKET] = chunk.try_into().expect("exact chunk");
            self.packet(&packet, &mut out);

            if self.failed {
                // Hand on the rest untouched.
                out.extend_from_slice(chunks.remainder());
                return out;
            }
        }

        self.partial.extend_from_slice(chunks.remainder());

        out
    }

    fn fail(&mut self, error: String) {
        self.failed = true;
        self.control.fail(error);
    }

    fn packet(&mut self, packet: &[u8; TS_PACKET], out: &mut Vec<u8>) {
        if packet[0] != 0x47 {
            out.extend_from_slice(packet);
            self.fail("the output is not a 188-byte transport stream".to_string());
            return;
        }

        let pid = (u16::from(packet[1] & 0x1F) << 8) | u16::from(packet[2]);
        let unit_start = packet[1] & 0x40 != 0;

        if pid == self.pid {
            out.extend_from_slice(packet);
            self.fail(format!("PID {} is already used by the muxer", self.pid));
            return;
        }

        if pid == 0 && unit_start {
            if let Some(pmt_pid) = pat_first_pmt_pid(packet) {
                self.pmt_pid = Some(pmt_pid);
            }
        } else if Some(pid) == self.pmt_pid && unit_start {
            out.extend_from_slice(&self.rewrite_pmt(packet));
            return;
        } else if Some(pid) == self.video_pid
            && unit_start
            && let Some((pts, key_frame)) = pes_pts(packet)
        {
            for section in self.control.messages_due(pts, key_frame) {
                self.section_packet(&section, out);
            }
        }

        out.extend_from_slice(packet);
    }

    fn section_packet(&mut self, section: &[u8], out: &mut Vec<u8>) {
        let mut packet = [0xFF_u8; TS_PACKET];

        packet[0] = 0x47;
        packet[1] = 0x40 | ((self.pid >> 8) as u8 & 0x1F);
        packet[2] = self.pid as u8;
        packet[3] = 0x10 | self.continuity;
        packet[4] = 0x00;
        packet[5..5 + section.len()].copy_from_slice(section);
        self.continuity = (self.continuity + 1) & 0x0F;
        out.extend_from_slice(&packet);
    }

    fn rewrite_pmt(&mut self, packet: &[u8; TS_PACKET]) -> [u8; TS_PACKET] {
        // The muxer repeats the same PMT; only its continuity counter moves.
        if let Some((original, rewritten)) = &self.pmt_cache
            && original[4..] == packet[4..]
            && original[..3] == packet[..3]
        {
            let mut rewritten = *rewritten;
            rewritten[3] = packet[3];
            return rewritten;
        }

        match add_scte35_to_pmt(packet, self.pid) {
            Ok((rewritten, video_pid)) => {
                self.video_pid = video_pid;
                self.pmt_cache = Some((*packet, rewritten));
                rewritten
            }
            Err(error) => {
                self.fail(error);
                *packet
            }
        }
    }
}

fn payload_start(packet: &[u8; TS_PACKET]) -> Option<usize> {
    let adaptation = (packet[3] >> 4) & 0x03;

    match adaptation {
        0x01 => Some(4),
        0x03 => {
            let start = 5 + usize::from(packet[4]);
            (start < TS_PACKET).then_some(start)
        }
        _ => None,
    }
}

/// The section a PSI packet starts, after its pointer field.
fn psi_section(packet: &[u8; TS_PACKET]) -> Option<(usize, &[u8])> {
    let start = payload_start(packet)?;
    let pointer = usize::from(packet[start]);
    let section_start = start + 1 + pointer;
    let section = packet.get(section_start..)?;

    if section.len() < 3 {
        return None;
    }

    let length = 3 + ((usize::from(section[1] & 0x0F) << 8) | usize::from(section[2]));

    section
        .get(..length)
        .map(|section| (section_start, section))
}

fn pat_first_pmt_pid(packet: &[u8; TS_PACKET]) -> Option<u16> {
    let (_, section) = psi_section(packet)?;

    if section[0] != 0x00 || section.len() < 12 {
        return None;
    }

    section[8..section.len() - 4]
        .chunks_exact(4)
        .find(|program| program[0] != 0 || program[1] != 0)
        .map(|program| (u16::from(program[2] & 0x1F) << 8) | u16::from(program[3]))
}

const VIDEO_STREAM_TYPES: [u8; 6] = [0x01, 0x02, 0x10, 0x1B, 0x24, 0x33];

/// The PMT with an SCTE-35 stream on `pid` and the `CUEI` registration
/// descriptor, and the PID of its video.
fn add_scte35_to_pmt(
    packet: &[u8; TS_PACKET],
    pid: u16,
) -> Result<([u8; TS_PACKET], Option<u16>), String> {
    let (section_start, section) =
        psi_section(packet).ok_or_else(|| "the PMT does not fit in one packet".to_string())?;

    if section[0] != 0x02 || section.len() < 16 {
        return Err("unexpected PMT".to_string());
    }

    if section_start != payload_start(packet).unwrap_or(0) + 1 {
        return Err("the PMT shares its packet with another section".to_string());
    }

    let program_info_length = (usize::from(section[10] & 0x0F) << 8) | usize::from(section[11]);
    let program_info = section
        .get(12..12 + program_info_length)
        .ok_or_else(|| "malformed PMT".to_string())?;
    let streams = section
        .get(12 + program_info_length..section.len() - 4)
        .ok_or_else(|| "malformed PMT".to_string())?;
    let mut video_pid = None;
    let mut offset = 0;

    while offset + 5 <= streams.len() {
        let stream_type = streams[offset];
        let es_pid = (u16::from(streams[offset + 1] & 0x1F) << 8) | u16::from(streams[offset + 2]);
        let info_length =
            (usize::from(streams[offset + 3] & 0x0F) << 8) | usize::from(streams[offset + 4]);

        if es_pid == pid {
            return Err(format!("PID {pid} is already used by the muxer"));
        }
        if video_pid.is_none() && VIDEO_STREAM_TYPES.contains(&stream_type) {
            video_pid = Some(es_pid);
        }

        offset += 5 + info_length;
    }

    let pcr_pid = (u16::from(section[8] & 0x1F) << 8) | u16::from(section[9]);

    if pcr_pid == pid {
        return Err(format!("PID {pid} is already used by the muxer"));
    }

    let has_cuei = program_info
        .windows(6)
        .any(|descriptor| descriptor == [0x05, 0x04, b'C', b'U', b'E', b'I']);
    let mut rewritten = Vec::with_capacity(section.len() + 14);

    rewritten.extend_from_slice(&section[..10]);
    let new_info_length = program_info_length + if has_cuei { 0 } else { 6 };
    rewritten.push(0xF0 | ((new_info_length >> 8) as u8 & 0x0F));
    rewritten.push(new_info_length as u8);
    rewritten.extend_from_slice(program_info);

    if !has_cuei {
        rewritten.extend_from_slice(&[0x05, 0x04, b'C', b'U', b'E', b'I']);
    }

    rewritten.extend_from_slice(streams);
    // stream_type 0x86 on `pid`, with a cue_identifier_descriptor: the
    // stream carries splice_insert, splice_null and splice_schedule.
    rewritten.extend_from_slice(&[
        0x86,
        0xE0 | ((pid >> 8) as u8 & 0x1F),
        pid as u8,
        0xF0,
        0x03,
        0x8A,
        0x01,
        0x00,
    ]);

    let section_length = rewritten.len() + 4 - 3;
    rewritten[1] = (rewritten[1] & 0xF0) | ((section_length >> 8) as u8 & 0x0F);
    rewritten[2] = section_length as u8;
    let crc = crc32_mpeg2(&rewritten);
    rewritten.extend_from_slice(&crc.to_be_bytes());

    if section_start + rewritten.len() > TS_PACKET {
        return Err("the PMT with SCTE-35 does not fit in one packet".to_string());
    }

    let mut out = [0xFF_u8; TS_PACKET];
    out[..section_start].copy_from_slice(&packet[..section_start]);
    out[section_start..section_start + rewritten.len()].copy_from_slice(&rewritten);

    Ok((out, video_pid))
}

/// The PTS of the PES a video packet starts, and whether it is a random
/// access point.
fn pes_pts(packet: &[u8; TS_PACKET]) -> Option<(i64, bool)> {
    let start = payload_start(packet)?;
    let key_frame = (packet[3] & 0x20) != 0 && packet[4] > 0 && (packet[5] & 0x40) != 0;
    let pes = packet.get(start..)?;

    if pes.len() < 14 || pes[..3] != [0x00, 0x00, 0x01] || pes[7] & 0x80 == 0 {
        return None;
    }

    let pts = (i64::from((pes[9] >> 1) & 0x07) << 30)
        | (i64::from(pes[10]) << 22)
        | (i64::from(pes[11] >> 1) << 15)
        | (i64::from(pes[12]) << 7)
        | i64::from(pes[13] >> 1);

    Some((pts, key_frame))
}

/// Opens a stream output whose transport stream gains SCTE-35 on its way to
/// `path`, a network address or a file.
pub(crate) fn open_output(
    path: &str,
    muxer: &str,
    protocol_options: &std::collections::BTreeMap<String, String>,
    control: SpliceControl,
    pid: u16,
) -> Result<format::context::Output> {
    let options = if is_network_url(path) {
        network_output_options(protocol_options)
    } else {
        Dictionary::new()
    };
    let (sink, remaining) = Scte35Sink::open(path, options, control, pid)?;
    let unused: Vec<_> = remaining
        .iter()
        .filter(|(key, _)| protocol_options.contains_key(*key))
        .map(|(key, _)| key)
        .collect();

    if !unused.is_empty() {
        bail!("unused output protocol options: {}", unused.join(", "));
    }

    // Seven transport packets, the size SRT and UDP send at a time.
    let io = StreamIo::from_write_with_capacity(sink, 7 * TS_PACKET)?;

    Ok(format::output_to_stream(io, Some(path), Some(muxer))?)
}

/// The network or file end of a stream output with SCTE-35. The muxer
/// writes here; the transport stream gains its SCTE-35 and goes on through
/// an AVIO context NoAr opened.
pub(crate) struct Scte35Sink {
    io: *mut ffi::AVIOContext,
    injector: TsInjector,
}

// SAFETY: the AVIO context is owned by the sink alone and only used by the
// thread that drives the muxer, through `&mut self`.
unsafe impl Send for Scte35Sink {}

impl Scte35Sink {
    pub(crate) fn open(
        url: &str,
        options: Dictionary<'_>,
        control: SpliceControl,
        pid: u16,
    ) -> Result<(Self, Dictionary<'static>)> {
        let url = CString::new(url)?;
        let mut io = ptr::null_mut();
        let (result, remaining) = unsafe {
            let mut dictionary = options.disown();
            let result = ffi::avio_open2(
                &mut io,
                url.as_ptr(),
                ffi::AVIO_FLAG_WRITE,
                ptr::null(),
                &mut dictionary,
            );

            (result, Dictionary::own(dictionary))
        };

        if result < 0 {
            return Err(ffmpeg::Error::from(result).into());
        }

        Ok((
            Self {
                io,
                injector: TsInjector::new(control, pid),
            },
            remaining,
        ))
    }

    fn check(&self) -> io::Result<()> {
        let error = unsafe { (*self.io).error };

        if error < 0 {
            Err(io::Error::other(ffmpeg::Error::from(error)))
        } else {
            Ok(())
        }
    }
}

impl io::Write for Scte35Sink {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let out = self.injector.process(data);

        if !out.is_empty() {
            let size = i32::try_from(out.len()).map_err(io::Error::other)?;
            unsafe { ffi::avio_write(self.io, out.as_ptr(), size) };
            self.check()?;
        }

        // The muxer flushes after each packet; so does the network.
        unsafe { ffi::avio_flush(self.io) };
        self.check()?;

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        unsafe { ffi::avio_flush(self.io) };
        self.check()
    }
}

impl Drop for Scte35Sink {
    fn drop(&mut self) {
        unsafe { ffi::avio_closep(&mut self.io) };
        self.injector.control.close_output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a splice_info_section back, for the fields NoAr writes.
    #[derive(Debug, PartialEq, Eq)]
    struct Parsed {
        event_id: u32,
        cancel: bool,
        out_of_network: bool,
        splice_time: Option<i64>,
        duration: Option<i64>,
        auto_return: bool,
    }

    fn parse(section: &[u8]) -> Parsed {
        assert_eq!(section[0], 0xFC);
        let length = 3 + ((usize::from(section[1] & 0x0F) << 8) | usize::from(section[2]));
        assert_eq!(length, section.len());
        assert_eq!(crc32_mpeg2(section), 0, "CRC over the whole section is 0");
        assert_eq!(&section[4..9], &[0, 0, 0, 0, 0], "no pts_adjustment");
        assert_eq!(section[10], 0xFF, "tier");
        assert_eq!(section[13], 0x05, "splice_insert");
        let command = &section[14..];
        let event_id = u32::from_be_bytes(command[..4].try_into().unwrap());
        let cancel = command[4] & 0x80 != 0;

        if cancel {
            return Parsed {
                event_id,
                cancel,
                out_of_network: false,
                splice_time: None,
                duration: None,
                auto_return: false,
            };
        }

        let flags = command[5];
        assert!(flags & 0x40 != 0, "program splice");
        assert!(flags & 0x10 == 0, "not immediate");
        let time = |bytes: &[u8]| {
            (i64::from(bytes[0] & 0x01) << 32)
                | (i64::from(bytes[1]) << 24)
                | (i64::from(bytes[2]) << 16)
                | (i64::from(bytes[3]) << 8)
                | i64::from(bytes[4])
        };
        assert!(command[6] & 0x80 != 0, "time specified");
        let splice_time = time(&command[6..11]);
        let (duration, auto_return) = if flags & 0x20 != 0 {
            (Some(time(&command[11..16])), command[11] & 0x80 != 0)
        } else {
            (None, false)
        };

        Parsed {
            event_id,
            cancel,
            out_of_network: flags & 0x80 != 0,
            splice_time: Some(splice_time),
            duration,
            auto_return,
        }
    }

    #[test]
    fn crc_matches_the_mpeg2_check_value() {
        assert_eq!(crc32_mpeg2(b"123456789"), 0x0376_E6E7);
    }

    #[test]
    fn a_break_out_carries_its_time_duration_and_auto_return() {
        let section = splice_insert(&SpliceInsert {
            event_id: 0x1234_5678,
            out_of_network: true,
            splice_time: (1 << 32) + 90_000,
            duration: Some(30 * 90_000),
        });

        assert_eq!(
            parse(&section),
            Parsed {
                event_id: 0x1234_5678,
                cancel: false,
                out_of_network: true,
                splice_time: Some((1 << 32) + 90_000),
                duration: Some(30 * 90_000),
                auto_return: true,
            }
        );
    }

    #[test]
    fn a_return_and_a_cancel_read_back() {
        let back = parse(&splice_insert(&SpliceInsert {
            event_id: 7,
            out_of_network: false,
            splice_time: 1_000,
            duration: None,
        }));
        assert!(!back.out_of_network && back.duration.is_none());
        assert_eq!(back.splice_time, Some(1_000));

        let cancel = parse(&splice_insert_cancel(7));
        assert!(cancel.cancel);
        assert_eq!(cancel.event_id, 7);
    }

    #[test]
    fn timestamps_compare_across_the_33_bit_wrap() {
        assert_eq!(pts_diff(10, PTS_MODULO - 10), 20);
        assert_eq!(pts_diff(PTS_MODULO - 10, 10), -20);
        assert_eq!(pts_diff(500, 200), 300);
    }

    fn psi_packet(pid: u16, section: &[u8], continuity: u8) -> [u8; TS_PACKET] {
        let mut packet = [0xFF_u8; TS_PACKET];
        packet[0] = 0x47;
        packet[1] = 0x40 | (pid >> 8) as u8;
        packet[2] = pid as u8;
        packet[3] = 0x10 | continuity;
        packet[4] = 0;
        packet[5..5 + section.len()].copy_from_slice(section);
        packet
    }

    fn with_crc(mut section: Vec<u8>) -> Vec<u8> {
        let length = section.len() + 4 - 3;
        section[1] = (section[1] & 0xF0) | (length >> 8) as u8;
        section[2] = length as u8;
        let crc = crc32_mpeg2(&section);
        section.extend_from_slice(&crc.to_be_bytes());
        section
    }

    /// A PAT and PMT as FFmpeg writes them: video on 0x100, audio on 0x101.
    fn pat_and_pmt() -> ([u8; TS_PACKET], [u8; TS_PACKET]) {
        let pat = with_crc(vec![
            0x00, 0xB0, 0, 0x00, 0x01, 0xC1, 0x00, 0x00, 0x00, 0x01, 0xF0, 0x00,
        ]);
        let pmt = with_crc(vec![
            0x02, 0xB0, 0, 0x00, 0x01, 0xC1, 0x00, 0x00, 0xE1, 0x00, 0xF0, 0x00, 0x1B, 0xE1, 0x00,
            0xF0, 0x00, 0x0F, 0xE1, 0x01, 0xF0, 0x00,
        ]);

        (psi_packet(0, &pat, 0), psi_packet(0x1000, &pmt, 0))
    }

    fn video_packet(pts: i64, key_frame: bool) -> [u8; TS_PACKET] {
        let mut packet = [0xFF_u8; TS_PACKET];
        packet[0] = 0x47;
        packet[1] = 0x41;
        packet[2] = 0x00;
        packet[3] = 0x30;
        packet[4] = 1;
        packet[5] = if key_frame { 0x40 } else { 0x00 };
        let pes = &mut packet[6..];
        pes[..9].copy_from_slice(&[0, 0, 1, 0xE0, 0, 0, 0x80, 0x80, 5]);
        pes[9] = 0x21 | (((pts >> 30) as u8 & 0x07) << 1);
        pes[10] = (pts >> 22) as u8;
        pes[11] = (((pts >> 15) as u8) << 1) | 1;
        pes[12] = (pts >> 7) as u8;
        pes[13] = ((pts as u8) << 1) | 1;
        packet
    }

    fn sections_on(stream: &[u8], pid: u16) -> Vec<Vec<u8>> {
        stream
            .chunks_exact(TS_PACKET)
            .filter(|packet| ((u16::from(packet[1] & 0x1F) << 8) | u16::from(packet[2])) == pid)
            .map(|packet| {
                let packet: &[u8; TS_PACKET] = packet.try_into().unwrap();
                psi_section(packet).unwrap().1.to_vec()
            })
            .collect()
    }

    #[test]
    fn the_pmt_lists_the_scte35_stream_with_its_descriptors() {
        let (_, pmt) = pat_and_pmt();
        let (rewritten, video_pid) = add_scte35_to_pmt(&pmt, 500).unwrap();
        let (_, section) = psi_section(&rewritten).unwrap();

        assert_eq!(video_pid, Some(0x100));
        assert_eq!(crc32_mpeg2(section), 0);
        assert_eq!(
            &section[10..18],
            &[0xF0, 0x06, 0x05, 0x04, b'C', b'U', b'E', b'I']
        );
        let streams = &section[18..section.len() - 4];
        assert_eq!(
            &streams[streams.len() - 8..],
            &[0x86, 0xE1, 0xF4, 0xF0, 0x03, 0x8A, 0x01, 0x00]
        );
        assert!(add_scte35_to_pmt(&pmt, 0x101).is_err(), "PID in use");
    }

    fn active_control(preroll_seconds: f64) -> SpliceControl {
        let control = SpliceControl::default();
        control.open_output(
            Scte35Config {
                pid: 500,
                preroll_seconds,
            },
            25,
            (1, 25),
        );
        control
    }

    #[test]
    fn a_break_is_announced_ahead_repeated_and_returned_from() {
        let control = active_control(2.0);
        let (pat, pmt) = pat_and_pmt();
        let mut injector = TsInjector::new(control.clone(), 500);
        let mut stream = Vec::new();

        stream.extend(injector.process(&pat));
        stream.extend(injector.process(&pmt));

        // A two second clip, then a 1.2 s break.
        assert!(!control.frame_needs_key_frame(0));
        control.plan_next_clip(ClipBreaks {
            at_start: None,
            after: Some(1.2),
        });
        let following = control.clip_started(0, Some(2.0));
        assert!(following.is_some());

        for frame in 0..100_i64 {
            let key_frame = control.frame_needs_key_frame(frame);
            assert_eq!(key_frame, frame == 50 || frame == 80, "frame {frame}");
            // Split writes across packet boundaries, as AVIO may.
            let packet = video_packet(frame * 3_600, key_frame);
            stream.extend(injector.process(&packet[..100]));
            stream.extend(injector.process(&packet[100..]));
        }

        let sections = sections_on(&stream, 500);
        let parsed: Vec<Parsed> = sections.iter().map(|section| parse(section)).collect();
        let outs: Vec<&Parsed> = parsed.iter().filter(|p| p.out_of_network).collect();
        let ins: Vec<&Parsed> = parsed.iter().filter(|p| !p.out_of_network).collect();

        assert_eq!(
            outs.len(),
            3,
            "copies at 2 s, 1 s and 0 s ahead: {parsed:?}"
        );
        assert_eq!(ins.len(), 3);
        assert!(outs.iter().all(|p| p.splice_time == Some(50 * 3_600)));
        assert!(
            outs.iter()
                .all(|p| p.duration == Some(30 * 3_600) && p.auto_return)
        );
        assert!(ins.iter().all(|p| p.splice_time == Some(80 * 3_600)));
        assert!(parsed.iter().all(|p| p.event_id == following.unwrap()));

        // The first copy goes out before the frame two seconds ahead.
        let first = stream
            .chunks_exact(TS_PACKET)
            .position(|packet| packet[2] == (500 & 0xFF) as u8 && packet[1] & 0x1F == 1)
            .unwrap();
        let next_video = &stream[(first + 1) * TS_PACKET..(first + 2) * TS_PACKET];
        assert_eq!(
            pes_pts(next_video.try_into().unwrap()).unwrap().0,
            0,
            "announced 2 s before frame 50"
        );

        // Continuity counts on.
        let counters: Vec<u8> = stream
            .chunks_exact(TS_PACKET)
            .filter(|packet| packet[2] == 0xF4 && packet[1] & 0x1F == 1)
            .map(|packet| packet[3] & 0x0F)
            .collect();
        assert_eq!(counters, vec![0, 1, 2, 3, 4, 5]);

        let status = control.status();
        assert!(status.breaks.is_empty());
        assert_eq!(status.recent[0].state, BreakState::Ended);
        assert_eq!(status.recent[0].out_key_frame, Some(true));
        assert_eq!(status.recent[0].in_key_frame, Some(true));
    }

    #[test]
    fn a_break_after_a_cut_clip_is_called_off() {
        let control = active_control(2.0);
        let (pat, pmt) = pat_and_pmt();
        let mut injector = TsInjector::new(control.clone(), 500);
        let mut stream = Vec::new();

        stream.extend(injector.process(&pat));
        stream.extend(injector.process(&pmt));
        control.plan_next_clip(ClipBreaks {
            at_start: None,
            after: Some(10.0),
        });
        let following = control.clip_started(0, Some(2.0));

        for frame in 0..10_i64 {
            control.frame_needs_key_frame(frame);
            stream.extend(injector.process(&video_packet(frame * 3_600, false)));
        }

        // Skipped at frame 10, before the break.
        control.clip_finished(following, false);
        assert!(!control.frame_needs_key_frame(50));

        for frame in 10..20_i64 {
            control.frame_needs_key_frame(frame);
            stream.extend(injector.process(&video_packet(frame * 3_600, false)));
        }

        let parsed: Vec<Parsed> = sections_on(&stream, 500)
            .iter()
            .map(|section| parse(section))
            .collect();
        assert_eq!(
            parsed.len(),
            2,
            "announced once, then cancelled: {parsed:?}"
        );
        assert!(parsed[1].cancel);
        assert_eq!(control.status().recent[0].state, BreakState::Cancelled);
    }

    #[test]
    fn a_manual_break_starts_after_the_preroll_and_cannot_overlap() {
        let control = active_control(4.0);
        control.frame_needs_key_frame(99);

        let first = control.insert_break(30.0).unwrap();
        assert_eq!(first.state, BreakState::Scheduled);
        assert!(matches!(
            control.insert_break(30.0),
            Err(SpliceError::Overlaps(id)) if id == first.event_id
        ));

        control.cancel_break(first.event_id).unwrap();
        assert!(control.status().breaks.is_empty());

        // 4 s of pre-roll after frame 99, then 30 s.
        let second = control.insert_break(30.0).unwrap();
        assert!(control.frame_needs_key_frame(200));
        assert!(control.frame_needs_key_frame(200 + 750));
        assert!(matches!(
            control.cancel_break(second.event_id),
            Err(SpliceError::Started(_))
        ));
        assert!(matches!(
            SpliceControl::default().insert_break(30.0),
            Err(SpliceError::Inactive)
        ));
    }
}
