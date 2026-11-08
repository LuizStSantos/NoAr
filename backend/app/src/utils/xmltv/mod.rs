//! XMLTV guide built from the channel's playlists and the `.nfo` files next to
//! the media. See [`library`] for the expected folder layout.
//!
//! Consecutive clips of the same program are merged into one guide entry, and
//! ads and filler extend the entry they interrupt instead of showing up on
//! their own.

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use log::*;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use tokio::fs;

mod document;

pub mod library;
pub mod nfo;
pub mod sheet;

use crate::{
    db::models::{AfterMode, EntryMode, ScheduleEvent, ScheduleKind},
    file::norm_abs_path,
    utils::{
        commercial::CommercialFolders,
        config::PlayoutConfig,
        errors::ServiceError,
        playlist::read_playlist,
        program::{channel_datetime, playlist_day_start},
        schedule::{Occurrence, occurrences_between, seconds},
        time_machine::time_now,
    },
};
use document::{ImageUrls, XmlWriter, write_programme};
#[cfg(test)]
use document::{episode_numbers, ratings, xml_safe, xmltv_date};
use library::{ClipMetadata, Image, NfoLibrary, has_image_extension};
#[cfg(test)]
use nfo::Nfo;

/// Days in the guide when the request does not say.
pub const DEFAULT_DAYS: u32 = 7;

/// Longest guide that can be requested, in days.
pub const MAX_DAYS: u32 = 14;

pub struct XmltvChannel {
    /// `<channel id>`, which must match the `tvg-id` used in the M3U list.
    pub id: String,
    pub name: String,
}

pub struct XmltvOptions {
    pub days: u32,
    /// Language code for titles and descriptions, e.g. `pt`.
    pub lang: Option<String>,
    /// URL that serves storage files under their path relative to the
    /// storage root, without a trailing slash.
    pub image_base_url: String,
}

/// One playlist entry placed on the channel's local timeline.
#[derive(Debug, Clone, PartialEq)]
struct Slot {
    start: NaiveDateTime,
    stop: NaiveDateTime,
    /// End of the playlist day the slot belongs to.
    day_end: NaiveDateTime,
    source: String,
    title: Option<String>,
    /// Ads, and scheduled events kept out of the guide.
    ad: bool,
    /// Placed by a scheduled event rather than the playlist.
    scheduled: bool,
    /// Index of the event's occurrence in the list given to [`apply_schedule`].
    occurrence: Option<usize>,
}

/// One entry of the guide; may cover several consecutive slots.
struct Programme {
    start: NaiveDateTime,
    stop: NaiveDateTime,
    source: String,
    playlist_title: Option<String>,
    clip: ClipMetadata,
    /// Placed by a scheduled event.
    scheduled: bool,
}

impl Programme {
    /// Guide title and sub-title, from the `.nfo` files, the playlist title
    /// or the file name.
    fn titles(&self) -> (String, Option<String>) {
        clip_titles(&self.clip, &self.source, self.playlist_title.as_deref())
    }

    fn is_same_programme(&self, clip: &ClipMetadata, playlist_title: &Option<String>) -> bool {
        self.clip.program_key == clip.program_key
            && self.clip.episode_key == clip.episode_key
            && &self.playlist_title == playlist_title
    }
}

/// Guide title and sub-title of a clip, from the `.nfo` files, the playlist
/// title or the file name.
pub(crate) fn clip_titles(
    clip: &ClipMetadata,
    source: &str,
    playlist_title: Option<&str>,
) -> (String, Option<String>) {
    let program = clip.program.as_deref();
    let episode = clip.episode.as_deref();
    let title = program
        .and_then(|nfo| nfo.title.as_deref())
        .or_else(|| episode.and_then(|nfo| nfo.show_title.as_deref()))
        .or(playlist_title)
        .or_else(|| episode.and_then(|nfo| nfo.title.as_deref()))
        .map(str::to_string)
        .unwrap_or_else(|| title_from_file_name(source));
    let sub_title = episode
        .and_then(|nfo| nfo.title.as_deref())
        .or(program.and(playlist_title))
        .filter(|sub_title| *sub_title != title)
        .map(str::to_string);

    (title, sub_title)
}

