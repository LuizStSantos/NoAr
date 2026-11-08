//! Runs scheduled events (see `utils::schedule`) over the running programme.
//!
//! A ticker checks every half second for due occurrences, claims each one in
//! the database so it plays once, and queues it. Events that interrupt cut the
//! clip on air through the engine's navigation request, the same way the
//! "next clip" control does, so the output keeps running. The play loop picks
//! the queued event up between clips, plays it, then moves the playlist's
//! time shift so the programme continues where it stopped or on the clock.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Mutex, MutexGuard, PoisonError, atomic::Ordering},
    time::{Duration, Instant},
};

use chrono::{NaiveDateTime, TimeDelta, TimeZone};
use chrono_tz::Tz;
use log::*;
use noar_engine::{AsyncPlayout, NavigationBlocked};
use serde::Serialize;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

mod playback;

use crate::{
    db::{
        handles,
        models::{AfterMode, EntryMode, ScheduleEvent, ScheduleKind, UnfinishedRun},
    },
    player::{as_run, controller::ChannelManager},
    utils::{
        config::PlayoutConfig,
        errors::ServiceError,
        schedule::{
            Occurrence,
            live::{live_protocol, read_live_url},
            occurrences_between, seconds,
        },
        time_machine::time_now,
    },
};

#[cfg(test)]
use playback::waiting_source;
use playback::{
    EventEnd, finish_run, forget_live, live_answers, play_clips, play_live, preload_event,
    preload_return, stopping,
};

/// How often the ticker looks for due events.
const TICK: Duration = Duration::from_millis(500);

/// A clip event whose start passed longer ago than this does not start any
/// more, for example after a restart.
const MEDIA_START_GRACE_SECONDS: i64 = 60;

/// The longest stretch of local time a clock change skips (summer time
/// starts): events placed in it are looked for this far back.
const MAX_CLOCK_GAP: Duration = Duration::from_secs(3 * 3600);

/// Pause between attempts while a live source does not answer.
const LIVE_RETRY_SECONDS: f64 = 5.0;

/// Events open what they start with this long before their start.
const PRELOAD_LEAD_SECONDS: f64 = 10.0;

/// An occurrence with less time left than this is not resumed after a stop.
const RESUME_MIN_SECONDS: f64 = 5.0;

/// How the moment an occurrence went on air is stored.
const ON_AIR_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";

/// The clip that was cut by an interrupting event.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InterruptedClip {
    pub source: String,
    pub title: Option<String>,
    /// Seconds into the clip where it was cut.
    pub position: f64,
    /// Title of the program the clip belongs to, from its `.nfo`; filled in
    /// when the status is reported.
    pub program: Option<String>,
}

/// The event on air, as reported to the web interface.
#[derive(Debug, Clone, Serialize)]
pub struct ActiveEvent {
    pub event_id: i32,
    pub title: String,
    pub kind: ScheduleKind,
    pub entry_mode: EntryMode,
    pub after_mode: AfterMode,
    /// Planned length in seconds.
    pub planned_seconds: f64,
    pub elapsed_seconds: f64,
    /// What is playing inside the event: a clip name, or the live protocol.
    pub item: Option<String>,
    pub interrupted: Option<InterruptedClip>,
    /// A live's end time, channel time (`YYYY-MM-DDTHH:MM:SS`), extensions
    /// included.
    pub ends_at: Option<String>,
    /// Seconds a live was extended by while on air (this airing only).
    pub extended_seconds: f64,
    #[serde(skip)]
    started: Instant,
    /// A live's planned end before any extension.
    #[serde(skip)]
    planned_end: Option<NaiveDateTime>,
}

/// Where an occurrence that was on air when the playout stopped picks up.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Resume {
    /// When it first went on air, channel time.
    on_air_at: NaiveDateTime,
    /// The programme's time shift before it first went on air.
    shift_before: Option<f64>,
}

#[derive(Debug)]
struct Queued {
    occurrence: Occurrence,
    /// When the live source last answered a signal check.
    signal_checked: Option<Instant>,
    resume: Option<Resume>,
    /// Started by hand or put back after a stop, not by the event's time.
    forced: bool,
}

#[derive(Debug, Default)]
struct Inner {
    /// Enabled events of the channel; `None` reloads them from the database.
    events: Option<Vec<ScheduleEvent>>,
    queue: VecDeque<Queued>,
    active: Option<ActiveEvent>,
    /// How much later the live on air ends than planned (this airing only).
    extension: TimeDelta,
    abort: bool,
    interrupt_wanted: bool,
    /// Counts player starts, so work left over from an earlier start (a live
    /// waiting for its signal) can tell it is outdated.
    generation: u64,
}

/// Scheduler state shared by the ticker, the play loop and the API.
#[derive(Debug, Default)]
pub struct ScheduleState {
    inner: Mutex<Inner>,
}

