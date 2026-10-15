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

use chrono::{NaiveDateTime, TimeDelta};
use ff_engine::{AsyncPlayout, ClipResult, NavigationBlocked};
use log::*;
use serde::Serialize;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::{
    db::{
        handles,
        models::{AfterMode, EntryMode, ScheduleEvent, ScheduleKind, UnfinishedRun},
    },
    player::{
        controller::ChannelManager,
        output::{play_plain, play_plain_from},
        utils::{Media, time_in_seconds},
    },
    utils::{
        config::PlayoutConfig,
        errors::ServiceError,
        schedule::{
            Occurrence, clip_duration,
            live::{live_protocol, read_live_url, test_signal},
            occurrences_between, seconds,
        },
        time_machine::time_now,
    },
};

/// How often the ticker looks for due events.
const TICK: Duration = Duration::from_millis(500);

/// A clip event whose start passed longer ago than this does not start any
/// more, for example after a restart.
const MEDIA_START_GRACE_SECONDS: i64 = 60;

/// Pause between attempts while a live source does not answer.
const LIVE_RETRY_SECONDS: f64 = 5.0;

/// A signal check younger than this is trusted when the live goes on air.
const FRESH_SIGNAL: Duration = Duration::from_secs(10);

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
    #[serde(skip)]
    started: Instant,
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
}

#[derive(Debug, Default)]
struct Inner {
    /// Enabled events of the channel; `None` reloads them from the database.
    events: Option<Vec<ScheduleEvent>>,
    queue: VecDeque<Queued>,
    active: Option<ActiveEvent>,
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
        inner.abort = false;
        inner.interrupt_wanted = false;
        inner.generation += 1;
    }

    fn generation(&self) -> u64 {
        self.lock().generation
    }

    fn queue(&self, occurrence: Occurrence, signal_checked: Option<Instant>) {
        self.push(occurrence, signal_checked, None);
    }

    /// Queue an occurrence that was on air when the playout stopped; it goes
    /// back on air right away.
    fn queue_resumed(&self, mut occurrence: Occurrence, resume: Resume) {
        occurrence.event.entry_mode = EntryMode::Interrupt;
        self.push(occurrence, None, Some(resume));
    }

    fn push(
        &self,
        occurrence: Occurrence,
        signal_checked: Option<Instant>,
        resume: Option<Resume>,
    ) {
        let mut inner = self.lock();

        if occurrence.event.entry_mode == EntryMode::Interrupt {
            inner.interrupt_wanted = true;
        }

        inner.queue.push_back(Queued {
            occurrence,
            signal_checked,
            resume,
        });
    }

    /// Queue an event to start right away, cutting what is on air.
    pub fn trigger(&self, mut occurrence: Occurrence) {
        occurrence.event.entry_mode = EntryMode::Interrupt;
        self.queue(occurrence, None);
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

    pub fn is_active(&self) -> bool {
        self.lock().active.is_some()
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

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(TICK) => {}
        }

        if let Err(error) = tick(&manager, &mut seen).await {
            warn!(channel = manager.id; "Scheduler: {error}");
        }
    }
}