/// Build the XMLTV document for one channel, from the start of the current
/// day for `options.days` days.
pub async fn build_xmltv(
    config: &PlayoutConfig,
    channel: &XmltvChannel,
    events: &[ScheduleEvent],
    options: &XmltvOptions,
) -> Result<String, ServiceError> {
    let now = time_now(&config.channel.timezone);
    let timezone = now.timezone();
    let days = i64::from(options.days.clamp(1, MAX_DAYS));
    let from = now.date_naive().and_time(NaiveTime::MIN);
    let to = from + TimeDelta::try_days(days).unwrap_or_default();

    let programmes = programmes_between(config, events, from, to).await?;
    let images = ImageUrls {
        base: &options.image_base_url,
        storage: config.channel.storage.clean(),
    };
    let mut xml = XmlWriter::new(options.lang.as_deref());

    xml.raw("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.raw("<!DOCTYPE tv SYSTEM \"xmltv.dtd\">\n");
    xml.open(
        0,
        "tv",
        &[
            ("generator-info-name", "NoAr"),
            ("generator-info-url", "https://github.com/LuizStSantos/NoAr"),
        ],
    );

    xml.open(1, "channel", &[("id", &channel.id)]);
    xml.localized(2, "display-name", &[], &channel.name);

    if let Some(icon) = channel_icon(config)
        .await
        .and_then(|logo| images.url(&logo))
    {
        xml.element(2, "icon", &[("src", &icon)], None);
    }

    xml.close(1, "channel");

    for programme in &programmes {
        let times = (
            channel_datetime(timezone, programme.start),
            channel_datetime(timezone, programme.stop),
        );
        let (Ok(start), Ok(stop)) = times else {
            warn!(
                "XMLTV: skip {:?}, its time does not exist in {timezone}",
                programme.source
            );
            continue;
        };

        write_programme(&mut xml, &channel.id, start, stop, programme, &images);
    }

    xml.close(0, "tv");

    Ok(xml.finish())
}

/// The guide's programmes overlapping `[from, to)`: playlists, scheduled
/// events placed the way the player airs them, clips grouped by program.
async fn programmes_between(
    config: &PlayoutConfig,
    events: &[ScheduleEvent],
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Result<Vec<Programme>, ServiceError> {
    let slots = collect_slots(config, from, to).await?;
    let mut occurrences = occurrences_between(events, from, to);
    let mut commercials = CommercialFolders::new(&config.channel.storage);

    // Events made only of commercials stay out of the guide, even when they
    // were saved before their folder was marked.
    for occurrence in &mut occurrences {
        if occurrence.event.kind == ScheduleKind::Media
            && occurrence
                .event
                .sources
                .iter()
                .all(|source| commercials.contains(source))
        {
            occurrence.event.show_in_guide = false;
        }
    }

    let slots = apply_schedule(slots, &occurrences);

    Ok(group_programmes(config, slots).await)
}

/// One entry of the guide, for the scheduler's timeline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuideBlock {
    /// Local `YYYY-MM-DDTHH:MM:SS`.
    pub start: String,
    pub stop: String,
    pub title: String,
    pub sub_title: Option<String>,
    /// Placed by a scheduled event shown in the guide.
    pub scheduled: bool,
}

