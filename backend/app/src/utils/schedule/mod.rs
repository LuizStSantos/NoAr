//! Scheduled events: breaks, clips and live sources placed at a time over the
//! running programme, once or on chosen weekdays.
//!
//! An event either waits for the clip on air to end ([`EntryMode::AfterClip`])
//! or cuts it at the exact time ([`EntryMode::Interrupt`]). Afterwards the
//! programme continues where it stopped ([`AfterMode::Continue`], so the rest
//! of the day runs later) or jumps to what the playlist has at that time
//! ([`AfterMode::Clock`]). The player side lives in `player::schedule`.

use std::path::Path;

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};
use tokio::task;

pub mod live;

use crate::{
    db::models::{AfterMode, EntryMode, ScheduleEvent, ScheduleKind, ScheduleRun},
    file::norm_abs_path,
    utils::{commercial::CommercialFolders, config::PlayoutConfig, errors::ServiceError},
};

/// Longest a clip event may run, a guard against picking the wrong files.
pub const MAX_MEDIA_EVENT_SECONDS: f64 = 6.0 * 3600.0;

/// Longest a live event may run.
pub const MAX_LIVE_EVENT_SECONDS: i64 = 24 * 3600;

/// Most clips one event may hold.
const MAX_EVENT_SOURCES: usize = 50;

/// One occurrence of an event, in the channel's local time.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Occurrence {
    pub event: ScheduleEvent,
    /// The day the occurrence belongs to.
    pub date: NaiveDate,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

impl Occurrence {
    pub fn date_key(&self) -> String {
        self.date.format("%Y-%m-%d").to_string()
    }
}

/// Whether an edit moved the event in time, so occurrences that already
/// played (or failed) should be allowed to play again at the new time.
pub fn timing_changed(before: &ScheduleEvent, after: &ScheduleEvent) -> bool {
    before.kind != after.kind
        || before.start_date != after.start_date
        || before.start_time != after.start_time
        || before.end_time != after.end_time
        || before.repeat_days != after.repeat_days
        || (!before.enabled && after.enabled)
}

pub fn parse_date(value: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| format!("invalid date '{value}', expected YYYY-MM-DD"))
}

pub fn parse_time(value: &str) -> Result<NaiveTime, String> {
    let value = value.trim();

    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| format!("invalid time '{value}', expected HH:MM:SS"))
}

fn weekday_bit(date: NaiveDate) -> u8 {
    1 << date.weekday().num_days_from_sunday()
}

impl ScheduleEvent {
    pub fn occurs_on(&self, date: NaiveDate) -> bool {
        let Ok(first) = parse_date(&self.start_date) else {
            return false;
        };

        if self.repeat_days == 0 {
            date == first
        } else {
            date >= first && self.repeat_days & weekday_bit(date) != 0
        }
    }

    /// The occurrence on `date`, if the event is enabled and runs that day.
    pub fn occurrence_on(&self, date: NaiveDate) -> Option<Occurrence> {
        if !self.enabled || !self.occurs_on(date) {
            return None;
        }

        let start = date.and_time(parse_time(&self.start_time).ok()?);
        let end = match self.kind {
            ScheduleKind::Live => {
                let mut end = date.and_time(parse_time(self.end_time.as_deref()?).ok()?);

                if end <= start {
                    end += TimeDelta::try_days(1)?;
                }

                end
            }
            ScheduleKind::Media => start + seconds(self.duration),
        };

        Some(Occurrence {
            event: self.clone(),
            date,
            start,
            end,
        })
    }
}

