//! Records what actually goes on air, for the report: a row opens when an
//! item starts and closes when it ends, with why it ended early. Recording
//! never stops the playout: a failed write is only logged.

use std::{sync::atomic::Ordering, time::Duration};

use chrono::{NaiveDateTime, TimeZone};
use log::*;
use noar_engine::{ClipResult, PlaybackControl};
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::{
    db::{
        handles::{self, CutBy},
        models::{NewAsRunEntry, ScheduleEvent},
    },
    player::{
        controller::ChannelManager,
        utils::{Media, is_live_source},
    },
    utils::{
        config::{PlayoutConfig, ProcessMode},
        paths::storage_relative,
        schedule::event_name,
        time_machine::time_now,
    },
};

/// How often the ingest watcher looks at the live takeover state.
const INGEST_POLL: Duration = Duration::from_millis(500);

/// Longest error text kept with an item that failed.
const MAX_DETAIL_CHARS: usize = 300;

/// How an item left the air.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiredEnd {
    /// Played to its planned end.
    Completed,
    /// Ended early: `reason` is `event`, `ingest`, `skipped` or `signal_lost`,
    /// `detail` names the event or the listener.
    Cut {
        reason: &'static str,
        detail: Option<String>,
    },
    /// The source could not be played; black or the filler aired instead.
    Failed(String),
    /// The playout stopped.
    Stopped,
}

impl AiredEnd {
    pub fn cut(reason: &'static str, detail: Option<String>) -> Self {
        Self::Cut { reason, detail }
    }

    /// Status, reason and detail as stored.
    fn columns(&self) -> (&'static str, Option<&'static str>, Option<String>) {
        match self {
            Self::Completed => ("completed", None, None),
            Self::Cut { reason, detail } => ("cut", Some(reason), detail.clone()),
            Self::Failed(error) => (
                "failed",
                Some("error"),
                Some(error.chars().take(MAX_DETAIL_CHARS).collect()),
            ),
            Self::Stopped => ("stopped", None, None),
        }
    }
}

/// A clip of the programme: from the playlist, or from the folder in folder
/// mode.
pub fn programme_entry(config: &PlayoutConfig, node: &Media) -> NewAsRunEntry {
    let kind = if node.is_placeholder || node.source.is_empty() {
        "filler"
    } else if node.ad {
        "commercial"
    } else if is_live_source(&node.source) {
        "live"
    } else {
        "program"
    };
    let origin = match config.processing.mode {
        ProcessMode::Folder => "folder",
        ProcessMode::Playlist => "playlist",
    };

    NewAsRunEntry {
        source: storage_relative(&config.channel.storage, &node.source),
        title: node.title.clone().filter(|title| !title.trim().is_empty()),
        kind,
        origin,
        seek: node.seek,
        planned_seconds: (node.out - node.seek).max(0.0),
        replaces: node
            .replaces
            .as_deref()
            .map(|source| storage_relative(&config.channel.storage, source)),
        ..NewAsRunEntry::default()
    }
}

/// A clip of a scheduled event; `source` is its storage path.
pub fn event_clip_entry(
    event: &ScheduleEvent,
    scheduled_at: Option<i64>,
    source: &str,
    seek: f64,
    length: f64,
    commercial: bool,
) -> NewAsRunEntry {
    NewAsRunEntry {
        source: source.to_string(),
        kind: if commercial { "commercial" } else { "program" },
        seek,
        planned_seconds: length,
        ..event_entry(event, scheduled_at)
    }
}

/// A scheduled live on air, for at most `length` seconds.
pub fn event_live_entry(
    event: &ScheduleEvent,
    scheduled_at: Option<i64>,
    length: f64,
) -> NewAsRunEntry {
    NewAsRunEntry {
        source: event.sources.first().cloned().unwrap_or_default(),
        kind: "live",
        planned_seconds: length,
        ..event_entry(event, scheduled_at)
    }
}

/// What airs while a scheduled live waits for its signal.
pub fn waiting_entry(
    config: &PlayoutConfig,
    event: &ScheduleEvent,
    scheduled_at: Option<i64>,
    source: &str,
) -> NewAsRunEntry {
    NewAsRunEntry {
        source: storage_relative(&config.channel.storage, source),
        kind: "filler",
        replaces: event.sources.first().cloned(),
        ..event_entry(event, scheduled_at)
    }
}

fn event_entry(event: &ScheduleEvent, scheduled_at: Option<i64>) -> NewAsRunEntry {
    NewAsRunEntry {
        origin: "schedule",
        event_id: Some(event.id),
        event_title: Some(event_name(event)),
        scheduled_at,
        ..NewAsRunEntry::default()
    }
}