async fn tick(
    manager: &ChannelManager,
    seen: &mut HashSet<(i32, NaiveDateTime)>,
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
    let now = time_now(&timezone).naive_local();
    let grace = TimeDelta::try_seconds(MEDIA_START_GRACE_SECONDS).unwrap_or_default();
    let due = occurrences_between(&events, now - grace, now + seconds(1.0));

    for occurrence in due.into_iter().filter(|o| o.start <= now) {
        let key = (occurrence.event.id, occurrence.start);

        if !seen.insert(key) {
            continue;
        }

        let still_on_time = match occurrence.event.kind {
            ScheduleKind::Media => now - occurrence.start <= grace,
            ScheduleKind::Live => occurrence.end - now > seconds(5.0),
        };
        let claimed = still_on_time
            && handles::claim_schedule_run(
                &manager.db_pool,
                occurrence.event.id,
                &occurrence.date_key(),
                "running",
            )
            .await?;

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

    if manager.schedule.wants_interrupt() && request_interrupt(manager).await {
        manager.schedule.interrupt_sent();
    }

    Ok(())
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
        if manager.schedule.generation() != generation || !manager.is_alive.load(Ordering::SeqCst) {
            return;
        }

        let timezone = manager.config.read().await.channel.timezone;
        let now = time_now(&timezone).naive_local();

        if occurrence.end - now <= seconds(5.0) {
            break;
        }

        if test_signal(&url).await.is_ok_and(|test| test.ok) {
            if manager.schedule.generation() == generation {
                manager.schedule.queue(occurrence, Some(Instant::now()));
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
    finish_run(&manager, &occurrence, "failed").await;
}

/// Cut the clip on air. `false` when a live ingest is on air or another
/// navigation is in progress; the caller tries again on the next tick.
async fn request_interrupt(manager: &ChannelManager) -> bool {
    let control = manager.playback_control.lock().await;

    match control.begin_navigation() {
        Ok(navigation) => {
            navigation.commit();
            true
        }
        Err(NavigationBlocked::Live) | Err(NavigationBlocked::Busy) => false,
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

    manager.schedule.lock().active = Some(ActiveEvent {
        event_id: event.id,
        title: event.title.clone(),
        kind: event.kind,
        entry_mode: event.entry_mode,
        after_mode: event.after_mode,
        planned_seconds,
        elapsed_seconds: 0.0,
        item: None,
        interrupted,
        started,
    });
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

    let result = match event.kind {
        ScheduleKind::Media => play_clips(manager, config, playout, &event, already_aired).await,
        ScheduleKind::Live => {
            play_live(manager, config, playout, &occurrence, queued.signal_checked).await
        }
    };
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
    manager.schedule.lock().active = None;

    // The playout stopping cut the event: it stays open, and the next start
    // puts it back on air if its time is not over.
    if !manager.is_alive.load(Ordering::SeqCst) {
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

struct EventEnd {
    /// Played to the end, or stopped by the operator.
    completed: bool,
    /// The desktop window was closed.
    stopped: bool,
}

/// Play the event's clips in order, leaving out the first `skip` seconds (the
/// part a resumed event already aired).
async fn play_clips(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    playout: &AsyncPlayout,
    event: &ScheduleEvent,
    mut skip: f64,
) -> Result<EventEnd, ServiceError> {
    let id = config.general.channel_id;
    let mut completed = true;

    for source in &event.sources {
        if manager.schedule.take_abort() {
            break;
        }

        let path = config.channel.storage.join(source);
        let Some(duration) = clip_duration(&path).await else {
            error!(channel = id; "Scheduled clip <span class=\"log-addr\">{source}</span> cannot be read; skipped");
            completed = false;
            continue;
        };

        if skip >= duration {
            skip -= duration;
            continue;
        }

        let seek = std::mem::take(&mut skip);
        let length = duration - seek;

        manager.schedule.set_item(source.clone());
        show_on_air(manager, &path.to_string_lossy(), &event.title, length).await;
        info!(channel = id;
            "Play scheduled clip for <span class=\"log-number\">{length:.1}</span> s from <span class=\"log-number\">{seek:.1}</span> s: <span class=\"log-addr\">{}</span>",
            path.display()
        );

        if matches!(
            play_plain_from(playout, config, &path.to_string_lossy(), seek, length).await?,
            ClipResult::Stopped
        ) {
            return Ok(EventEnd {
                completed,
                stopped: true,
            });
        }
    }

    Ok(EventEnd {
        completed,
        stopped: false,
    })
}

async fn play_live(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    playout: &AsyncPlayout,
    occurrence: &Occurrence,
    signal_checked: Option<Instant>,
) -> Result<EventEnd, ServiceError> {
    let id = config.general.channel_id;
    let event = &occurrence.event;
    let Some(path) = event.sources.first() else {
        return Ok(EventEnd {
            completed: false,
            stopped: false,
        });
    };
    let url = match read_live_url(&config.channel.storage.join(path)).await {
        Ok(url) => url,
        Err(error) => {
            error!(channel = id; "Live <span class=\"log-addr\">{}</span>: {error}", event.title);
            return Ok(EventEnd {
                completed: false,
                stopped: false,
            });
        }
    };
    let protocol = live_protocol(&url);
    let retry = Duration::from_secs(u64::from(event.retry_seconds));
    let mut trusted_signal = signal_checked.is_some_and(|checked| checked.elapsed() < FRESH_SIGNAL);
    let mut lost_since: Option<Instant> = None;

    loop {
        if manager.schedule.take_abort() {
            return Ok(EventEnd {
                completed: true,
                stopped: false,
            });
        }

        let now = time_now(&config.channel.timezone).naive_local();
        let remaining = (occurrence.end - now).num_milliseconds() as f64 / 1000.0;

        if remaining < 1.0 {
            return Ok(EventEnd {
                completed: true,
                stopped: false,
            });
        }

        let signal = trusted_signal || test_signal(&url).await.is_ok_and(|test| test.ok);
        trusted_signal = false;

        if signal {
            lost_since = None;
            manager.schedule.set_item(protocol.to_string());
            show_on_air(manager, &url, &event.title, remaining).await;
            info!(channel = id;
                "Live <span class=\"log-addr\">{}</span> on air from {protocol} for <span class=\"log-number\">{remaining:.0}</span> s",
                event.title
            );

            if matches!(
                play_plain(playout, config, &url, remaining).await?,
                ClipResult::Stopped
            ) {
                return Ok(EventEnd {
                    completed: true,
                    stopped: true,
                });
            }

            // Ending before the end time means the signal was lost.
            continue;
        }

        let lost = *lost_since.get_or_insert_with(Instant::now);

        if lost.elapsed() >= retry {
            error!(channel = id;
                "Live <span class=\"log-addr\">{}</span>: no signal for {} s; back to the programme",
                event.title,
                retry.as_secs()
            );

            return Ok(EventEnd {
                completed: false,
                stopped: false,
            });
        }

        manager
            .schedule
            .set_item(format!("{protocol} · waiting for signal"));
        warn!(channel = id; "Live <span class=\"log-addr\">{}</span>: waiting for the signal", event.title);

        if matches!(
            play_plain(
                playout,
                config,
                &waiting_source(config),
                remaining.min(LIVE_RETRY_SECONDS)
            )
            .await?,
            ClipResult::Stopped
        ) {
            return Ok(EventEnd {
                completed: false,
                stopped: true,
            });
        }
    }
}

/// Report the event's clip as the media on air, so the player view shows
/// its name and progress instead of the interrupted clip.
async fn show_on_air(manager: &ChannelManager, source: &str, title: &str, duration: f64) {
    let begin = {
        let channel = manager.channel.lock().await;

        time_in_seconds(&channel.timezone) + channel.time_shift
    };
    let mut media = Media::new(0, source, false).await;

    media.index = None;
    media.begin = Some(begin);
    media.seek = 0.0;
    media.out = duration;
    media.duration = duration;
    media.title = Some(title.to_string());

    *manager.current_media.lock().await = Some(media);
}

/// What airs while a live waits for its signal: the filler clip when one is
/// set, else black (the engine generates it for an empty source).
fn waiting_source(config: &PlayoutConfig) -> String {
    let filler = &config.storage.filler_path;

    if !config.storage.filler.trim().is_empty() && filler.is_file() {
        filler.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

async fn finish_run(manager: &ChannelManager, occurrence: &Occurrence, status: &str) {
    close_run(manager, occurrence.event.id, &occurrence.date_key(), status).await;
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
    fn waiting_screen_uses_the_filler_only_when_it_exists() {
        let mut config = PlayoutConfig::default();

        assert_eq!(waiting_source(&config), "");

        config.storage.filler = "missing.mp4".to_string();
        config.storage.filler_path = std::env::temp_dir().join("noar-missing-filler.mp4");

        assert_eq!(waiting_source(&config), "");
    }
}