/// The day's guide as the scheduler shows it: program names from the
/// `.nfo` files and times moved by scheduled events.
pub async fn guide_blocks(
    config: &PlayoutConfig,
    events: &[ScheduleEvent],
    date: NaiveDate,
) -> Result<Vec<GuideBlock>, ServiceError> {
    let from = date.and_time(NaiveTime::MIN);
    let to = from + TimeDelta::try_days(1).unwrap_or_default();
    let format = |time: NaiveDateTime| time.format("%Y-%m-%dT%H:%M:%S").to_string();

    Ok(programmes_between(config, events, from, to)
        .await?
        .into_iter()
        .map(|programme| {
            let (title, sub_title) = programme.titles();

            GuideBlock {
                start: format(programme.start),
                stop: format(programme.stop),
                title,
                sub_title,
                scheduled: programme.scheduled,
            }
        })
        .collect())
}

/// A playlist clip the player wants to place in the guide.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipQuery {
    pub source: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub ad: bool,
    /// Seconds the clip plays (`out - in`). Accepted for compatibility;
    /// inserts are now the clips their sheet marks.
    #[serde(default)]
    #[allow(dead_code)]
    pub length: f64,
}

/// The guide programme a playlist clip belongs to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClipProgramme {
    /// Consecutive clips with the same key form one guide entry.
    pub key: String,
    pub title: String,
    pub sub_title: Option<String>,
    /// Ads and filler, which extend the programme they interrupt.
    pub is_break: bool,
    /// A short clip without a sheet (an ident, a promo) placed inside or
    /// after a programme; it extends that programme too.
    pub insert: bool,
    /// From a commercials folder: a break whatever the playlist says.
    pub commercial: bool,
}

/// What [`insert_runs`] needs to know about one clip.
struct RunClip {
    key: String,
    /// Marked as an ident or promo in its sheet.
    insert: bool,
    is_break: bool,
}

/// Which clips are inserts: clips their sheet marks as an ident or promo,
/// after a programme with another key. Breaks do not start or end a
/// programme; a marked clip with no programme before it stands alone.
fn insert_runs(clips: &[RunClip]) -> Vec<bool> {
    let mut programme: Option<&str> = None;

    clips
        .iter()
        .map(|clip| {
            if clip.is_break {
                return false;
            }

            let is_insert = clip.insert && programme.is_some_and(|key| key != clip.key);

            if !is_insert {
                programme = Some(&clip.key);
            }

            is_insert
        })
        .collect()
}

/// Key of the guide entry a clip belongs to.
fn programme_key(clip: &ClipMetadata, playlist_title: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        clip.program_key.display(),
        clip.episode_key
            .as_ref()
            .map(|key| key.display().to_string())
            .unwrap_or_default(),
        playlist_title.unwrap_or_default()
    )
}

/// Program names of playlist clips, as the guide will show them. The player
/// uses this to group a playlist, including changes that are not saved yet.
pub async fn clip_programmes(config: &PlayoutConfig, clips: Vec<ClipQuery>) -> Vec<ClipProgramme> {
    let mut library = NfoLibrary::new(&config.channel.storage);
    let mut commercials = CommercialFolders::new(&config.channel.storage);
    let filler = filler_path(config);
    let mut programmes = Vec::with_capacity(clips.len());
    let mut runs = Vec::with_capacity(clips.len());

    for query in clips {
        let commercial = commercials.contains(&query.source);
        let is_break = query.ad
            || commercial
            || filler
                .as_deref()
                .is_some_and(|filler| Path::new(&query.source).clean().starts_with(filler));
        let clip = library.lookup(&query.source).await;
        let playlist_title = query
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());
        let key = programme_key(&clip, playlist_title.as_deref());

        runs.push(RunClip {
            key: key.clone(),
            insert: clip.insert,
            is_break,
        });

        let programme = Programme {
            start: NaiveDateTime::default(),
            stop: NaiveDateTime::default(),
            source: query.source,
            playlist_title,
            clip,
            scheduled: false,
        };
        let (title, sub_title) = programme.titles();

        programmes.push(ClipProgramme {
            key,
            title,
            sub_title,
            is_break,
            insert: false,
            commercial,
        });
    }

    for (programme, insert) in programmes.iter_mut().zip(insert_runs(&runs)) {
        programme.insert = insert;
    }

    programmes
}

