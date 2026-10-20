//! The report of a channel's playlist days: what aired, from the as-run
//! records (see `player::as_run`), and what is still to come, from the
//! playlists and the Scheduler. Times before the channel's first record only
//! have the plan, marked as unrecorded.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeDelta};
use chrono_tz::Tz;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::{
    db::{
        handles,
        models::{AsRunEntry, ScheduleEvent, ScheduleKind, ScheduleRun},
    },
    player::utils::is_live_source,
    utils::{
        commercial::CommercialFolders,
        config::{PlayoutConfig, ProcessMode},
        errors::ServiceError,
        paths::storage_relative,
        playlist::read_playlist,
        program::{channel_datetime, playlist_day_start},
        schedule::{Occurrence, clip_duration, event_name, occurrences_between, parse_date},
        time_machine::time_now,
        xmltv::{PlannedSlot, clip_titles, filler_path, library::NfoLibrary, planned_slots},
    },
};

/// Longest report, in days.
pub const MAX_DAYS: i64 = 62;

/// A stretch longer than this without any record shows as a gap.
const GAP_MILLIS: i64 = 60_000;

/// Days of the report, both included; `to` defaults to `from`.
#[derive(Debug, Clone, Deserialize)]
pub struct ReportQuery {
    pub from: String,
    #[serde(default)]
    pub to: Option<String>,
}

impl ReportQuery {
    pub fn dates(&self) -> Result<(NaiveDate, NaiveDate), ServiceError> {
        let from = parse_date(&self.from).map_err(ServiceError::BadRequest)?;
        let to = match self.to.as_deref() {
            Some(to) if !to.trim().is_empty() => {
                parse_date(to).map_err(ServiceError::BadRequest)?
            }
            _ => from,
        };
        let days = (to - from).num_days() + 1;

        if days < 1 {
            return Err(ServiceError::BadRequest(
                "the last day comes before the first".to_string(),
            ));
        }

        if days > MAX_DAYS {
            return Err(ServiceError::BadRequest(format!(
                "a report covers at most {MAX_DAYS} days"
            )));
        }

        Ok((from, to))
    }
}

/// What the player is doing, which moves what is still to come.
#[derive(Debug, Clone, Default)]
pub struct PlayoutState {
    pub running: bool,
    /// Playlist day the player is on, `YYYY-MM-DD`.
    pub date: String,
    /// Seconds the playlist runs ahead of the clock (behind when negative).
    pub time_shift: f64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub timezone: String,
    /// When each playlist day starts, `HH:MM:SS`.
    pub day_start: String,
    /// Unix milliseconds: items before it aired, items after it are planned.
    pub now: i64,
    /// When the channel's first recorded item went on air, Unix milliseconds.
    /// Before it there is no record of what aired, only the plan.
    pub recorded_since: Option<i64>,
    pub days: Vec<ReportDay>,
    pub items: Vec<ReportItem>,
}