impl ScheduleState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Reload the events on the next tick, after they were changed.
    pub fn invalidate(&self) {
        self.lock().events = None;
    }

    /// Forget queued and active events, when the player (re)starts.
    pub fn reset(&self) {
        let mut inner = self.lock();

        inner.queue.clear();
        inner.active = None;
        inner.extension = TimeDelta::zero();
        inner.abort = false;
        inner.interrupt_wanted = false;
        inner.generation += 1;
    }

    fn generation(&self) -> u64 {
        self.lock().generation
    }

    fn queue(&self, occurrence: Occurrence, signal_checked: Option<Instant>) {
        self.push(occurrence, signal_checked, None, false);
    }

    /// Queue an occurrence that was on air when the playout stopped; it goes
    /// back on air right away.
    fn queue_resumed(&self, mut occurrence: Occurrence, resume: Resume) {
        occurrence.event.entry_mode = EntryMode::Interrupt;
        self.push(occurrence, None, Some(resume), true);
    }

    fn push(
        &self,
        occurrence: Occurrence,
        signal_checked: Option<Instant>,
        resume: Option<Resume>,
        forced: bool,
    ) {
        let mut inner = self.lock();

        if occurrence.event.entry_mode == EntryMode::Interrupt {
            inner.interrupt_wanted = true;
        }

        inner.queue.push_back(Queued {
            occurrence,
            signal_checked,
            resume,
            forced,
        });
    }

    /// Queue an event to start right away, cutting what is on air.
    pub fn trigger(&self, mut occurrence: Occurrence) {
        occurrence.event.entry_mode = EntryMode::Interrupt;
        self.push(occurrence, None, None, true);
    }

    /// Bring what waits in the queue in line with an event that was changed,
    /// or removed (`None`): what it no longer schedules leaves the queue, the
    /// rest airs as it is now. Returns the occurrences that left.
    pub fn revise(&self, event_id: i32, event: Option<&ScheduleEvent>) -> Vec<Occurrence> {
        let mut inner = self.lock();
        let mut dropped = vec![];

        for mut queued in std::mem::take(&mut inner.queue) {
            if queued.occurrence.event.id != event_id {
                inner.queue.push_back(queued);
                continue;
            }

            match event.and_then(|event| revised(&queued.occurrence, event, queued.forced)) {
                Some(occurrence) => {
                    queued.occurrence = occurrence;
                    inner.queue.push_back(queued);
                }
                None => dropped.push(queued.occurrence),
            }
        }

        let interrupts = inner
            .queue
            .iter()
            .any(|queued| queued.occurrence.event.entry_mode == EntryMode::Interrupt);

        inner.interrupt_wanted &= interrupts;

        dropped
    }

    pub fn has_queued(&self) -> bool {
        !self.lock().queue.is_empty()
    }

    pub fn queued(&self) -> Vec<Occurrence> {
        self.lock()
            .queue
            .iter()
            .map(|queued| queued.occurrence.clone())
            .collect()
    }

    fn take_queued(&self) -> Option<Queued> {
        let mut inner = self.lock();
        let queued = inner.queue.pop_front();

        if inner.queue.is_empty() {
            inner.interrupt_wanted = false;
        }

        queued
    }

    pub fn active(&self) -> Option<ActiveEvent> {
        self.lock().active.clone().map(|mut active| {
            active.elapsed_seconds = active.started.elapsed().as_secs_f64();
            active
        })
    }

    /// End the live on air at `new_end` (channel time) instead of when it was
    /// going to, for this airing only; the saved event keeps its times.
    /// Returns the live as it is now, or why it cannot change: no live on
    /// air, or an end that is not later than now.
    pub fn extend_live(
        &self,
        new_end: NaiveDateTime,
        now: NaiveDateTime,
    ) -> Result<ActiveEvent, String> {
        let mut inner = self.lock();
        let Some(active) = inner.active.as_mut() else {
            return Err("no live is on air".to_string());
        };
        let Some(planned_end) = active
            .planned_end
            .filter(|_| active.kind == ScheduleKind::Live)
        else {
            return Err("the event on air is not a live".to_string());
        };
        if new_end <= now + seconds(5.0) {
            return Err(
                "the new end must be later than now; to end the live now, go back to the programme"
                    .to_string(),
            );
        }
        if new_end - planned_end > TimeDelta::try_hours(24).unwrap_or_default() {
            return Err("a live can be extended by 24 hours at most".to_string());
        }

        let extension = new_end - planned_end;
        let change = extension - inner.extension;
        let active = inner.active.as_mut().expect("checked above");
        active.planned_seconds += change.num_milliseconds() as f64 / 1000.0;
        active.extended_seconds = extension.num_milliseconds() as f64 / 1000.0;
        active.ends_at = Some(new_end.format("%Y-%m-%dT%H:%M:%S").to_string());
        let mut current = active.clone();
        inner.extension = extension;
        current.elapsed_seconds = current.started.elapsed().as_secs_f64();

        Ok(current)
    }

    /// When the live on air now ends compared with its planned end.
    fn live_extension(&self) -> TimeDelta {
        self.lock().extension
    }

    /// The live on air's end, extensions included.
    pub fn live_end(&self) -> Option<NaiveDateTime> {
        let inner = self.lock();
        let active = inner.active.as_ref()?;

        active.planned_end.map(|end| end + inner.extension)
    }

    pub fn is_active(&self) -> bool {
        self.lock().active.is_some()
    }

    /// Close the event that was on air. A stop asked while it was closing
    /// (a second "end", say) was meant for it, not for what comes next.
    fn close_active(&self) {
        let mut inner = self.lock();

        inner.active = None;
        inner.extension = TimeDelta::zero();
        inner.abort = false;
    }

    /// Ask the event on air to stop; returns `false` when none is on air.
    pub fn request_abort(&self) -> bool {
        let mut inner = self.lock();

        inner.abort = inner.active.is_some();
        inner.abort
    }

    fn take_abort(&self) -> bool {
        std::mem::take(&mut self.lock().abort)
    }

    fn set_item(&self, item: String) {
        if let Some(active) = self.lock().active.as_mut() {
            active.item = Some(item);
        }
    }

    fn cached_events(&self) -> Option<Vec<ScheduleEvent>> {
        self.lock().events.clone()
    }

    fn store_events(&self, events: Vec<ScheduleEvent>) {
        self.lock().events = Some(events);
    }

    fn wants_interrupt(&self) -> bool {
        let inner = self.lock();

        inner.interrupt_wanted && inner.active.is_none() && !inner.queue.is_empty()
    }

    fn interrupt_sent(&self) {
        self.lock().interrupt_wanted = false;
    }
}