/// Unix milliseconds of a channel-local time, such as an occurrence's start.
pub fn local_millis(config: &PlayoutConfig, local: NaiveDateTime) -> Option<i64> {
    time_now(&config.channel.timezone)
        .timezone()
        .from_local_datetime(&local)
        .earliest()
        .map(|time| time.timestamp_millis())
}

/// Record an item going on air; returns its row, for [`record_end`].
pub async fn record_start(manager: &ChannelManager, entry: NewAsRunEntry) -> Option<i64> {
    insert(manager, &entry, CutBy::default()).await
}

/// Record how the item of row `id` left the air.
pub async fn record_end(manager: &ChannelManager, id: Option<i64>, end: AiredEnd) {
    let Some(id) = id else {
        return;
    };
    let (status, reason, detail) = end.columns();

    if let Err(error) = handles::finish_as_run(
        &manager.db_pool,
        id,
        now_millis(),
        status,
        reason,
        detail.as_deref(),
    )
    .await
    {
        error!(channel = manager.id; "Unable to record the end of the item on air: {error}");
    }
}

/// Close what a playout that did not stop cleanly left on air.
pub async fn close_unfinished(manager: &ChannelManager) {
    match handles::close_open_as_run(&manager.db_pool, manager.id, now_millis()).await {
        Ok(0) => {}
        Ok(count) => {
            info!(channel = manager.id; "Closed {count} on-air record(s) left open by the last run");
        }
        Err(error) => {
            error!(channel = manager.id; "Unable to close the on-air records: {error}");
        }
    }
}

/// How a clip of the programme ended, from the engine's result. Call it
/// before the play loop acts on the result: a skip is told apart from an
/// event cutting in by the event still waiting in the queue.
pub fn programme_end(manager: &ChannelManager, result: &ClipResult) -> AiredEnd {
    match result {
        ClipResult::Skipped if manager.is_alive.load(Ordering::SeqCst) => {
            match manager.schedule.queued().first() {
                Some(next) => AiredEnd::cut("event", Some(event_name(&next.event))),
                None => AiredEnd::cut("skipped", None),
            }
        }
        _ => common_end(manager, result),
    }
}

/// How a clip of a scheduled event ended.
pub fn event_clip_end(manager: &ChannelManager, result: &ClipResult) -> AiredEnd {
    common_end(manager, result)
}

/// How a scheduled live ended, with `remaining` seconds of its time left:
/// ending early, or the engine covering a dead source with black, means the
/// signal was lost.
pub fn live_end(manager: &ChannelManager, result: &ClipResult, remaining: f64) -> AiredEnd {
    match result {
        ClipResult::Fallback { reason } => AiredEnd::cut("signal_lost", Some(reason.clone())),
        ClipResult::Played if remaining >= 1.0 => AiredEnd::cut("signal_lost", None),
        _ => common_end(manager, result),
    }
}

fn common_end(manager: &ChannelManager, result: &ClipResult) -> AiredEnd {
    match result {
        ClipResult::Played => AiredEnd::Completed,
        ClipResult::Skipped if manager.is_alive.load(Ordering::SeqCst) => {
            AiredEnd::cut("skipped", None)
        }
        ClipResult::Skipped | ClipResult::Stopped => AiredEnd::Stopped,
        ClipResult::LiveEnded => AiredEnd::cut("ingest", None),
        ClipResult::Fallback { reason } => AiredEnd::Failed(reason.clone()),
    }
}

/// Record live ingest taking over: the item it cuts ends, an ingest row opens
/// until the live input ends.
pub async fn watch_ingest(
    manager: ChannelManager,
    control: PlaybackControl,
    cancel: CancellationToken,
) {
    let mut on_air: Option<Option<i64>> = None;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(INGEST_POLL) => {}
        }

        let (active, listener_id) = control.live_status();

        if active && on_air.is_none() {
            let name = listener_name(&*manager.config.read().await, listener_id);
            let entry = NewAsRunEntry {
                source: name.clone(),
                title: Some(name.clone()),
                kind: "ingest",
                origin: "ingest",
                ..NewAsRunEntry::default()
            };
            let cut_by = CutBy {
                reason: Some("ingest"),
                detail: Some(&name),
            };

            info!(channel = manager.id; "Live ingest <span class=\"log-addr\">{name}</span> on air");
            on_air = Some(insert(&manager, &entry, cut_by).await);
        } else if !active && let Some(id) = on_air.take() {
            record_end(&manager, id, AiredEnd::Completed).await;
        }
    }

    if let Some(id) = on_air {
        record_end(&manager, id, AiredEnd::Stopped).await;
    }
}