#[derive(Debug, Serialize)]
pub struct ReportDay {
    /// `YYYY-MM-DD`
    pub date: String,
    pub start: i64,
    pub end: i64,
    /// The day has a playlist; without one its plan only has scheduled events.
    pub playlist: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReportItem {
    /// Playlist day, `YYYY-MM-DD`.
    pub day: String,
    /// Unix milliseconds.
    pub start: i64,
    /// Unix milliseconds; for the item on air, when it is due to end.
    pub end: i64,
    /// Program title from the `.nfo` files, the playlist or the file name.
    /// Empty for filler and gaps, which the interface names.
    pub title: String,
    pub sub_title: Option<String>,
    /// Storage path, a listener name for ingest, or empty.
    pub source: String,
    /// Storage folder of the program the clip belongs to, when a `.nfo`
    /// describes it.
    pub program: Option<String>,
    /// `program`, `commercial`, `live`, `filler`, `ingest`, or `gap`: a
    /// stretch without any record (the playout was stopped).
    pub kind: String,
    /// `playlist`, `folder`, `schedule` or `ingest`; empty for gaps.
    pub origin: String,
    pub event_id: Option<i64>,
    pub event_title: Option<String>,
    /// When the scheduled event was due, Unix milliseconds.
    pub scheduled_at: Option<i64>,
    /// `aired`, `on_air`, `planned`, or `unrecorded`: the plan of a time
    /// that has no record.
    pub state: &'static str,
    /// How an aired item ended: `completed`, `cut`, `failed` or `stopped`.
    pub status: Option<String>,
    /// Why it ended early, see `player::as_run::AiredEnd`.
    pub reason: Option<String>,
    pub detail: Option<String>,
    pub planned_seconds: f64,
    /// What a filler stood in for.
    pub replaces: Option<String>,
}

/// Build the report of the playlist days `from` to `to`, both included.
pub async fn build_report(
    config: &PlayoutConfig,
    pool: &SqlitePool,
    events: &[ScheduleEvent],
    playout: &PlayoutState,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Report, ServiceError> {
    let channel_id = config.general.channel_id;
    let now = time_now(&config.channel.timezone);
    let now_millis = now.timestamp_millis();
    let start_sec = config.playlist.start_sec.ok_or_else(|| {
        ServiceError::Conflict("Playlist start time has not been initialized".to_string())
    })?;
    let clock = Clock {
        timezone: now.timezone(),
        start_sec,
    };
    let days = report_days(config, &clock, from, to).await?;
    let (Some(first_day), Some(last_day)) = (days.first(), days.last()) else {
        return Err(ServiceError::BadRequest("no days to report".to_string()));
    };
    let range = (first_day.start, last_day.end);
    let recorded_since = handles::select_first_as_run_start(pool, channel_id).await?;
    let mut describer = Describer::new(config);
    let mut items = vec![];

    // What aired.
    let mut aired = vec![];

    for row in handles::select_as_run(pool, channel_id, range.0, range.1).await? {
        if row.started_at >= range.0 {
            aired.push(describer.aired(row));
        }
    }

    for item in &mut aired {
        describer.describe(item).await;
    }

    if let Some(recorded_since) = recorded_since {
        let recorded = (recorded_since.max(range.0), now_millis.min(range.1));

        items.extend(gaps(&aired, recorded));
    }

    let on_air_until = aired
        .iter()
        .filter(|item| item.state == "on_air")
        .map(|item| item.end)
        .max();

    items.extend(aired);

    // Before the first record: the plan, as it was.
    let unrecorded_end = recorded_since
        .unwrap_or(now_millis)
        .min(now_millis)
        .min(range.1);

    if unrecorded_end > range.0 {
        let window = (clock.local(range.0)?, clock.local(unrecorded_end)?);
        let occurrences = occurrences_between(events, window.0, window.1);
        let plan = Plan {
            occurrences: &occurrences,
            window,
            shift: None,
            state: "unrecorded",
        };

        items.extend(
            plan.items(config, &clock, &mut describer)
                .await?
                .into_iter()
                .filter(|item| item.start >= range.0 && item.start < unrecorded_end)
                .map(|item| ReportItem {
                    end: item.end.min(unrecorded_end),
                    ..item
                }),
        );
    }

    // What is still to come, from where the item on air ends.
    let planned_from = now_millis.max(range.0);

    if planned_from < range.1 {
        let window = (clock.local(planned_from)?, clock.local(range.1)?);
        let runs = handles::select_schedule_runs(
            pool,
            channel_id,
            &(from - TimeDelta::try_days(1).unwrap_or_default())
                .format("%Y-%m-%d")
                .to_string(),
            &to.format("%Y-%m-%d").to_string(),
        )
        .await?;
        let now_local = now.naive_local();
        // Events on air now still move the programme after them; finished
        // ones already did, through the time shift.
        let occurrences: Vec<Occurrence> = occurrences_between(events, window.0, window.1)
            .into_iter()
            .filter(|occurrence| occurrence.end > now_local && !is_finished(occurrence, &runs))
            .collect();
        let shift = match playout.running && !playout.date.is_empty() {
            true => Some((clock.day_end(config, &playout.date)?, playout.time_shift)),
            false => None,
        };
        let plan = Plan {
            occurrences: &occurrences,
            window,
            shift,
            state: "planned",
        };
        let cutoff = on_air_until.unwrap_or(now_millis).max(now_millis) - 1_000;

        items.extend(
            plan.items(config, &clock, &mut describer)
                .await?
                .into_iter()
                .filter(|item| item.start >= cutoff && item.start < range.1),
        );
    }

    items.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));