/// Occurrences overlapping `[from, to)`, ordered by start time.
pub fn occurrences_between(
    events: &[ScheduleEvent],
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Vec<Occurrence> {
    let mut occurrences = vec![];
    // A live event that started the day before can still be running.
    let mut date = from.date() - TimeDelta::try_days(1).unwrap_or_default();

    while date <= to.date() {
        occurrences.extend(
            events
                .iter()
                .filter_map(|event| event.occurrence_on(date))
                .filter(|occurrence| occurrence.end > from && occurrence.start < to),
        );

        let Some(next) = date.succ_opt() else {
            break;
        };

        date = next;
    }

    occurrences.sort_by(|a, b| a.start.cmp(&b.start).then(a.event.id.cmp(&b.event.id)));

    occurrences
}

/// The event's title, or the name of its live folder or first clip when it
/// has none.
pub fn event_name(event: &ScheduleEvent) -> String {
    if !event.title.trim().is_empty() {
        return event.title.clone();
    }

    let Some(source) = event.sources.first() else {
        return String::new();
    };
    let path = Path::new(source);
    let name = if source.ends_with(".live") {
        path.parent().and_then(Path::file_name)
    } else {
        path.file_stem()
    };

    name.map_or_else(|| source.clone(), |name| name.to_string_lossy().to_string())
}

pub fn seconds(value: f64) -> TimeDelta {
    TimeDelta::try_milliseconds((value.max(0.0) * 1000.0) as i64).unwrap_or_default()
}

/// An occurrence as shown in the web interface, with its run state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OccurrenceView {
    pub event: ScheduleEvent,
    /// Local start and end, `YYYY-MM-DDTHH:MM:SS`.
    pub start: String,
    pub end: String,
    /// `running`, `done`, `skipped` or `failed` once the occurrence aired.
    pub status: Option<String>,
}