/// The name a listener is known by in the report: its own name, else its
/// protocol. The address is left out, as it can hold a stream key.
fn listener_name(config: &PlayoutConfig, listener_id: Option<i32>) -> String {
    let listener = config
        .ingest
        .listeners
        .iter()
        .find(|listener| Some(listener.id) == listener_id);

    match listener {
        Some(listener) if !listener.name.trim().is_empty() => listener.name.trim().to_string(),
        Some(listener) => format!("{} ingest", listener.backend.to_uppercase()),
        None => "Live ingest".to_string(),
    }
}

async fn insert(manager: &ChannelManager, entry: &NewAsRunEntry, cut_by: CutBy<'_>) -> Option<i64> {
    match handles::insert_as_run(&manager.db_pool, manager.id, entry, now_millis(), cut_by).await {
        Ok(id) => Some(id),
        Err(error) => {
            error!(channel = manager.id; "Unable to record the item on air: {error}");
            None
        }
    }
}

fn now_millis() -> i64 {
    time_now(&None).timestamp_millis()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::utils::config::LiveInput;

    use super::*;

    fn config() -> PlayoutConfig {
        let mut config = PlayoutConfig::default();

        config.channel.storage = PathBuf::from("/data/media");
        config
    }

    fn event(title: &str, source: &str) -> ScheduleEvent {
        ScheduleEvent {
            id: 7,
            title: title.to_string(),
            sources: vec![source.to_string()],
            ..ScheduleEvent::default()
        }
    }

    #[test]
    fn programme_clips_are_stored_by_storage_path_and_kind() {
        let config = config();
        let clip = Media {
            source: "/data/media/Jornal/ep1.mp4".to_string(),
            title: Some("Jornal".to_string()),
            seek: 10.0,
            out: 70.0,
            ..Media::default()
        };
        let commercial = Media {
            source: "/data/media/intervalo/A.mp4".to_string(),
            ad: true,
            out: 30.0,
            ..Media::default()
        };
        let filler = Media {
            source: "/data/filler/black.mp4".to_string(),
            is_placeholder: true,
            replaces: Some("/data/media/Cinema/missing.mp4".to_string()),
            out: 90.0,
            ..Media::default()
        };

        let entry = programme_entry(&config, &clip);
        assert_eq!(entry.source, "Jornal/ep1.mp4");
        assert_eq!(entry.kind, "program");
        assert_eq!(entry.origin, "playlist");
        assert_eq!(entry.planned_seconds, 60.0);
        assert_eq!(programme_entry(&config, &commercial).kind, "commercial");

        let entry = programme_entry(&config, &filler);
        assert_eq!(entry.kind, "filler");
        assert_eq!(entry.source, "/data/filler/black.mp4");
        assert_eq!(entry.replaces.as_deref(), Some("Cinema/missing.mp4"));
    }

    #[test]
    fn events_without_a_title_are_named_after_their_source() {
        assert_eq!(event_name(&event("Break", "intervalo/A.mp4")), "Break");
        assert_eq!(
            event_name(&event("", "Lives/Plantão/signal.live")),
            "Plantão"
        );
        assert_eq!(event_name(&event(" ", "intervalo/A.mp4")), "A");
    }

    #[test]
    fn a_waiting_screen_names_the_live_it_stands_in_for() {
        let entry = waiting_entry(
            &config(),
            &event("", "Lives/Plantão/signal.live"),
            Some(1_000),
            "",
        );

        assert_eq!(entry.kind, "filler");
        assert_eq!(entry.origin, "schedule");
        assert_eq!(entry.event_id, Some(7));
        assert_eq!(entry.event_title.as_deref(), Some("Plantão"));
        assert_eq!(entry.replaces.as_deref(), Some("Lives/Plantão/signal.live"));
    }

    #[test]
    fn errors_are_kept_short() {
        let (status, reason, detail) = AiredEnd::Failed("x".repeat(1_000)).columns();

        assert_eq!(status, "failed");
        assert_eq!(reason, Some("error"));
        assert_eq!(detail.map(|detail| detail.len()), Some(MAX_DETAIL_CHARS));
    }

    #[test]
    fn listeners_are_named_without_their_address() {
        let mut config = config();

        config.ingest.listeners = vec![
            LiveInput {
                id: 1,
                name: "Estúdio".to_string(),
                backend: "srt".to_string(),
                identifier: "srt://0.0.0.0:9000?streamid=secret".to_string(),
                ..Default::default()
            },
            LiveInput {
                id: 2,
                backend: "rtmp".to_string(),
                identifier: "rtmp://0.0.0.0:1936/live/key".to_string(),
                ..Default::default()
            },
        ];

        assert_eq!(listener_name(&config, Some(1)), "Estúdio");
        assert_eq!(listener_name(&config, Some(2)), "RTMP ingest");
        assert_eq!(listener_name(&config, None), "Live ingest");
    }
}