/// Place every playlist entry between `from` and `to` on the timeline.
///
/// Times are accumulated like in [`crate::utils::program::build_program`], so
/// the guide matches the program view. A playlist ends where the next day's
/// playlist starts, and never runs longer than the configured playlist length.
async fn collect_slots(
    config: &PlayoutConfig,
    from: NaiveDateTime,
    to: NaiveDateTime,
) -> Result<Vec<Slot>, ServiceError> {
    let start_sec = config.playlist.start_sec.ok_or_else(|| {
        ServiceError::Conflict("Playlist start time has not been initialized".to_string())
    })?;
    let length_ms = (config.playlist.length_sec.unwrap_or(86_400.0) * 1000.0) as i64;
    let length = TimeDelta::try_milliseconds(length_ms).unwrap_or_default();
    // The playlist of the previous day can still be running at `from`.
    let first_day = from.date() - TimeDelta::try_days(1).unwrap_or_default();
    let day_count = (to.date() - first_day).num_days();
    let mut slots: Vec<Slot> = vec![];
    let mut commercials = CommercialFolders::new(&config.channel.storage);

    for offset in 0..=day_count {
        let date = (first_day + TimeDelta::try_days(offset).unwrap_or_default())
            .format("%Y-%m-%d")
            .to_string();
        let playlist = match read_playlist(config, date.clone()).await {
            Ok(playlist) => playlist,
            Err(ServiceError::NoContent()) => continue,
            Err(e) => {
                warn!("XMLTV: skip playlist {date}: {e}");
                continue;
            }
        };
        let day_start = playlist_day_start(&date, start_sec)?;
        let day_end = day_start + length;

        cut_at(&mut slots, day_start);

        let mut begin = day_start;

        for media in playlist.program {
            if begin >= day_end || begin >= to {
                break;
            }

            let duration_ms = ((media.out - media.seek) * 1000.0) as i64;
            let duration = TimeDelta::try_milliseconds(duration_ms).unwrap_or_default();
            let stop = (begin + duration).min(day_end);

            if duration > TimeDelta::zero() && stop > from {
                let ad = media.ad || commercials.contains(&media.source);

                slots.push(Slot {
                    start: begin,
                    stop,
                    day_end,
                    source: media.source,
                    title: media.title,
                    ad,
                    scheduled: false,
                    occurrence: None,
                });
            }

            begin += duration;
        }
    }

    Ok(slots)
}

/// Drop or shorten slots that would still run when a new playlist starts.
fn cut_at(slots: &mut Vec<Slot>, day_start: NaiveDateTime) {
    while let Some(last) = slots.last_mut() {
        if last.start < day_start {
            last.stop = last.stop.min(day_start);
            break;
        }

        slots.pop();
    }
}

async fn group_programmes(config: &PlayoutConfig, slots: Vec<Slot>) -> Vec<Programme> {
    let mut library = NfoLibrary::new(&config.channel.storage);
    let filler = filler_path(config);
    let mut programmes: Vec<Programme> = vec![];
    let mut entries = Vec::with_capacity(slots.len());

    for slot in slots {
        if is_break(&slot, filler.as_deref()) {
            entries.push((slot, None));
            continue;
        }

        let clip = library.lookup(&slot.source).await;
        // A scheduled event's own title only stands in when no `.nfo`
        // describes what it plays.
        let playlist_title = slot
            .title
            .clone()
            .filter(|_| !(slot.scheduled && clip.program.is_some()))
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());

        entries.push((slot, Some((clip, playlist_title))));
    }

    let runs: Vec<RunClip> = entries
        .iter()
        .map(|(_, entry)| match entry {
            Some((clip, title)) => RunClip {
                key: programme_key(clip, title.as_deref()),
                insert: clip.insert,
                is_break: false,
            },
            None => RunClip {
                key: String::new(),
                insert: false,
                is_break: true,
            },
        })
        .collect();
    let inserts = insert_runs(&runs);

    for ((slot, entry), insert) in entries.into_iter().zip(inserts) {
        // Breaks and inserts extend the programme they interrupt.
        let extends = entry.is_none() || insert;

        if extends
            && let Some(previous) = programmes.last_mut()
            && is_contiguous(previous.stop, slot.start)
        {
            previous.stop = slot.stop;
            continue;
        }

        let Some((clip, playlist_title)) = entry else {
            continue;
        };

        if let Some(previous) = programmes.last_mut()
            && is_contiguous(previous.stop, slot.start)
            && previous.is_same_programme(&clip, &playlist_title)
        {
            previous.stop = slot.stop;
            continue;
        }

        programmes.push(Programme {
            start: slot.start,
            stop: slot.stop,
            source: slot.source,
            playlist_title,
            clip,
            scheduled: slot.scheduled,
        });
    }

    programmes
}