    for item in &mut items {
        item.day = clock.day_of(item.start)?;
    }

    Ok(Report {
        timezone: clock.timezone.name().to_string(),
        day_start: config.playlist.day_start.clone(),
        now: now_millis,
        recorded_since,
        days,
        items,
    })
}

/// Times of the channel: its time zone and when its playlist days start.
struct Clock {
    timezone: Tz,
    start_sec: f64,
}

impl Clock {
    fn millis(&self, local: NaiveDateTime) -> Result<i64, ServiceError> {
        Ok(channel_datetime(self.timezone, local)?.timestamp_millis())
    }

    fn local(&self, millis: i64) -> Result<NaiveDateTime, ServiceError> {
        DateTime::from_timestamp_millis(millis)
            .map(|time| time.with_timezone(&self.timezone).naive_local())
            .ok_or_else(|| ServiceError::Conflict(format!("invalid time {millis}")))
    }

    fn day_start(&self, date: NaiveDate) -> Result<NaiveDateTime, ServiceError> {
        playlist_day_start(&date.format("%Y-%m-%d").to_string(), self.start_sec)
    }

    /// End of the playlist day `date`, `YYYY-MM-DD`, as the player has it.
    fn day_end(&self, config: &PlayoutConfig, date: &str) -> Result<NaiveDateTime, ServiceError> {
        let length = config.playlist.length_sec.unwrap_or(86_400.0);

        Ok(playlist_day_start(date, self.start_sec)?
            + TimeDelta::try_milliseconds((length * 1000.0) as i64).unwrap_or_default())
    }

    /// The playlist day a moment belongs to, `YYYY-MM-DD`.
    fn day_of(&self, millis: i64) -> Result<String, ServiceError> {
        let local = self.local(millis)?;
        let date = local.date();
        let date = match local < self.day_start(date)? {
            true => date - TimeDelta::try_days(1).unwrap_or_default(),
            false => date,
        };

        Ok(date.format("%Y-%m-%d").to_string())
    }
}

async fn report_days(
    config: &PlayoutConfig,
    clock: &Clock,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<ReportDay>, ServiceError> {
    let mut days = vec![];
    let mut date = from;

    while date <= to {
        let next = date + TimeDelta::try_days(1).unwrap_or_default();
        let name = date.format("%Y-%m-%d").to_string();

        days.push(ReportDay {
            playlist: read_playlist(config, name.clone()).await.is_ok(),
            date: name,
            start: clock.millis(clock.day_start(date)?)?,
            end: clock.millis(clock.day_start(next)?)?,
        });
        date = next;
    }

    Ok(days)
}

fn is_finished(occurrence: &Occurrence, runs: &[ScheduleRun]) -> bool {
    runs.iter().any(|run| {
        run.event_id == occurrence.event.id
            && run.occurrence == occurrence.date_key()
            && run.status != "running"
    })
}

/// Stretches of `recorded` (Unix milliseconds) longer than [`GAP_MILLIS`]
/// that no aired item covers.
fn gaps(aired: &[ReportItem], recorded: (i64, i64)) -> Vec<ReportItem> {
    let mut gaps = vec![];
    let mut covered_until = recorded.0;
    let mut gap = |start: i64, end: i64| {
        if end - start > GAP_MILLIS {
            gaps.push(ReportItem {
                start,
                end,
                kind: "gap".to_string(),
                state: "aired",
                ..ReportItem::default()
            });
        }
    };

    for item in aired {
        if item.start >= recorded.1 {
            break;
        }

        gap(covered_until, item.start);
        covered_until = covered_until.max(item.end);
    }

    gap(covered_until, recorded.1);

    gaps
}

/// Part of the plan to list.
struct Plan<'a> {
    occurrences: &'a [Occurrence],
    /// Channel-local start and end.
    window: (NaiveDateTime, NaiveDateTime),
    /// The player's time shift and the end of the playlist day it applies to.
    shift: Option<(NaiveDateTime, f64)>,
    state: &'static str,
}