/// Check for due events until `cancel` fires. Runs beside the play loop.
pub async fn run_ticker(manager: ChannelManager, cancel: CancellationToken) {
    // Occurrences already looked at, by event and start: an event moved to a
    // new time is a new occurrence and is looked at again.
    let mut seen: HashSet<(i32, NaiveDateTime)> = HashSet::new();
    // Occurrences whose first clip or live is already opening.
    let mut preloaded: HashSet<(i32, NaiveDateTime)> = HashSet::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(TICK) => {}
        }

        if let Err(error) = tick(&manager, &mut seen, &mut preloaded).await {
            warn!(channel = manager.id; "Scheduler: {error}");
        }
    }
}

async fn tick(
    manager: &ChannelManager,
    seen: &mut HashSet<(i32, NaiveDateTime)>,
    preloaded: &mut HashSet<(i32, NaiveDateTime)>,
) -> Result<(), ServiceError> {
    let events = match manager.schedule.cached_events() {
        Some(events) => events,
        None => {
            let events = handles::select_schedule_events(&manager.db_pool, manager.id).await?;
            manager.schedule.store_events(events.clone());
            events
        }
    };
    let timezone = manager.config.read().await.channel.timezone;
    let now_at = time_now(&timezone);
    let zone = now_at.timezone();
    let now = now_at.naive_local();
    let grace = TimeDelta::try_seconds(MEDIA_START_GRACE_SECONDS).unwrap_or_default();
    let due = occurrences_between(
        &events,
        now - grace - seconds(MAX_CLOCK_GAP.as_secs_f64()),
        now + seconds(1.0),
    );
    let upcoming = occurrences_between(&events, now, now + seconds(PRELOAD_LEAD_SECONDS));

    for occurrence in upcoming.into_iter().filter(|o| o.start > now) {
        if preloaded.insert((occurrence.event.id, occurrence.start)) {
            preload_event(manager, &occurrence).await;
        }
    }

    for occurrence in due.into_iter().filter(|o| is_due(o, now, zone, grace)) {
        let key = (occurrence.event.id, occurrence.start);

        if !seen.insert(key) || !starts_now(&occurrence, now, zone, grace) {
            continue;
        }

        let claimed = match handles::claim_schedule_run(
            &manager.db_pool,
            occurrence.event.id,
            &occurrence.date_key(),
            "running",
        )
        .await
        {
            Ok(claimed) => claimed,
            Err(error) => {
                // Look at it again on the next tick: a passing database error
                // must not lose the occurrence, and the others still go ahead.
                seen.remove(&key);
                warn!(channel = manager.id;
                    "Scheduler could not claim <span class=\"log-addr\">{}</span>: {error}",
                    occurrence.event.title
                );
                continue;
            }
        };

        if !claimed {
            continue;
        }

        info!(channel = manager.id;
            "Scheduled event <span class=\"log-addr\">{}</span> is due",
            occurrence.event.title
        );

        match occurrence.event.kind {
            ScheduleKind::Media => manager.schedule.queue(occurrence, None),
            ScheduleKind::Live => {
                tokio::spawn(queue_when_signal_answers(manager.clone(), occurrence));
            }
        }
    }

    seen.retain(|(_, start)| {
        start.date() >= now.date() - TimeDelta::try_days(2).unwrap_or_default()
    });
    preloaded.retain(|(_, start)| *start > now - TimeDelta::try_hours(1).unwrap_or_default());

    if manager.schedule.wants_interrupt() && request_interrupt(manager).await {
        manager.schedule.interrupt_sent();
    }

    Ok(())
}

/// When a local time comes on the wall clock. A time the clock skips, in the
/// gap when summer time starts, comes when the gap ends; any other time is
/// itself.
fn wall_clock_time(time: NaiveDateTime, zone: Tz) -> NaiveDateTime {
    let mut candidate = time;
    let step = TimeDelta::try_minutes(1).unwrap_or_default();

    for _ in 0..=MAX_CLOCK_GAP.as_secs() / 60 {
        if zone.from_local_datetime(&candidate).earliest().is_some() {
            return candidate;
        }
        candidate += step;
    }

    time
}

/// Whether an occurrence's start has come and is recent enough to look at.
fn is_due(occurrence: &Occurrence, now: NaiveDateTime, zone: Tz, grace: TimeDelta) -> bool {
    let start = wall_clock_time(occurrence.start, zone);

    match occurrence.event.kind {
        ScheduleKind::Media => start <= now && now - start <= grace,
        // A live still running is due however long ago it started.
        ScheduleKind::Live => start <= now && occurrence.end > now - grace,
    }
}

/// Whether a due occurrence still starts: a clip within its grace after its
/// start, a live while it has more than a few seconds left.
fn starts_now(occurrence: &Occurrence, now: NaiveDateTime, zone: Tz, grace: TimeDelta) -> bool {
    match occurrence.event.kind {
        ScheduleKind::Media => now - wall_clock_time(occurrence.start, zone) <= grace,
        ScheduleKind::Live => occurrence.end - now > seconds(5.0),
    }
}

