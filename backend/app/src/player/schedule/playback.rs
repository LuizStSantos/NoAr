//! Scheduled-event execution and live-source handover.
//!
//! The parent owns occurrence eligibility, the queue and restart generations.
//! This module executes an already selected event, prepares source connections,
//! records waiting screens and preserves the programme's preload/return timing.

use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use log::*;
use noar_engine::{AsyncPlayout, ClipBreaks, ClipResult, PreloadSignal};
use tokio::{task::JoinHandle, time::sleep};

use crate::{
    db::models::{AfterMode, ScheduleEvent, ScheduleKind},
    player::{
        as_run::{self, AiredEnd},
        controller::ChannelManager,
        output::{play_plain, play_plain_from, preload_later},
        utils::{Media, time_in_seconds},
    },
    utils::{
        commercial::CommercialFolders,
        config::PlayoutConfig,
        errors::ServiceError,
        schedule::{
            Occurrence,
            live::{live_protocol, read_live_url},
            trimmed_clip, trimmed_length,
        },
        time_machine::time_now,
    },
};

use super::{LIVE_RETRY_SECONDS, PRELOAD_LEAD_SECONDS, close_run};

/// A signal check younger than this is trusted when the live goes on air.
const FRESH_SIGNAL: Duration = Duration::from_secs(10);
/// How long a live source gets to send its first packet.
const SIGNAL_WAIT: Duration = Duration::from_secs(12);
const SIGNAL_POLL: Duration = Duration::from_millis(200);

pub(super) struct EventEnd {
    /// Played to the end, or stopped by the operator.
    pub(super) completed: bool,
    /// The desktop window was closed.
    pub(super) stopped: bool,
}