fn is_contiguous(previous_stop: NaiveDateTime, next_start: NaiveDateTime) -> bool {
    (next_start - previous_stop).num_milliseconds().abs() <= 1000
}

/// One entry of the plan as the report lists it: a playlist clip, or a
/// scheduled event placed the way the player airs it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlannedSlot {
    pub start: NaiveDateTime,
    pub stop: NaiveDateTime,
    pub source: String,
    pub title: Option<String>,
    /// A playlist ad or a clip from a commercials folder.
    pub ad: bool,
    /// Index of the event's occurrence in the list given to [`planned_slots`].
    pub occurrence: Option<usize>,
}

/// The plan of `[from, to)`: the playlist clips, moved by the scheduled
/// `occurrences` the way the player airs them. `shift` is the player's time
/// shift for the playlist day ending at the given time: its clips air that
/// many seconds earlier (later when negative), as the player has them.
pub(crate) async fn planned_slots(
    config: &PlayoutConfig,
    occurrences: &[Occurrence],
    from: NaiveDateTime,
    to: NaiveDateTime,
    shift: Option<(NaiveDateTime, f64)>,
) -> Result<Vec<PlannedSlot>, ServiceError> {
    let margin = shift.map_or(0.0, |(_, shift)| shift.abs());
    let mut slots = collect_slots(config, from - seconds(margin), to).await?;

    if let Some((day_end, shift)) = shift {
        let offset = TimeDelta::try_milliseconds((-shift * 1000.0) as i64).unwrap_or_default();

        for slot in slots.iter_mut().filter(|slot| slot.day_end == day_end) {
            slot.start += offset;
            slot.stop = (slot.stop + offset).min(day_end);
        }

        slots.retain(|slot| slot.start < slot.stop);
    }

    Ok(apply_schedule(slots, occurrences)
        .into_iter()
        .map(|slot| PlannedSlot {
            start: slot.start,
            stop: slot.stop,
            source: slot.source,
            title: slot.title,
            ad: slot.ad,
            occurrence: slot.occurrence,
        })
        .collect())
}

/// The configured filler file or folder, if one is set.
pub(crate) fn filler_path(config: &PlayoutConfig) -> Option<PathBuf> {
    let filler = config.storage.filler_path.clean();

    // An empty filler setting resolves to the storage root itself.
    (!config.storage.filler.trim().is_empty() && filler != config.channel.storage.clean())
        .then_some(filler)
}

fn is_break(slot: &Slot, filler: Option<&Path>) -> bool {
    slot.ad
        || (!slot.scheduled
            && filler.is_some_and(|filler| Path::new(&slot.source).clean().starts_with(filler)))
}