/// Occurrences overlapping `[from, to)` with the state of their runs.
pub fn occurrence_views(
    events: &[ScheduleEvent],
    runs: &[ScheduleRun],
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Vec<OccurrenceView> {
    occurrences_between(events, from, to)
        .into_iter()
        .map(|occurrence| {
            let date = occurrence.date_key();
            let status = runs
                .iter()
                .find(|run| run.event_id == occurrence.event.id && run.occurrence == date)
                .map(|run| run.status.clone());

            OccurrenceView::new(occurrence, status)
        })
        .collect()
}

impl OccurrenceView {
    pub fn new(occurrence: Occurrence, status: Option<String>) -> Self {
        Self {
            start: occurrence.start.format("%Y-%m-%dT%H:%M:%S").to_string(),
            end: occurrence.end.format("%Y-%m-%dT%H:%M:%S").to_string(),
            event: occurrence.event,
            status,
        }
    }
}

/// An occurrence that starts now, for "play now": clips run their length, a
/// live runs as long as its usual slot.
pub fn occurrence_now(event: &ScheduleEvent, now: NaiveDateTime) -> Option<Occurrence> {
    let length = match event.kind {
        ScheduleKind::Media => seconds(event.duration),
        ScheduleKind::Live => {
            let start = parse_time(&event.start_time).ok()?;
            let end = parse_time(event.end_time.as_deref()?).ok()?;
            let mut length = end - start;

            if length <= TimeDelta::zero() {
                length += TimeDelta::try_days(1)?;
            }

            length
        }
    };

    Some(Occurrence {
        event: event.clone(),
        date: now.date(),
        start: now,
        end: now + length,
    })
}

/// Event as sent by the web interface.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleEventInput {
    pub kind: ScheduleKind,
    #[serde(default)]
    pub title: String,
    pub sources: Vec<String>,
    pub start_date: String,
    pub start_time: String,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub repeat_days: u8,
    #[serde(default)]
    pub entry_mode: EntryMode,
    #[serde(default)]
    pub after_mode: AfterMode,
    #[serde(default)]
    pub show_in_guide: bool,
    #[serde(default = "default_retry_seconds")]
    pub retry_seconds: u32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_retry_seconds() -> u32 {
    120
}

fn default_enabled() -> bool {
    true
}

/// Check an event from the web interface against the channel's storage and
/// turn it into a row: clip paths become storage-relative and their total
/// length is measured.
pub async fn prepare_event(
    config: &PlayoutConfig,
    id: i32,
    input: ScheduleEventInput,
) -> Result<ScheduleEvent, ServiceError> {
    let bad_request = ServiceError::BadRequest;
    let start_date = parse_date(&input.start_date).map_err(bad_request)?;
    let start_time = parse_time(&input.start_time).map_err(bad_request)?;

    if input.repeat_days > 127 {
        return Err(bad_request("repeat_days must be between 0 and 127".into()));
    }

    if input.sources.is_empty() || input.sources.len() > MAX_EVENT_SOURCES {
        return Err(bad_request(format!(
            "an event needs between 1 and {MAX_EVENT_SOURCES} sources"
        )));
    }

    let storage = &config.channel.storage;
    let mut sources = Vec::with_capacity(input.sources.len());

    for source in &input.sources {
        let (path, _, relative) = norm_abs_path(storage, source)?;

        if !path.is_file() {
            return Err(bad_request(format!(
                "'{source}' is not a file in the storage"
            )));
        }

        sources.push((path, relative));
    }

    let (duration, end_time) = match input.kind {
        ScheduleKind::Media => {
            let mut total = 0.0;

            for (path, relative) in &sources {
                total += clip_duration(path).await.ok_or_else(|| {
                    bad_request(format!("cannot read the length of '{relative}'"))
                })?;
            }

            if total > MAX_MEDIA_EVENT_SECONDS {
                return Err(bad_request("the clips are longer than 6 hours".into()));
            }

            (total, None)
        }
        ScheduleKind::Live => {
            let [(path, _)] = sources.as_slice() else {
                return Err(bad_request("a live event takes one .live file".into()));
            };

            live::read_live_url(path).await?;

            let end = input
                .end_time
                .as_deref()
                .ok_or_else(|| bad_request("a live event needs an end time".into()))
                .and_then(|end| parse_time(end).map_err(bad_request))?;
            let mut length = end - start_time;

            if length <= TimeDelta::zero() {
                length += TimeDelta::try_days(1).unwrap_or_default();
            }

            if length.num_seconds() > MAX_LIVE_EVENT_SECONDS {
                return Err(bad_request("a live event can run 24 hours at most".into()));
            }

            (0.0, Some(end.format("%H:%M:%S").to_string()))
        }
    };

    let relative_sources: Vec<String> = sources.into_iter().map(|(_, rel)| rel).collect();
    let mut commercials = CommercialFolders::new(storage);
    // Commercials never show in the guide.
    let all_commercials = input.kind == ScheduleKind::Media
        && relative_sources
            .iter()
            .all(|source| commercials.contains(source));
    let title = match input.title.trim() {
        "" => default_title(input.kind, &relative_sources),
        title => title.to_string(),
    };

    Ok(ScheduleEvent {
        id,
        channel_id: config.general.channel_id,
        kind: input.kind,
        title,
        sources: relative_sources,
        duration,
        start_date: start_date.format("%Y-%m-%d").to_string(),
        start_time: start_time.format("%H:%M:%S").to_string(),
        end_time,
        repeat_days: input.repeat_days,
        entry_mode: input.entry_mode,
        after_mode: input.after_mode,
        show_in_guide: input.show_in_guide && !all_commercials,
        retry_seconds: input.retry_seconds.min(3600),
        enabled: input.enabled,
    })
}

/// Length of a clip in seconds, or `None` when it cannot be read.
pub async fn clip_duration(path: &Path) -> Option<f64> {
    let path = path.to_string_lossy().to_string();
    let probe = task::spawn_blocking(move || noar_engine::probe_media(&path))
        .await
        .ok()?
        .ok()?;

    probe
        .format
        .duration
        .or_else(|| probe.video.iter().find_map(|stream| stream.duration))
        .filter(|duration| duration.is_finite() && *duration > 0.0)
}

/// A readable title from the first source: the live's folder name, or the
/// clip's file name.
fn default_title(kind: ScheduleKind, sources: &[String]) -> String {
    let Some(first) = sources.first().map(Path::new) else {
        return String::new();
    };
    let name = match kind {
        ScheduleKind::Live => first.parent().and_then(Path::file_name),
        ScheduleKind::Media => first.file_stem(),
    }
    .or_else(|| first.file_stem())
    .map(|name| name.to_string_lossy().replace('_', " "))
    .unwrap_or_default();
    let extra = sources.len().saturating_sub(1);

    match kind {
        ScheduleKind::Media if extra > 0 => format!("{name} +{extra}"),
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: ScheduleKind) -> ScheduleEvent {
        ScheduleEvent {
            id: 1,
            kind,
            start_date: "2026-10-12".to_string(),
            start_time: "13:00:00".to_string(),
            duration: 210.0,
            enabled: true,
            ..ScheduleEvent::default()
        }
    }

    fn date(value: &str) -> NaiveDate {
        parse_date(value).unwrap()
    }

    fn at(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%F %T").unwrap()
    }

    #[test]
    fn only_a_change_in_time_counts_as_moving_the_event() {
        let before = event(ScheduleKind::Live);
        let mut renamed = before.clone();
        renamed.title = "Jogo".to_string();
        renamed.entry_mode = EntryMode::Interrupt;

        assert!(!timing_changed(&before, &renamed));

        let mut later = before.clone();
        later.start_time = "14:00:00".to_string();

        assert!(timing_changed(&before, &later));

        let mut longer = before.clone();
        longer.end_time = Some("15:00:00".to_string());

        assert!(timing_changed(&before, &longer));

        let mut disabled = before.clone();
        disabled.enabled = false;

        assert!(!timing_changed(&before, &disabled));
        assert!(timing_changed(&disabled, &before));
    }

    #[test]
    fn single_events_run_once_and_repeating_ones_on_their_weekdays() {
        let once = event(ScheduleKind::Media);
        let mut weekdays = event(ScheduleKind::Media);
        // Monday to Friday.
        weekdays.repeat_days = 0b0011_1110;

        assert!(once.occurs_on(date("2026-10-12")));
        assert!(!once.occurs_on(date("2026-10-13")));
        // 2026-10-12 is a Monday; Saturday the 17th is off, Monday the 19th on.
        assert!(weekdays.occurs_on(date("2026-10-16")));
        assert!(!weekdays.occurs_on(date("2026-10-17")));
        assert!(weekdays.occurs_on(date("2026-10-19")));
        assert!(!weekdays.occurs_on(date("2026-10-09")));
    }

    #[test]
    fn occurrences_use_clip_length_or_live_end_and_cross_midnight() {
        let media = event(ScheduleKind::Media)
            .occurrence_on(date("2026-10-12"))
            .unwrap();
        let mut live = event(ScheduleKind::Live);
        live.start_time = "23:30:00".to_string();
        live.end_time = Some("00:30:00".to_string());
        let live = live.occurrence_on(date("2026-10-12")).unwrap();

        assert_eq!(media.start, at("2026-10-12 13:00:00"));
        assert_eq!(media.end, at("2026-10-12 13:03:30"));
        assert_eq!(live.end, at("2026-10-13 00:30:00"));
    }

    #[test]
    fn occurrences_between_include_running_lives_from_the_day_before() {
        let mut late_live = event(ScheduleKind::Live);
        late_live.id = 2;
        late_live.repeat_days = 127;
        late_live.start_time = "23:00:00".to_string();
        late_live.end_time = Some("01:00:00".to_string());
        let mut disabled = event(ScheduleKind::Media);
        disabled.enabled = false;
        let events = [event(ScheduleKind::Media), late_live, disabled];

        let found = occurrences_between(
            &events,
            at("2026-10-13 00:00:00"),
            at("2026-10-14 00:00:00"),
        );
        let starts: Vec<_> = found.iter().map(|o| (o.event.id, o.start)).collect();

        assert_eq!(
            starts,
            [
                (2, at("2026-10-12 23:00:00")),
                (2, at("2026-10-13 23:00:00")),
            ]
        );
    }

    #[test]
    fn default_titles_come_from_the_sources() {
        assert_eq!(
            default_title(ScheduleKind::Live, &["Lives/Big Match/signal.live".into()]),
            "Big Match"
        );
        assert_eq!(
            default_title(
                ScheduleKind::Media,
                &["Breaks/break_a.mp4".into(), "Breaks/b.mp4".into()]
            ),
            "break a +1"
        );
    }
}