impl Plan<'_> {
    async fn items(
        &self,
        config: &PlayoutConfig,
        clock: &Clock,
        describer: &mut Describer,
    ) -> Result<Vec<ReportItem>, ServiceError> {
        let slots = planned_slots(
            config,
            self.occurrences,
            self.window.0,
            self.window.1,
            self.shift,
        )
        .await?;
        let mut items = vec![];

        for slot in slots {
            let start = clock.millis(slot.start)?;
            let end = clock.millis(slot.stop)?;
            let occurrence = slot
                .occurrence
                .and_then(|index| self.occurrences.get(index));

            match occurrence {
                Some(occurrence) => {
                    let due = clock.millis(occurrence.start)?;

                    items.extend(describer.event(occurrence, due, start, end).await);
                }
                None => items.push(describer.clip(&slot, start, end)),
            }
        }

        for item in &mut items {
            item.state = self.state;
            describer.describe(item).await;
        }

        Ok(items)
    }
}

/// Fills in what the report shows of each item: titles, the program, the
/// kind of a planned clip.
struct Describer {
    storage: PathBuf,
    library: NfoLibrary,
    commercials: CommercialFolders,
    filler: Option<PathBuf>,
    /// Origin of programme clips: the playlist, or the folder in folder mode.
    origin: &'static str,
    durations: HashMap<String, Option<f64>>,
}

impl Describer {
    fn new(config: &PlayoutConfig) -> Self {
        Self {
            storage: config.channel.storage.clean(),
            library: NfoLibrary::new(&config.channel.storage),
            commercials: CommercialFolders::new(&config.channel.storage),
            filler: filler_path(config),
            origin: match config.processing.mode {
                ProcessMode::Folder => "folder",
                ProcessMode::Playlist => "playlist",
            },
            durations: HashMap::new(),
        }
    }

    fn aired(&self, row: AsRunEntry) -> ReportItem {
        let on_air = row.ended_at.is_none();
        let expected_end = row.started_at + (row.planned_seconds * 1000.0) as i64;

        ReportItem {
            start: row.started_at,
            end: row.ended_at.unwrap_or(expected_end.max(row.started_at)),
            title: row.title.unwrap_or_default(),
            source: row.source,
            kind: row.kind,
            origin: row.origin,
            event_id: row.event_id,
            event_title: row.event_title,
            scheduled_at: row.scheduled_at,
            state: if on_air { "on_air" } else { "aired" },
            status: (!on_air).then_some(row.status),
            reason: row.reason,
            detail: row.detail,
            planned_seconds: row.planned_seconds,
            replaces: row.replaces,
            ..ReportItem::default()
        }
    }

    /// A playlist clip of the plan.
    fn clip(&self, slot: &PlannedSlot, start: i64, end: i64) -> ReportItem {
        let source = storage_relative(&self.storage, &slot.source);
        let is_filler = slot.source.is_empty()
            || self
                .filler
                .as_deref()
                .is_some_and(|filler| Path::new(&slot.source).clean().starts_with(filler));
        let kind = if is_filler {
            "filler"
        } else if slot.ad {
            "commercial"
        } else if is_live_source(&slot.source) {
            "live"
        } else {
            "program"
        };

        ReportItem {
            start,
            end,
            title: slot.title.clone().unwrap_or_default(),
            source,
            kind: kind.to_string(),
            origin: self.origin.to_string(),
            planned_seconds: (end - start) as f64 / 1000.0,
            ..ReportItem::default()
        }
    }

    /// A scheduled event of the plan, `[start, end)`, clip by clip.
    async fn event(
        &mut self,
        occurrence: &Occurrence,
        due: i64,
        start: i64,
        end: i64,
    ) -> Vec<ReportItem> {
        let event = &occurrence.event;
        let base = ReportItem {
            origin: "schedule".to_string(),
            event_id: Some(i64::from(event.id)),
            event_title: Some(event_name(event)),
            scheduled_at: Some(due),
            ..ReportItem::default()
        };

        if event.kind == ScheduleKind::Live {
            return vec![ReportItem {
                start,
                end,
                source: event.sources.first().cloned().unwrap_or_default(),
                kind: "live".to_string(),
                planned_seconds: (end - start) as f64 / 1000.0,
                ..base
            }];
        }

        let mut items = vec![];
        let mut clip_start = start;

        for source in &event.sources {
            if clip_start >= end {
                break;
            }

            let Some(length) = self.duration(source).await else {
                continue;
            };
            let clip_end = (clip_start + (length * 1000.0) as i64).min(end);
            let kind = match self.commercials.contains(source) {
                true => "commercial",
                false => "program",
            };

            items.push(ReportItem {
                start: clip_start,
                end: clip_end,
                source: source.clone(),
                kind: kind.to_string(),
                planned_seconds: (clip_end - clip_start) as f64 / 1000.0,
                ..base.clone()
            });
            clip_start = clip_end;
        }

        items
    }