/// Play the event's clips in order, leaving out the first `skip` seconds (the
/// part a resumed event already aired).
pub(super) async fn play_clips(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    playout: &AsyncPlayout,
    event: &ScheduleEvent,
    scheduled_at: Option<i64>,
    mut skip: f64,
) -> Result<EventEnd, ServiceError> {
    let id = config.general.channel_id;
    let mut completed = true;
    let mut commercials = CommercialFolders::new(&config.channel.storage);
    // The run of commercials on air or next has had its SCTE-35 break.
    let mut break_announced = false;
    // Assembled programs play as their blocks, breaks and lives, as they
    // are now. A live in a program airs for the length the program gives it.
    let expanded;
    let mut live_lengths: Vec<Option<f64>> = vec![];
    let event = if event
        .sources
        .iter()
        .any(|source| crate::utils::assembled::is_program(source))
    {
        let (sources, lengths) = crate::utils::assembled::expand_sources_with_lengths(
            &config.channel.storage,
            &event.sources,
        )
        .await
        .into_iter()
        .unzip();
        live_lengths = lengths;
        expanded = ScheduleEvent {
            sources,
            ..event.clone()
        };
        &expanded
    } else {
        event
    };

    // The progress in the player view runs over the whole event, from where
    // it started (before a resumed part) for its planned length.
    let event_begin = shifted_now(manager).await - skip;

    for (index, source) in event.sources.iter().enumerate() {
        if manager.schedule.take_abort() || stopping(manager) {
            break;
        }

        let mut path = config.channel.storage.join(source);
        let live_length = live_lengths.get(index).copied().flatten();
        // A live airs at its current address; a clip keeps its saved trim.
        let timing = match live_length {
            Some(length) => match read_live_url(&path).await {
                Ok(url) => {
                    path = PathBuf::from(url);
                    Some((0.0, length))
                }
                Err(error) => {
                    error!(channel = id; "Live <span class=\"log-addr\">{source}</span> of the program: {error}");
                    None
                }
            },
            None => trimmed_clip(&path).await,
        };
        let Some((start, duration)) = timing else {
            error!(channel = id; "Scheduled clip <span class=\"log-addr\">{source}</span> cannot be read; skipped");
            completed = false;
            continue;
        };

        if skip >= duration {
            skip -= duration;
            continue;
        }

        let resumed = std::mem::take(&mut skip);
        let seek = start + resumed;
        let length = duration - resumed;

        manager.schedule.set_item(source.clone());
        // A program changed since it was scheduled may run longer than planned.
        let clip_end = shifted_now(manager).await - event_begin + length;
        show_on_air(
            manager,
            &path.to_string_lossy(),
            &event.title,
            event_begin,
            event.duration.max(clip_end),
        )
        .await;
        info!(channel = id;
            "Play scheduled clip for <span class=\"log-number\">{length:.1}</span> s from <span class=\"log-number\">{seek:.1}</span> s: <span class=\"log-addr\">{}</span>",
            path.display()
        );

        let commercial = commercials.contains(source);
        let entry = as_run::event_clip_entry(event, scheduled_at, source, seek, length, commercial);
        let aired = as_run::record_start(manager, entry).await;
        // A live of a program connects when its turn comes.
        let next_is_live = live_lengths.get(index + 1).copied().flatten().is_some();
        let next_preload = match event.sources.get(index + 1).filter(|_| !next_is_live) {
            Some(next) => {
                let next = config.channel.storage.join(next);

                Some(preload_later(
                    manager.preloader.lock().await.clone(),
                    next.to_string_lossy().to_string(),
                    length,
                ))
            }
            None => None,
        };
        let mut breaks = event_breaks(
            config,
            event,
            index,
            (commercial, length),
            &mut commercials,
            &mut break_announced,
        )
        .await;
        // Breaks marked inside the video itself, when it is not a commercial.
        if !commercial {
            breaks.within = crate::utils::logo_marks::read_marks(&path)
                .await
                .breaks_in_clip(seek, Some(length));
        }
        manager.splice_control.plan_next_clip(breaks.clone());
        let result = play_plain_from(
            playout,
            config,
            &path.to_string_lossy(),
            seek,
            length,
            commercial,
        )
        .await;

        if let Some(task) = next_preload {
            task.abort();
        }

        if breaks.after.is_some() && !matches!(result, Ok(ClipResult::Played)) {
            break_announced = false;
        }

        let end = match &result {
            Ok(result) => as_run::event_clip_end(manager, result),
            Err(error) => AiredEnd::Failed(error.to_string()),
        };

        as_run::record_end(manager, aired, end).await;

        if matches!(result?, ClipResult::Stopped) {
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

/// The SCTE-35 break around clip `index` of a scheduled event, like the
/// playlist's: one after it when commercials follow, one with it when it
/// opens a run of commercials no earlier clip announced.
async fn event_breaks(
    config: &PlayoutConfig,
    event: &ScheduleEvent,
    index: usize,
    (commercial, length): (bool, f64),
    commercials: &mut CommercialFolders,
    announced: &mut bool,
) -> ClipBreaks {
    if !config.scte35.enable || (commercial && *announced) {
        return ClipBreaks::default();
    }

    let mut commercials_next = 0.0;

    for source in event.sources.iter().skip(index + 1) {
        if !commercials.contains(source) {
            break;
        }

        match trimmed_length(&config.channel.storage.join(source)).await {
            Some(duration) => commercials_next += duration,
            None => break,
        }
    }

    let (run, breaks) = if commercial {
        (
            length + commercials_next,
            ClipBreaks {
                at_start: Some(length + commercials_next),
                after: None,
                within: Vec::new(),
            },
        )
    } else {
        *announced = false;

        if commercials_next <= 0.0 {
            return ClipBreaks::default();
        }

        (
            commercials_next,
            ClipBreaks {
                at_start: None,
                after: Some(commercials_next),
                within: Vec::new(),
            },
        )
    };

    *announced = true;

    if config.scte35.signals(run) {
        breaks
    } else {
        warn!(channel = config.general.channel_id;
            "SCTE-35: commercials of <span class=\"log-number\">{run:.1}</span> s in <span class=\"log-addr\">{}</span> not signalled; breaks run from {} to {} s",
            event.title,
            config.scte35.min_break,
            config.scte35.max_break
        );
        ClipBreaks::default()
    }
}

pub(super) async fn play_live(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    playout: &AsyncPlayout,
    occurrence: &Occurrence,
    scheduled_at: Option<i64>,
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
    let mut check: Option<SignalCheck> = None;
    // The on-air record of the waiting screen, from when it first airs until
    // the signal answers or the event gives up.
    let mut waiting: Option<Option<i64>> = None;
    // Extended on air: it runs to the new end.
    let seconds_left = || {
        let now = time_now(&config.channel.timezone).naive_local();

        (occurrence.end + manager.schedule.live_extension() - now).num_milliseconds() as f64
            / 1000.0
    };

    loop {
        // The stop cut what aired; going on would put the live back on air
        // in a playout that is shutting down.
        if stopping(manager) {
            end_waiting(manager, &mut waiting, AiredEnd::Stopped).await;

            return Ok(EventEnd {
                completed: false,
                stopped: false,
            });
        }

        if manager.schedule.take_abort() {
            end_waiting(manager, &mut waiting, AiredEnd::cut("skipped", None)).await;

            return Ok(EventEnd {
                completed: true,
                stopped: false,
            });
        }

        let remaining = seconds_left();

        if remaining < 1.0 {
            end_waiting(manager, &mut waiting, AiredEnd::Completed).await;

            return Ok(EventEnd {
                completed: true,
                stopped: false,
            });
        }

        if trusted_signal {
            trusted_signal = false;
            lost_since = None;
            end_waiting(manager, &mut waiting, AiredEnd::Completed).await;
            manager.schedule.set_item(protocol.to_string());
            let now = shifted_now(manager).await;
            show_on_air(manager, &url, &event.title, now, remaining).await;
            info!(channel = id;
                "Live <span class=\"log-addr\">{}</span> on air from {protocol} for <span class=\"log-number\">{remaining:.0}</span> s",
                event.title
            );

            let entry = as_run::event_live_entry(event, scheduled_at, remaining);
            let aired = as_run::record_start(manager, entry).await;
            let aired_from = Instant::now();
            let result = play_plain(playout, config, &url, remaining).await;
            // It ran its whole time and was extended meanwhile: the signal is
            // fine, so it goes on at once (its connection was kept open).
            let ran_its_time = aired_from.elapsed().as_secs_f64() >= remaining - 1.0;
            let end = match &result {
                Ok(result) => as_run::live_end(manager, result, seconds_left()),
                Err(error) => AiredEnd::Failed(error.to_string()),
            };

            as_run::record_end(manager, aired, end).await;

            if matches!(result?, ClipResult::Stopped) {
                return Ok(EventEnd {
                    completed: true,
                    stopped: true,
                });
            }

            if ran_its_time && seconds_left() >= 1.0 {
                trusted_signal = true;
                info!(channel = id;
                    "Live <span class=\"log-addr\">{}</span> extended; on air for <span class=\"log-number\">{:.0}</span> s more",
                    event.title,
                    seconds_left()
                );
            }

            // Ending before the end time means the signal was lost.
            continue;
        }

        // Not known to answer: the waiting screen airs while the source is
        // checked, so the output never stops for the check.
        let source = waiting_source(config);

        if waiting.is_none() {
            let entry = as_run::waiting_entry(config, event, scheduled_at, &source);

            waiting = Some(as_run::record_start(manager, entry).await);
        }

        manager
            .schedule
            .set_item(format!("{protocol} · waiting for signal"));

        let running = check.get_or_insert_with(|| SignalCheck::start(manager, &url));
        let (answer, result) = wait_on_air(
            playout,
            config,
            &source,
            remaining.min(LIVE_RETRY_SECONDS),
            running,
        )
        .await;

        if answer.is_some() {
            check = None;
        }

        match &result {
            Ok(ClipResult::Stopped) => end_waiting(manager, &mut waiting, AiredEnd::Stopped).await,
            // Cut short for the live, whose start closes the record.
            Ok(ClipResult::Skipped) if answer != Some(true) => {
                end_waiting(manager, &mut waiting, AiredEnd::cut("skipped", None)).await;
            }
            Err(error) => {
                end_waiting(manager, &mut waiting, AiredEnd::Failed(error.to_string())).await;
            }
            Ok(_) => {}
        }

        if matches!(result?, ClipResult::Stopped) {
            return Ok(EventEnd {
                completed: false,
                stopped: true,
            });
        }

        match answer {
            Some(true) => trusted_signal = true,
            Some(false) => {
                let lost = *lost_since.get_or_insert_with(Instant::now);

                if lost.elapsed() >= retry {
                    end_waiting(manager, &mut waiting, AiredEnd::Completed).await;
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

                warn!(channel = id; "Live <span class=\"log-addr\">{}</span>: waiting for the signal", event.title);
            }
            None => {}
        }
    }
}

/// A signal check running beside the playout; dropping it stops the check.
struct SignalCheck(JoinHandle<bool>);

impl SignalCheck {
    fn start(manager: &ChannelManager, url: &str) -> Self {
        let manager = manager.clone();
        let url = url.to_string();

        Self(tokio::spawn(
            async move { live_answers(&manager, &url).await },
        ))
    }
}

/// Whether the live source is sending. The check is its preload: the
/// connection stays open for the takeover, so a source that takes one caller
/// at a time is not called twice, and the switch needs no new connection.
pub(super) async fn live_answers(manager: &ChannelManager, url: &str) -> bool {
    let preloader = manager.preloader.lock().await.clone();
    let id = manager.config.read().await.general.channel_id;
    let deadline = Instant::now() + SIGNAL_WAIT;

    preloader.preload_live(url);

    loop {
        match preloader.signal(url) {
            Some(PreloadSignal::Ready) => return true,
            None => return false,
            Some(PreloadSignal::Failed(reason)) => {
                warn!(channel = id; "Signal check of {url}: {reason}");

                return false;
            }
            Some(signal @ (PreloadSignal::Opening | PreloadSignal::Stalled))
                if Instant::now() >= deadline =>
            {
                warn!(channel = id;
                    "Signal check of {url}: {} after {} s",
                    if signal == PreloadSignal::Opening { "still connecting, or no picture yet" } else { "no media" },
                    SIGNAL_WAIT.as_secs()
                );

                return false;
            }
            Some(_) => sleep(SIGNAL_POLL).await,
        }
    }
}

/// Open what an event starts with ahead of its start, so the switch to it
/// is immediate. A live connects and stays at its live edge.
pub(super) async fn preload_event(manager: &ChannelManager, occurrence: &Occurrence) {
    let storage = manager.config.read().await.channel.storage.clone();
    // An assembled program starts with its first clip.
    let Some(source) = crate::utils::assembled::expand_sources(
        &storage,
        &occurrence.event.sources[..occurrence.event.sources.len().min(1)],
    )
    .await
    .into_iter()
    .next() else {
        return;
    };
    let path = storage.join(source);
    let target = match occurrence.event.kind {
        ScheduleKind::Media => path.to_string_lossy().to_string(),
        ScheduleKind::Live => match read_live_url(&path).await {
            Ok(url) => url,
            Err(_) => return,
        },
    };

    let preloader = manager.preloader.lock().await.clone();
    if occurrence.event.kind == ScheduleKind::Live {
        preloader.preload_live(&target);
    } else {
        preloader.preload(&target);
    }
}

/// Open the clip the programme goes back to shortly before the event ends,
/// as an event's start is opened before it: leaving a live or an event is
/// then as immediate as entering it.
pub(super) fn preload_return(
    manager: &ChannelManager,
    config: &PlayoutConfig,
    after_mode: AfterMode,
    shift_before: f64,
    started: Instant,
    planned_seconds: f64,
) -> JoinHandle<()> {
    let manager = manager.clone();
    let timezone = config.channel.timezone;
    let start_sec = config.playlist.start_sec.unwrap_or_default();
    let left = planned_seconds - started.elapsed().as_secs_f64();
    let wait = Duration::from_secs_f64((left - PRELOAD_LEAD_SECONDS).max(0.0));

    tokio::spawn(async move {
        sleep(wait).await;

        // Where the playlist takes up when the event ends, with the time
        // shift the event leaves behind.
        let shift = match after_mode {
            AfterMode::Continue => {
                shift_before - (started.elapsed().as_secs_f64() + PRELOAD_LEAD_SECONDS)
            }
            AfterMode::Clock => shift_before,
        };
        let mut position = time_in_seconds(&timezone) + PRELOAD_LEAD_SECONDS;

        if position < start_sec {
            position += 86400.0;
        }

        let source = {
            let list = manager.current_list.lock().await;

            list.iter()
                .find(|item| {
                    item.begin.unwrap_or(start_sec) + item.out - item.seek > position + shift
                })
                .map(|item| item.source.clone())
                .filter(|source| !source.trim().is_empty())
        };

        if let Some(source) = source {
            manager.preloader.lock().await.clone().preload(&source);
        }
    })
}

/// Close a live's connection that nothing will take any more.
pub(super) async fn forget_live(manager: &ChannelManager, event: &ScheduleEvent) {
    let Some(source) = event.sources.first() else {
        return;
    };
    let path = manager.config.read().await.channel.storage.join(source);

    if let Ok(url) = read_live_url(&path).await {
        manager.preloader.lock().await.clone().forget(&url);
    }
}

impl Drop for SignalCheck {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Air the waiting screen `source` for up to `seconds` while `check` runs. A
/// source that answers cuts the waiting screen short. Returns the answer, if
/// it came, and how the waiting screen ended.
async fn wait_on_air(
    playout: &AsyncPlayout,
    config: &PlayoutConfig,
    source: &str,
    seconds: f64,
    check: &mut SignalCheck,
) -> (Option<bool>, Result<ClipResult, ServiceError>) {
    let waiting = play_plain(playout, config, source, seconds);

    tokio::pin!(waiting);

    tokio::select! {
        result = &mut waiting => {
            let answer = if check.0.is_finished() {
                Some((&mut check.0).await.unwrap_or(false))
            } else {
                None
            };

            (answer, result)
        }
        answer = &mut check.0 => {
            let answered = answer.unwrap_or(false);

            if !answered {
                return (Some(false), waiting.await);
            }

            let control = playout.playback_control();

            control.skip_current();

            let result = waiting.await;

            // The waiting screen may have ended just before the skip, which
            // must not cut the live instead.
            control.cancel_skip();

            (Some(true), result)
        }
    }
}

/// Close the on-air record of a live's waiting screen, if one is open.
async fn end_waiting(manager: &ChannelManager, waiting: &mut Option<Option<i64>>, end: AiredEnd) {
    if let Some(id) = waiting.take() {
        let end = match end {
            // The playout stopping is not an operator's skip.
            AiredEnd::Cut { .. } if stopping(manager) => AiredEnd::Stopped,
            end => end,
        };

        as_run::record_end(manager, id, end).await;
    }
}

/// The channel clock as the player view counts it: with the time shift.
async fn shifted_now(manager: &ChannelManager) -> f64 {
    let channel = manager.channel.lock().await;

    time_in_seconds(&channel.timezone) + channel.time_shift
}

/// Report the event as the media on air, so the player view shows its name
/// and progress instead of the interrupted clip: `source` is the clip on air,
/// and the progress runs over the whole event, which started at `begin` (see
/// [`shifted_now`]) and lasts `length` seconds.
async fn show_on_air(manager: &ChannelManager, source: &str, title: &str, begin: f64, length: f64) {
    let mut media = Media::new(0, source, false).await;

    media.index = None;
    media.begin = Some(begin);
    media.seek = 0.0;
    media.out = length;
    media.duration = length;
    media.title = Some(title.to_string());

    *manager.current_media.lock().await = Some(media);
}

/// What airs while a live waits for its signal: the filler clip when one is
/// set, else black (the engine generates it for an empty source).
pub(super) fn waiting_source(config: &PlayoutConfig) -> String {
    let filler = &config.storage.filler_path;

    if !config.storage.filler.trim().is_empty() && filler.is_file() {
        filler.to_string_lossy().to_string()
    } else {
        String::new()
    }
}

/// The playout is stopping or restarting: an event on air ends with it, and
/// the next start puts it back on air.
pub(super) fn stopping(manager: &ChannelManager) -> bool {
    !manager.is_alive.load(Ordering::SeqCst)
}

pub(super) async fn finish_run(manager: &ChannelManager, occurrence: &Occurrence, status: &str) {
    close_run(manager, occurrence.event.id, &occurrence.date_key(), status).await;
}
