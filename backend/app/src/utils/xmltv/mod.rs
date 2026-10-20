//! XMLTV guide built from the channel's playlists and the `.nfo` files next to
//! the media. See [`library`] for the expected folder layout.
//!
//! Consecutive clips of the same program are merged into one guide entry, and
//! ads and filler extend the entry they interrupt instead of showing up on
//! their own.

use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use chrono_tz::Tz;
use log::*;
use path_clean::PathClean;
use quick_xml::escape::escape;
use serde::{Deserialize, Serialize};
use tokio::fs;

pub mod library;
pub mod nfo;
pub mod sheet;

use crate::{
    db::models::{AfterMode, EntryMode, ScheduleEvent, ScheduleKind},
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
use library::{ClipMetadata, Image, NfoLibrary, has_image_extension};
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
    /// Seconds the clip plays (`out - in`).
    #[serde(default)]
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

/// Longest run of clips without a sheet or title that counts as an insert,
/// like a station ident or a promo: it extends the programme it interrupts
/// instead of starting a guide entry of its own.
pub const INSERT_MAX_SECONDS: f64 = 300.0;

/// What [`insert_runs`] needs to know about one clip.
struct RunClip {
    key: String,
    /// Has a `.nfo` or a playlist title, so it is meant to be seen.
    described: bool,
    is_break: bool,
    length: f64,
}

/// Which clips are inserts: runs of consecutive clips with the same key, no
/// description and at most [`INSERT_MAX_SECONDS`] in total, after a
/// programme with another key. Breaks do not start or end a programme.
fn insert_runs(clips: &[RunClip]) -> Vec<bool> {
    let mut inserts = vec![false; clips.len()];
    let mut programme_key: Option<&str> = None;
    let mut index = 0;

    while index < clips.len() {
        let first = &clips[index];

        if first.is_break {
            index += 1;
            continue;
        }

        let mut end = index + 1;

        while end < clips.len() && !clips[end].is_break && clips[end].key == first.key {
            end += 1;
        }

        let length: f64 = clips[index..end].iter().map(|clip| clip.length).sum();
        let is_insert = programme_key.is_some_and(|key| key != first.key)
            && !first.described
            && length <= INSERT_MAX_SECONDS;

        if is_insert {
            inserts[index..end].fill(true);
        } else {
            programme_key = Some(&first.key);
        }

        index = end;
    }

    inserts
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

fn is_described(clip: &ClipMetadata, playlist_title: Option<&str>) -> bool {
    clip.program.is_some() || clip.episode.is_some() || playlist_title.is_some()
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
            described: is_described(&clip, playlist_title.as_deref()),
            is_break,
            length: query.length,
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
        .map(|(slot, entry)| match entry {
            Some((clip, title)) => RunClip {
                key: programme_key(clip, title.as_deref()),
                described: is_described(clip, title.as_deref()),
                is_break: false,
                length: (slot.stop - slot.start).num_milliseconds() as f64 / 1000.0,
            },
            None => RunClip {
                key: String::new(),
                described: false,
                is_break: true,
                length: 0.0,
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

/// The channel logo, when it is an image inside the storage.
async fn channel_icon(config: &PlayoutConfig) -> Option<Image> {
    let logo = PathBuf::from(&config.processing.logo_path);
    let is_file = fs::metadata(&logo)
        .await
        .is_ok_and(|metadata| metadata.is_file());

    (is_file && has_image_extension(&logo)).then_some(Image::Local(logo))
}

fn write_programme(
    xml: &mut XmlWriter,
    channel_id: &str,
    start: DateTime<Tz>,
    stop: DateTime<Tz>,
    programme: &Programme,
    images: &ImageUrls,
) {
    let program = programme.clip.program.as_deref();
    let episode = programme.clip.episode.as_deref();
    let start = start.format("%Y%m%d%H%M%S %z").to_string();
    let stop = stop.format("%Y%m%d%H%M%S %z").to_string();
    let (title, sub_title) = programme.titles();
    let description = episode
        .and_then(Nfo::description)
        .or_else(|| program.and_then(Nfo::description));
    let nfos: Vec<&Nfo> = [episode, program].into_iter().flatten().collect();

    xml.open(
        1,
        "programme",
        &[("start", &start), ("stop", &stop), ("channel", channel_id)],
    );
    xml.localized(2, "title", &[], &title);

    if let Some(sub_title) = &sub_title {
        xml.localized(2, "sub-title", &[], sub_title);
    }

    if let Some(description) = description {
        xml.localized(2, "desc", &[], description);
    }

    // Credits come from one file, so episode and program casts don't mix.
    if let Some(credits) = nfos.iter().find(|nfo| has_credits(nfo)) {
        write_credits(xml, credits);
    }

    if let Some(date) = nfos.iter().find_map(|nfo| xmltv_date(nfo)) {
        xml.element(2, "date", &[], Some(&date));
    }

    for genre in unique(nfos.iter().rev().flat_map(|nfo| &nfo.genres)) {
        xml.localized(2, "category", &[], genre);
    }

    let artwork = &programme.clip.artwork;
    let still = artwork.still.as_ref().and_then(|image| images.url(image));
    let poster = artwork.poster.as_ref().and_then(|image| images.url(image));
    let backdrop = artwork
        .backdrop
        .as_ref()
        .and_then(|image| images.url(image));

    if let Some(icon) = still.as_ref().or(poster.as_ref()).or(backdrop.as_ref()) {
        xml.element(2, "icon", &[("src", icon)], None);
    }

    for country in unique(nfos.iter().rev().flat_map(|nfo| &nfo.countries)) {
        xml.element(2, "country", &[], Some(country));
    }

    if let Some((xmltv_ns, onscreen)) = episode.and_then(episode_numbers) {
        xml.element(2, "episode-num", &[("system", "xmltv_ns")], Some(&xmltv_ns));
        xml.element(2, "episode-num", &[("system", "onscreen")], Some(&onscreen));
    }

    if let Some(rating) = nfos.iter().find_map(|nfo| nfo.rating.as_deref()) {
        for (system, value) in ratings(rating) {
            let attributes: &[(&str, &str)] = match system {
                Some(system) => &[("system", system)],
                None => &[],
            };

            xml.open(2, "rating", attributes);
            xml.element(3, "value", &[], Some(value));
            xml.close(2, "rating");
        }
    }

    let typed_images = [
        ("poster", "P", poster),
        ("backdrop", "L", backdrop),
        ("still", "L", still),
    ];

    for (kind, orient, url) in typed_images {
        if let Some(url) = url {
            xml.element(
                2,
                "image",
                &[("type", kind), ("orient", orient)],
                Some(&url),
            );
        }
    }

    xml.close(1, "programme");
}

fn has_credits(nfo: &Nfo) -> bool {
    !nfo.directors.is_empty() || !nfo.actors.is_empty() || !nfo.writers.is_empty()
}

fn write_credits(xml: &mut XmlWriter, nfo: &Nfo) {
    xml.open(2, "credits", &[]);

    for director in &nfo.directors {
        xml.element(3, "director", &[], Some(director));
    }

    for actor in &nfo.actors {
        let attributes: &[(&str, &str)] = match &actor.role {
            Some(role) => &[("role", role)],
            None => &[],
        };

        xml.element(3, "actor", attributes, Some(&actor.name));
    }

    for writer in &nfo.writers {
        xml.element(3, "writer", &[], Some(writer));
    }

    xml.close(2, "credits");
}

fn unique<'a>(values: impl Iterator<Item = &'a String>) -> Vec<&'a str> {
    let mut unique: Vec<&str> = vec![];

    for value in values {
        if !unique.contains(&value.as_str()) {
            unique.push(value);
        }
    }

    unique
}

/// `YYYYMMDD` from the premiere date, or `YYYY` from the year.
fn xmltv_date(nfo: &Nfo) -> Option<String> {
    let digits: String = nfo
        .premiered
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();

    if digits.len() >= 8 {
        return Some(digits[..8].to_string());
    }

    nfo.year
        .as_deref()
        .map(str::trim)
        .filter(|year| year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .or_else(|| (digits.len() >= 4).then(|| digits[..4].to_string()))
}

/// `xmltv_ns` (zero based) and on-screen episode numbers.
fn episode_numbers(nfo: &Nfo) -> Option<(String, String)> {
    let zero_based = |number: Option<u32>| {
        number
            .filter(|number| *number > 0)
            .map(|number| (number - 1).to_string())
            .unwrap_or_default()
    };
    let onscreen = match (nfo.season, nfo.episode) {
        (Some(season), Some(episode)) => format!("S{season:02}E{episode:02}"),
        (Some(season), None) => format!("S{season:02}"),
        (None, Some(episode)) => format!("E{episode:02}"),
        (None, None) => return None,
    };
    let xmltv_ns = format!("{}.{}.", zero_based(nfo.season), zero_based(nfo.episode));

    Some((xmltv_ns, onscreen))
}

/// Split ratings like `BR:14` or `US:PG-13 / BR:12` into system and value.
fn ratings(rating: &str) -> Vec<(Option<&str>, &str)> {
    rating
        .split(" / ")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.split_once(':') {
            Some((system, value)) if !system.trim().is_empty() && !value.trim().is_empty() => {
                (Some(system.trim()), value.trim())
            }
            _ => (None, part),
        })
        .collect()
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

/// Turns artwork into URLs: storage images go through the image route,
/// remote `<thumb>` URLs are used as they are.
struct ImageUrls<'a> {
    base: &'a str,
    storage: PathBuf,
}

impl ImageUrls<'_> {
    fn url(&self, image: &Image) -> Option<String> {
        let path = match image {
            Image::Remote(url) => return Some(url.clone()),
            Image::Local(path) => path,
        };
        let relative = path.strip_prefix(&self.storage).ok()?;
        let mut url = self.base.to_string();

        for component in relative.components() {
            let Component::Normal(segment) = component else {
                return None;
            };

            url.push('/');
            url.push_str(&percent_encode(segment.to_str()?));
        }

        Some(url)
    }
}

/// Percent-encode one URL path segment.
fn percent_encode(segment: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(segment.len());

    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }

    encoded
}

/// Minimal indented XML output; all text and attribute values are escaped.
struct XmlWriter<'a> {
    out: String,
    lang: Option<&'a str>,
}

impl<'a> XmlWriter<'a> {
    fn new(lang: Option<&'a str>) -> Self {
        Self {
            out: String::new(),
            lang,
        }
    }

    fn raw(&mut self, text: &str) {
        self.out.push_str(text);
    }

    fn start_tag(&mut self, depth: usize, name: &str, attributes: &[(&str, &str)]) {
        self.out.push_str(&"  ".repeat(depth));
        self.out.push('<');
        self.out.push_str(name);

        for (key, value) in attributes {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            self.out.push_str(&escape(xml_safe(value)));
            self.out.push('"');
        }
    }

    fn open(&mut self, depth: usize, name: &str, attributes: &[(&str, &str)]) {
        self.start_tag(depth, name, attributes);
        self.out.push_str(">\n");
    }

    fn close(&mut self, depth: usize, name: &str) {
        self.out.push_str(&"  ".repeat(depth));
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push_str(">\n");
    }

    /// Element with text, or self-closing when `text` is `None`.
    fn element(
        &mut self,
        depth: usize,
        name: &str,
        attributes: &[(&str, &str)],
        text: Option<&str>,
    ) {
        self.start_tag(depth, name, attributes);

        match text {
            Some(text) => {
                self.out.push('>');
                self.out.push_str(&escape(xml_safe(text)));
                self.out.push_str("</");
                self.out.push_str(name);
                self.out.push_str(">\n");
            }
            None => self.out.push_str("/>\n"),
        }
    }

    /// Element with text and the `lang` attribute, when a language is set.
    fn localized(&mut self, depth: usize, name: &str, attributes: &[(&str, &str)], text: &str) {
        let mut attributes = attributes.to_vec();

        if let Some(lang) = self.lang {
            attributes.push(("lang", lang));
        }

        self.element(depth, name, &attributes, Some(text));
    }

    fn finish(self) -> String {
        self.out
    }
}

/// Drop characters XML 1.0 does not allow, such as stray control characters
/// in hand-edited `.nfo` files.
fn xml_safe(text: &str) -> String {
    text.chars()
        .filter(|c| {
            matches!(c, '\t' | '\n' | '\r') || (*c >= ' ' && !matches!(c, '\u{FFFE}' | '\u{FFFF}'))
        })
        .collect()
}

#[cfg(test)]
mod tests;