/// Place scheduled events on the timeline the way the player airs them: at
/// their time, or after the clip on air for events that wait. Afterwards the
/// playlist either runs later (continue) or the event covers it (clock).
fn apply_schedule(mut slots: Vec<Slot>, occurrences: &[Occurrence]) -> Vec<Slot> {
    for (index, occurrence) in occurrences.iter().enumerate() {
        let event = &occurrence.event;
        let start = match event.entry_mode {
            EntryMode::Interrupt => occurrence.start,
            EntryMode::AfterClip => slots
                .iter()
                .find(|slot| {
                    !slot.scheduled && slot.start < occurrence.start && occurrence.start < slot.stop
                })
                .map_or(occurrence.start, |slot| slot.stop),
        };
        let end = match event.kind {
            ScheduleKind::Media => start + seconds(event.duration),
            ScheduleKind::Live => occurrence.end,
        };

        if end <= start {
            continue;
        }

        let day_end = slots
            .iter()
            .find(|slot| slot.start <= start && start < slot.stop)
            .map_or(end, |slot| slot.day_end);

        slots = match event.after_mode {
            AfterMode::Clock => cover(slots, start, end),
            AfterMode::Continue => delay(slots, start, end - start, day_end),
        };

        slots.push(Slot {
            start,
            stop: end,
            day_end,
            source: event.sources.first().cloned().unwrap_or_default(),
            title: Some(event.title.clone()),
            ad: !event.show_in_guide,
            scheduled: true,
            occurrence: Some(index),
        });
        slots.sort_by_key(|slot| slot.start);
    }

    slots
}

/// Remove `[start, end)` from the playlist slots; a slot that spans it keeps
/// its parts before and after.
fn cover(slots: Vec<Slot>, start: NaiveDateTime, end: NaiveDateTime) -> Vec<Slot> {
    let mut covered = Vec::with_capacity(slots.len() + 1);

    for slot in slots {
        if slot.stop <= start || slot.start >= end {
            covered.push(slot);
            continue;
        }

        if slot.start < start {
            covered.push(Slot {
                stop: start,
                ..slot.clone()
            });
        }

        if slot.stop > end {
            covered.push(Slot { start: end, ..slot });
        }
    }

    covered
}

/// Push the playlist slots from `start` on by `length`, up to the end of
/// their playlist day; the next day's playlist starts on time.
fn delay(
    slots: Vec<Slot>,
    start: NaiveDateTime,
    length: TimeDelta,
    day_end: NaiveDateTime,
) -> Vec<Slot> {
    let mut delayed = Vec::with_capacity(slots.len() + 1);

    for slot in slots {
        if slot.stop <= start || slot.day_end != day_end {
            delayed.push(slot);
            continue;
        }

        let mut later = slot.clone();

        if slot.start < start {
            delayed.push(Slot {
                stop: start,
                ..slot
            });
            later.start = start;
        }

        later.start += length;
        later.stop = (later.stop + length).min(day_end);

        if later.start < later.stop {
            delayed.push(later);
        }
    }

    delayed
}

/// The guide's own logo when one is set and found, else the logo on air:
/// an image inside the storage.
async fn channel_icon(config: &PlayoutConfig) -> Option<Image> {
    let guide_logo = config.processing.guide_logo.trim();
    let guide_logo = (!guide_logo.is_empty())
        .then(|| norm_abs_path(&config.channel.storage, guide_logo).ok())
        .flatten()
        .map(|(path, _, _)| path);

    for logo in guide_logo
        .into_iter()
        .chain([PathBuf::from(&config.processing.logo_path)])
    {
        let is_file = fs::metadata(&logo)
            .await
            .is_ok_and(|metadata| metadata.is_file());

        if is_file && has_image_extension(&logo) {
            return Some(Image::Local(logo));
        }
    }

    None
}

fn title_from_file_name(source: &str) -> String {
    let title = Path::new(source)
        .file_stem()
        .map(|stem| stem.to_string_lossy().replace('_', " "))
        .unwrap_or_default();
    let title = title.trim();

    if title.is_empty() {
        source.to_string()
    } else {
        title.to_string()
    }
}

#[cfg(test)]
mod tests;