    async fn duration(&mut self, source: &str) -> Option<f64> {
        if let Some(duration) = self.durations.get(source) {
            return *duration;
        }

        let duration = clip_duration(&self.storage.join(source)).await;

        self.durations.insert(source.to_string(), duration);

        duration
    }

    /// Titles and program of a clip or live, from its `.nfo` files.
    async fn describe(&mut self, item: &mut ReportItem) {
        if !matches!(item.kind.as_str(), "program" | "commercial" | "live") {
            return;
        }

        let clip = self.library.lookup(&item.source).await;
        // A live without a sheet goes by its event's name.
        let fallback = match item.kind.as_str() {
            "live" => item.event_title.as_deref().unwrap_or_default(),
            _ => "",
        };
        let playlist_title = [item.title.trim(), fallback.trim()]
            .into_iter()
            .find(|title| !title.is_empty())
            .map(str::to_string);
        let (title, sub_title) = clip_titles(&clip, &item.source, playlist_title.as_deref());

        item.title = title;
        item.sub_title = sub_title;
        item.program = clip
            .program
            .is_some()
            .then(|| storage_relative(&self.storage, &clip.program_key.to_string_lossy()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aired(start: i64, end: i64) -> ReportItem {
        ReportItem {
            start,
            end,
            state: "aired",
            ..ReportItem::default()
        }
    }

    #[test]
    fn stretches_without_a_record_are_gaps() {
        let items = [
            aired(0, 10_000),
            aired(100_000, 200_000),
            aired(200_500, 210_000),
        ];
        let gaps = gaps(&items, (0, 400_000));

        assert_eq!(
            gaps.iter()
                .map(|gap| (gap.start, gap.end))
                .collect::<Vec<_>>(),
            [(10_000, 100_000), (210_000, 400_000)]
        );
        assert!(gaps.iter().all(|gap| gap.kind == "gap"));
    }

    #[test]
    fn the_item_on_air_leaves_no_gap_up_to_now() {
        let items = [aired(0, 50_000), aired(50_000, 900_000)];

        assert!(gaps(&items, (0, 300_000)).is_empty());
    }

    #[test]
    fn a_day_runs_from_its_playlist_start() {
        let clock = Clock {
            timezone: Tz::America__Sao_Paulo,
            start_sec: 6.0 * 3600.0,
        };
        let at = |local: &str| {
            clock
                .millis(NaiveDateTime::parse_from_str(local, "%F %T").unwrap())
                .unwrap()
        };

        assert_eq!(
            clock.day_of(at("2026-10-20 05:59:59")).unwrap(),
            "2026-10-19"
        );
        assert_eq!(
            clock.day_of(at("2026-10-20 06:00:00")).unwrap(),
            "2026-10-20"
        );
        assert_eq!(
            clock.day_of(at("2026-10-20 23:30:00")).unwrap(),
            "2026-10-20"
        );
    }

    #[test]
    fn a_report_covers_whole_days_up_to_a_limit() {
        let query = |from: &str, to: Option<&str>| ReportQuery {
            from: from.to_string(),
            to: to.map(str::to_string),
        };

        assert!(query("2026-10-01", Some("2026-10-31")).dates().is_ok());
        assert_eq!(
            query("2026-10-20", None).dates().unwrap(),
            (
                NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
            )
        );
        assert!(query("2026-10-20", Some("2026-10-19")).dates().is_err());
        assert!(query("2026-01-01", Some("2026-12-31")).dates().is_err());
        assert!(query("20/10/2026", None).dates().is_err());
    }
}