/// What a new start of the playout does with an occurrence the stop cut.
#[derive(Debug, PartialEq)]
enum ResumePlan {
    /// It was on air and has time left: back on air where it was.
    OnAir(Resume),
    /// It waited for its start (a clip to end, a live signal): wait again.
    Wait,
    /// Its time is over.
    Over { aired: bool },
}

fn resume_plan(occurrence: &Occurrence, run: &UnfinishedRun, now: NaiveDateTime) -> ResumePlan {
    let on_air_at = run
        .on_air_at
        .as_deref()
        .and_then(|value| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f").ok());
    let end = match (occurrence.event.kind, on_air_at) {
        (ScheduleKind::Media, Some(on_air_at)) => on_air_at + seconds(occurrence.event.duration),
        _ => occurrence.end,
    };

    if seconds_between(now, end) < RESUME_MIN_SECONDS {
        return ResumePlan::Over {
            aired: on_air_at.is_some(),
        };
    }

    match on_air_at {
        Some(on_air_at) => ResumePlan::OnAir(Resume {
            on_air_at,
            shift_before: run.shift_before,
        }),
        None => ResumePlan::Wait,
    }
}

/// After the playout (re)starts, pick up the occurrences its stop cut: one
/// still inside its time goes back on air where it was (a live keeps its end
/// time), one that waited for its start waits again, and the others are
/// closed. Without this a restart during an event fell back to the playlist.
pub(crate) async fn resume_unfinished(manager: &ChannelManager) {
    if let Err(error) = try_resume_unfinished(manager).await {
        warn!(channel = manager.id; "Scheduler: cannot resume cut events: {error}");
    }
}

async fn try_resume_unfinished(manager: &ChannelManager) -> Result<(), ServiceError> {
    let timezone = manager.config.read().await.channel.timezone;
    let now = time_now(&timezone).naive_local();
    let day = TimeDelta::try_days(1).unwrap_or_default();
    let since = (now - day).date().format("%Y-%m-%d").to_string();
    let runs = handles::select_unfinished_runs(&manager.db_pool, manager.id, &since).await?;

    if runs.is_empty() {
        return Ok(());
    }

    let events = handles::select_schedule_events(&manager.db_pool, manager.id).await?;
    let occurrences = occurrences_between(&events, now - day - day, now + seconds(1.0));

    for run in runs {
        let Some(occurrence) = occurrences
            .iter()
            .find(|occurrence| {
                occurrence.event.id == run.event_id && occurrence.date_key() == run.occurrence
            })
            .cloned()
        else {
            // Removed, disabled or moved since it was cut.
            close_run(manager, run.event_id, &run.occurrence, "skipped").await;
            continue;
        };

        match resume_plan(&occurrence, &run, now) {
            ResumePlan::OnAir(resume) => {
                info!(channel = manager.id;
                    "Scheduled event <span class=\"log-addr\">{}</span> was cut by the stop; it goes back on air",
                    occurrence.event.title
                );
                manager.schedule.queue_resumed(occurrence, resume);
            }
            ResumePlan::Wait => match occurrence.event.kind {
                ScheduleKind::Media => manager.schedule.queue(occurrence, None),
                ScheduleKind::Live => {
                    tokio::spawn(queue_when_signal_answers(manager.clone(), occurrence));
                }
            },
            ResumePlan::Over { aired } => {
                let status = if aired { "done" } else { "skipped" };

                close_run(manager, run.event_id, &run.occurrence, status).await;
            }
        }
    }

    Ok(())
}

async fn close_run(manager: &ChannelManager, event_id: i32, occurrence: &str, status: &str) {
    if let Err(error) =
        handles::finish_schedule_run(&manager.db_pool, event_id, occurrence, status).await
    {
        error!(channel = manager.id; "Unable to save the scheduled event state: {error}");
    }
}

/// `occurrence` as `event` schedules it now, or `None` when it no longer
/// does: disabled, or moved to another day or time. One started by hand or
/// put back after a stop keeps its time and how it enters.
fn revised(occurrence: &Occurrence, event: &ScheduleEvent, forced: bool) -> Option<Occurrence> {
    if !event.enabled {
        return None;
    }

    if forced {
        let mut occurrence = occurrence.clone();
        let entry_mode = occurrence.event.entry_mode;

        occurrence.event = event.clone();
        occurrence.event.entry_mode = entry_mode;

        return Some(occurrence);
    }

    event
        .occurrence_on(occurrence.date)
        .filter(|current| current.start == occurrence.start)
}

/// The occurrence as its event schedules it now, read from the database;
/// `None` once the event was removed, disabled or moved.
async fn still_scheduled(manager: &ChannelManager, occurrence: &Occurrence) -> Option<Occurrence> {
    match handles::select_schedule_event(&manager.db_pool, manager.id, occurrence.event.id).await {
        Ok(event) => event.and_then(|event| revised(occurrence, &event, false)),
        // The database not answering is no reason to drop it.
        Err(error) => {
            error!(channel = manager.id; "Unable to read the scheduled event: {error}");
            Some(occurrence.clone())
        }
    }
}

fn seconds_between(from: NaiveDateTime, to: NaiveDateTime) -> f64 {
    (to - from).num_milliseconds() as f64 / 1000.0
}

/// Wait until the live source answers before taking over, so a missing
/// signal never cuts the programme. Gives up after the event's retry time.
async fn queue_when_signal_answers(manager: ChannelManager, occurrence: Occurrence) {
    let id = manager.id;
    let generation = manager.schedule.generation();
    let event = occurrence.event.clone();
    let Some(path) = event.sources.first() else {
        return;
    };
    let path = manager.config.read().await.channel.storage.join(path);
    let first_try = Instant::now();
    let retry = Duration::from_secs(u64::from(event.retry_seconds));

    let url = match read_live_url(&path).await {
        Ok(url) => url,
        Err(error) => {
            error!(channel = id; "Live <span class=\"log-addr\">{}</span>: {error}", event.title);
            finish_run(&manager, &occurrence, "failed").await;
            return;
        }
    };

    loop {
        // The playout stopped or restarted meanwhile. The occurrence stays
        // claimed, and the next start waits for its signal again.
        if manager.schedule.generation() != generation || stopping(&manager) {
            return;
        }

        // Removed, disabled or moved while it waited for its signal.
        let Some(current) = still_scheduled(&manager, &occurrence).await else {
            info!(channel = id;
                "Live <span class=\"log-addr\">{}</span> was removed or changed; it no longer waits for its signal",
                event.title
            );
            manager.preloader.lock().await.clone().forget(&url);
            return;
        };

        let timezone = manager.config.read().await.channel.timezone;
        let now = time_now(&timezone).naive_local();

        if occurrence.end - now <= seconds(5.0) {
            break;
        }

        if live_answers(&manager, &url).await {
            if manager.schedule.generation() == generation {
                manager.schedule.queue(current, Some(Instant::now()));
            }

            return;
        }

        if first_try.elapsed() >= retry {
            break;
        }

        warn!(channel = id;
            "Live <span class=\"log-addr\">{}</span>: no signal at {}, trying again",
            event.title,
            live_protocol(&url)
        );
        sleep(Duration::from_secs_f64(LIVE_RETRY_SECONDS)).await;
    }

    error!(channel = id;
        "Live <span class=\"log-addr\">{}</span> skipped: the source did not answer",
        event.title
    );
    manager.preloader.lock().await.clone().forget(&url);
    finish_run(&manager, &occurrence, "failed").await;
}

/// Cut the clip on air. `false` when another
/// navigation is in progress; the caller tries again on the next tick.
async fn request_interrupt(manager: &ChannelManager) -> bool {
    let control = manager.playback_control.lock().await;

    match control.begin_navigation() {
        Ok(navigation) => {
            navigation.commit();
            true
        }
        Err(NavigationBlocked::Busy) => false,
    }
}

/// Stop the event on air and continue the programme as the event says.
pub async fn return_to_programme(manager: &ChannelManager) -> bool {
    if !manager.schedule.request_abort() {
        return false;
    }

    manager.playback_control.lock().await.skip_current();

    true
}

/// Called by the play loop between clips: plays the next queued event, if
/// any. Returns `Some(stopped)` when an event ran; `stopped` means the
/// desktop window was closed during it.
pub(crate) async fn play_queued_event(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    playout: &AsyncPlayout,
    interrupted: Option<InterruptedClip>,
) -> Result<Option<bool>, ServiceError> {
    // A stopping playout airs nothing new; the queue starts over with it.
    if stopping(manager) {
        return Ok(None);
    }

    let Some(queued) = manager.schedule.take_queued() else {
        return Ok(None);
    };
    let occurrence = queued.occurrence;
    let event = occurrence.event.clone();
    let channel_id = config.general.channel_id;
    let now = time_now(&config.channel.timezone).naive_local();
    let resume = queued.resume;
    let current_shift = manager.channel.lock().await.time_shift;
    // A resumed occurrence counts from its first start, so the programme
    // afterwards continues as if the playout had never stopped.
    let shift_before = resume
        .and_then(|resume| resume.shift_before)
        .unwrap_or(current_shift);
    let on_air_at = resume.map_or(now, |resume| resume.on_air_at);
    let already_aired = seconds_between(on_air_at, now).max(0.0);
    let started = Instant::now()
        .checked_sub(Duration::from_secs_f64(already_aired))
        .unwrap_or_else(Instant::now);
    let planned_seconds = match event.kind {
        ScheduleKind::Media => event.duration,
        ScheduleKind::Live => seconds_between(on_air_at, occurrence.end),
    };

    if let Err(error) = handles::mark_schedule_run_on_air(
        &manager.db_pool,
        event.id,
        &occurrence.date_key(),
        &on_air_at.format(ON_AIR_FORMAT).to_string(),
        shift_before,
    )
    .await
    {
        error!(channel = channel_id; "Unable to save the scheduled event state: {error}");
    }

    let planned_end = (event.kind == ScheduleKind::Live).then_some(occurrence.end);
    {
        let mut schedule = manager.schedule.lock();
        schedule.active = Some(ActiveEvent {
            event_id: event.id,
            title: event.title.clone(),
            kind: event.kind,
            entry_mode: event.entry_mode,
            after_mode: event.after_mode,
            planned_seconds,
            elapsed_seconds: 0.0,
            item: None,
            interrupted,
            ends_at: planned_end.map(|end| end.format("%Y-%m-%dT%H:%M:%S").to_string()),
            extended_seconds: 0.0,
            started,
            planned_end,
        });
        schedule.extension = TimeDelta::zero();
    }
    manager.schedule.lock().abort = false;

    if resume.is_some() {
        info!(channel = channel_id;
            "Scheduled event <span class=\"log-addr\">{}</span> back on air after <span class=\"log-number\">{already_aired:.1}</span> s",
            event.title
        );
    } else {
        info!(channel = channel_id;
            "Scheduled event <span class=\"log-addr\">{}</span> on air",
            event.title
        );
    }

    let scheduled_at = as_run::local_millis(config, occurrence.start);
    let return_preload = preload_return(
        manager,
        config,
        event.after_mode,
        shift_before,
        started,
        planned_seconds,
    );
    let result = match event.kind {
        ScheduleKind::Media => {
            play_clips(
                manager,
                config,
                playout,
                &event,
                scheduled_at,
                already_aired,
            )
            .await
        }
        ScheduleKind::Live => {
            let result = play_live(
                manager,
                config,
                playout,
                &occurrence,
                scheduled_at,
                queued.signal_checked,
            )
            .await;

            forget_live(manager, &event).await;

            result
        }
    };

    return_preload.abort();
    // An end or skip that arrived while the event was closing was meant for
    // it; left pending it would cut the programme that comes back now.
    manager.playback_control.lock().await.cancel_skip();

    let elapsed = started.elapsed().as_secs_f64();
    let shift = match event.after_mode {
        AfterMode::Continue => shift_before - elapsed,
        AfterMode::Clock => shift_before,
    };
    let current_date = manager.current_date.lock().await.clone();

    if let Err(error) =
        handles::update_stat(&manager.db_pool, channel_id, &Some(current_date), shift).await
    {
        error!(channel = channel_id; "Unable to write status: {error}");
    }

    manager.channel.lock().await.time_shift = shift;
    manager.list_init.store(true, Ordering::SeqCst);
    manager.schedule.close_active();

    // The playout stopping, or the output failing and the playout restarting,
    // cut the event: it stays open, and the next start puts it back on air if
    // its time is not over.
    if stays_open(stopping(manager), &result) {
        info!(channel = channel_id;
            "Scheduled event <span class=\"log-addr\">{}</span> stopped with the playout",
            event.title
        );

        return result.map(|end| Some(end.stopped));
    }

    let status = match &result {
        Ok(EventEnd {
            completed: true, ..
        }) => "done",
        _ => "failed",
    };

    finish_run(manager, &occurrence, status).await;
    info!(channel = channel_id;
        "Scheduled event <span class=\"log-addr\">{}</span> ended after <span class=\"log-number\">{elapsed:.1}</span> s; programme {}",
        event.title,
        match event.after_mode {
            AfterMode::Continue => "continues where it stopped",
            AfterMode::Clock => "continues on the clock",
        }
    );

    result.map(|end| Some(end.stopped))
}

/// Whether an event cut short stays open for the next start to resume.
///
/// Only an output failure or a requested playout restart comes back as an
/// error here (a clip that cannot play falls back instead), and the playout
/// restarts after it: the event must not be closed as failed by a short
/// output loss.
fn stays_open<T>(stopping: bool, result: &Result<T, ServiceError>) -> bool {
    stopping || result.is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::schedule::parse_date;

    fn occurrence(id: i32, entry_mode: EntryMode) -> Occurrence {
        let event = ScheduleEvent {
            id,
            entry_mode,
            title: format!("Event {id}"),
            start_date: "2026-10-14".to_string(),
            start_time: "13:00:00".to_string(),
            duration: 30.0,
            enabled: true,
            ..ScheduleEvent::default()
        };

        event
            .occurrence_on(parse_date("2026-10-14").unwrap())
            .unwrap()
    }

    fn at(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    fn run(on_air_at: Option<&str>) -> UnfinishedRun {
        UnfinishedRun {
            event_id: 1,
            occurrence: "2026-10-14".to_string(),
            on_air_at: on_air_at.map(str::to_string),
            shift_before: on_air_at.map(|_| -12.5),
        }
    }

    fn on_air(kind: ScheduleKind, planned_end: Option<NaiveDateTime>) -> ActiveEvent {
        ActiveEvent {
            event_id: 7,
            title: "Jornal".to_string(),
            kind,
            entry_mode: EntryMode::Interrupt,
            after_mode: AfterMode::Clock,
            planned_seconds: 3600.0,
            elapsed_seconds: 0.0,
            item: None,
            interrupted: None,
            ends_at: None,
            extended_seconds: 0.0,
            started: Instant::now(),
            planned_end,
        }
    }

    #[test]
    fn a_live_on_air_is_extended_for_this_airing() {
        let at = |time: &str| {
            NaiveDateTime::parse_from_str(&format!("2026-11-02T{time}"), "%Y-%m-%dT%H:%M:%S")
                .unwrap()
        };
        let state = ScheduleState::default();
        let now = at("12:40:00");
        assert!(
            state.extend_live(at("13:30:00"), now).is_err(),
            "nothing on air"
        );

        state.lock().active = Some(on_air(ScheduleKind::Live, Some(at("13:00:00"))));
        let extended = state.extend_live(at("13:30:00"), now).unwrap();
        assert_eq!(extended.extended_seconds, 1800.0);
        assert_eq!(extended.planned_seconds, 5400.0);
        assert_eq!(extended.ends_at.as_deref(), Some("2026-11-02T13:30:00"));
        assert_eq!(state.live_end(), Some(at("13:30:00")));
        assert_eq!(state.live_extension(), TimeDelta::try_minutes(30).unwrap());

        // Changed again: from the planned end, not on top of the last change.
        let changed = state.extend_live(at("13:15:00"), now).unwrap();
        assert_eq!(changed.extended_seconds, 900.0);
        assert_eq!(changed.planned_seconds, 4500.0);

        assert!(
            state.extend_live(at("12:40:02"), now).is_err(),
            "an end that already passed"
        );

        state.lock().active = Some(on_air(ScheduleKind::Media, None));
        assert!(
            state.extend_live(at("13:30:00"), now).is_err(),
            "not a live"
        );
    }

    #[test]
    fn a_removed_event_leaves_the_queue() {
        let state = ScheduleState::default();

        state.queue(occurrence(1, EntryMode::AfterClip), None);
        state.queue(occurrence(2, EntryMode::Interrupt), None);

        assert_eq!(state.revise(2, None).len(), 1);
        assert_eq!(state.queued().len(), 1);
        assert_eq!(state.queued()[0].event.id, 1);
        // Nothing left in the queue cuts the clip on air.
        assert!(!state.wants_interrupt());
    }

    #[test]
    fn a_changed_event_airs_as_it_is_now_or_leaves_the_queue() {
        let state = ScheduleState::default();
        let waiting = occurrence(1, EntryMode::AfterClip);
        let mut renamed = waiting.event.clone();
        renamed.title = "Renamed".to_string();
        renamed.sources = vec!["Breaks/new.mp4".to_string()];

        state.queue(waiting.clone(), None);
        assert!(state.revise(1, Some(&renamed)).is_empty());
        assert_eq!(state.queued()[0].event.title, "Renamed");
        assert_eq!(state.queued()[0].event.sources, renamed.sources);

        let mut moved = renamed.clone();
        moved.start_time = "15:00:00".to_string();
        assert_eq!(state.revise(1, Some(&moved)).len(), 1);
        assert!(!state.has_queued());

        let mut disabled = renamed;
        disabled.enabled = false;
        state.queue(waiting, None);
        assert_eq!(state.revise(1, Some(&disabled)).len(), 1);
        assert!(!state.has_queued());
    }

    #[test]
    fn an_event_started_by_hand_keeps_its_time_when_changed() {
        let state = ScheduleState::default();
        let mut now = occurrence(1, EntryMode::AfterClip);
        now.start = at("2026-10-14 09:12:00");
        let mut moved = now.event.clone();
        moved.start_time = "15:00:00".to_string();

        state.trigger(now);
        assert!(state.revise(1, Some(&moved)).is_empty());
        assert_eq!(state.queued()[0].start, at("2026-10-14 09:12:00"));
        assert_eq!(state.queued()[0].event.entry_mode, EntryMode::Interrupt);
        assert_eq!(state.revise(1, None).len(), 1);
    }

    #[test]
    fn a_cut_clip_event_resumes_only_inside_its_length() {
        // 30 s of clips, on air at 13:00:02 because it waited for a clip.
        let clips = occurrence(1, EntryMode::AfterClip);
        let cut = run(Some("2026-10-14 13:00:02.000"));

        assert_eq!(
            resume_plan(&clips, &cut, at("2026-10-14 13:00:20")),
            ResumePlan::OnAir(Resume {
                on_air_at: at("2026-10-14 13:00:02"),
                shift_before: Some(-12.5),
            })
        );
        assert_eq!(
            resume_plan(&clips, &cut, at("2026-10-14 13:00:29")),
            ResumePlan::Over { aired: true }
        );
        assert_eq!(
            resume_plan(&clips, &run(None), at("2026-10-14 13:00:10")),
            ResumePlan::Wait
        );
        assert_eq!(
            resume_plan(&clips, &run(None), at("2026-10-14 13:05:00")),
            ResumePlan::Over { aired: false }
        );
    }

    #[test]
    fn a_cut_live_resumes_until_its_end_time() {
        let mut live = occurrence(1, EntryMode::Interrupt);
        live.event.kind = ScheduleKind::Live;
        live.end = at("2026-10-14 14:00:00");
        let cut = run(Some("2026-10-14 13:00:00.250"));

        assert!(matches!(
            resume_plan(&live, &cut, at("2026-10-14 13:40:00")),
            ResumePlan::OnAir(_)
        ));
        assert_eq!(
            resume_plan(&live, &cut, at("2026-10-14 13:59:58")),
            ResumePlan::Over { aired: true }
        );
    }

    #[test]
    fn a_restart_outdates_work_of_the_earlier_start() {
        let state = ScheduleState::default();
        let before = state.generation();

        state.queue_resumed(
            occurrence(1, EntryMode::AfterClip),
            Resume {
                on_air_at: at("2026-10-14 13:00:00"),
                shift_before: None,
            },
        );

        assert!(
            state.wants_interrupt(),
            "a resumed event goes on air at once"
        );

        state.reset();

        assert_ne!(state.generation(), before);
        assert!(!state.has_queued());
    }

    #[test]
    fn queue_keeps_order_and_asks_to_interrupt_only_when_needed() {
        let state = ScheduleState::default();

        state.queue(occurrence(1, EntryMode::AfterClip), None);

        assert!(!state.wants_interrupt());

        state.queue(occurrence(2, EntryMode::Interrupt), None);

        assert!(state.wants_interrupt());
        assert_eq!(state.take_queued().unwrap().occurrence.event.id, 1);
        assert!(state.wants_interrupt());
        assert_eq!(state.take_queued().unwrap().occurrence.event.id, 2);
        assert!(!state.wants_interrupt());
        assert!(state.take_queued().is_none());
    }

    #[test]
    fn manual_triggers_interrupt_and_abort_needs_an_active_event() {
        let state = ScheduleState::default();

        state.trigger(occurrence(3, EntryMode::AfterClip));

        assert!(state.wants_interrupt());
        assert_eq!(state.queued()[0].event.entry_mode, EntryMode::Interrupt);
        assert!(!state.request_abort());
        assert!(!state.take_abort());
    }

    #[test]
    fn a_second_end_while_the_event_closes_does_not_reach_what_follows() {
        let state = ScheduleState::default();

        state.lock().active = Some(on_air(ScheduleKind::Live, Some(at("2026-11-02 13:00:00"))));
        assert!(state.request_abort());
        assert!(state.take_abort());
        // The live loop took the first end; a second one comes in before the
        // event is closed.
        assert!(state.request_abort());

        state.close_active();

        assert!(!state.take_abort());
        assert!(!state.is_active());
        assert!(!state.request_abort());
    }

    #[test]
    fn waiting_screen_uses_the_filler_only_when_it_exists() {
        let mut config = PlayoutConfig::default();

        assert_eq!(waiting_source(&config), "");

        config.storage.filler = "missing.mp4".to_string();
        config.storage.filler_path = std::env::temp_dir().join("noar-missing-filler.mp4");

        assert_eq!(waiting_source(&config), "");
    }

    fn media_event_at(start_date: &str, start_time: &str) -> ScheduleEvent {
        ScheduleEvent {
            channel_id: 1,
            kind: ScheduleKind::Media,
            title: "Spring news".to_string(),
            sources: vec!["news.mp4".to_string()],
            start_date: start_date.to_string(),
            start_time: start_time.to_string(),
            duration: 600.0,
            enabled: true,
            ..ScheduleEvent::default()
        }
    }

    #[test]
    fn an_event_cut_by_the_output_stays_open_to_resume() {
        let failed: Result<(), ServiceError> = Err(ServiceError::Conflict("output lost".into()));

        assert!(stays_open(false, &failed));
        assert!(stays_open(true, &Ok(())));
        assert!(!stays_open(false, &Ok(())));
    }

    #[test]
    fn a_time_the_clock_skips_comes_when_the_gap_ends() {
        let berlin = Tz::Europe__Berlin;

        // 29 March 2026: Berlin goes from 02:00 straight to 03:00.
        assert_eq!(
            wall_clock_time(at("2026-03-29 02:30:00"), berlin),
            at("2026-03-29 03:00:00")
        );
        assert_eq!(
            wall_clock_time(at("2026-03-29 01:30:00"), berlin),
            at("2026-03-29 01:30:00")
        );
        // 25 October 2026: 02:30 comes twice; it is not moved.
        assert_eq!(
            wall_clock_time(at("2026-10-25 02:30:00"), berlin),
            at("2026-10-25 02:30:00")
        );
        assert_eq!(
            wall_clock_time(at("2026-03-29 02:30:00"), Tz::UTC),
            at("2026-03-29 02:30:00")
        );
    }

    #[test]
    fn a_clip_in_the_summer_time_gap_starts_when_the_gap_ends() {
        let berlin = Tz::Europe__Berlin;
        let grace = TimeDelta::try_seconds(MEDIA_START_GRACE_SECONDS).unwrap();
        let event = media_event_at("2026-03-29", "02:30:00");
        let occurrence = event
            .occurrence_on(parse_date("2026-03-29").unwrap())
            .unwrap();

        let now = at("2026-03-29 03:00:20");
        assert!(is_due(&occurrence, now, berlin, grace));
        assert!(starts_now(&occurrence, now, berlin, grace));

        // Past the grace after the gap it no longer starts, like any clip.
        assert!(!is_due(
            &occurrence,
            at("2026-03-29 03:05:00"),
            berlin,
            grace
        ));
        // In a zone without the gap nothing changes: 30 minutes late.
        assert!(!is_due(&occurrence, now, Tz::UTC, grace));
    }

    #[tokio::test]
    async fn a_database_error_while_claiming_is_retried_on_the_next_tick() {
        let (manager, pool) = crate::test_support::channel_manager().await;
        manager.config.write().await.channel.timezone = Some(Tz::Europe__Berlin);
        handles::insert_schedule_event(&pool, &media_event_at("2026-06-10", "12:00:00"))
            .await
            .unwrap();
        crate::utils::time_machine::set_mock_time(&Some("2026-06-10T12:00:05+02:00".to_string()))
            .unwrap();
        let mut seen = HashSet::new();
        let mut preloaded = HashSet::new();

        // The claim fails: nothing is queued, and the occurrence is not marked seen.
        sqlx::query("ALTER TABLE schedule_runs RENAME TO schedule_runs_away")
            .execute(&pool)
            .await
            .unwrap();
        tick(&manager, &mut seen, &mut preloaded).await.unwrap();
        assert!(manager.schedule.queued().is_empty());
        assert!(seen.is_empty());

        // Once the database answers again, the next tick queues it.
        sqlx::query("ALTER TABLE schedule_runs_away RENAME TO schedule_runs")
            .execute(&pool)
            .await
            .unwrap();
        tick(&manager, &mut seen, &mut preloaded).await.unwrap();
        assert_eq!(manager.schedule.queued().len(), 1);

        tokio::fs::remove_dir_all(manager.storage.root.read().await.clone())
            .await
            .unwrap();
    }
}
