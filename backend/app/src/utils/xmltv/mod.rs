//! XMLTV guide built from the channel's playlists and the `.nfo` files next to
//! the media. See [`library`] for the expected folder layout.
//!
//! Consecutive clips of the same program are merged into one guide entry, and
//! ads and filler extend the entry they interrupt instead of showing up on
//! their own.

use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeDelta};
use chrono_tz::Tz;
use log::*;
use path_clean::PathClean;
use quick_xml::escape::escape;
use tokio::fs;

pub mod library;
pub mod nfo;

use crate::{
    player::utils::Media,
    utils::{
        config::PlayoutConfig,
        errors::ServiceError,
        playlist::read_playlist,
        program::{channel_datetime, playlist_day_start},
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
struct Slot {
    start: NaiveDateTime,
    stop: NaiveDateTime,
    media: Media,
}

/// One entry of the guide; may cover several consecutive slots.
struct Programme {
    start: NaiveDateTime,
    stop: NaiveDateTime,
    source: String,
    playlist_title: Option<String>,
    clip: ClipMetadata,
}

impl Programme {
    fn is_same_programme(&self, clip: &ClipMetadata, playlist_title: &Option<String>) -> bool {
        self.clip.program_key == clip.program_key
            && self.clip.episode_key == clip.episode_key
            && &self.playlist_title == playlist_title
    }
}

/// Build the XMLTV document for one channel, from the start of the current
/// day for `options.days` days.
pub async fn build_xmltv(
    config: &PlayoutConfig,
    channel: &XmltvChannel,
    options: &XmltvOptions,
) -> Result<String, ServiceError> {
    let now = time_now(&config.channel.timezone);
    let timezone = now.timezone();
    let days = i64::from(options.days.clamp(1, MAX_DAYS));
    let from = now.date_naive().and_time(NaiveTime::MIN);
    let to = from + TimeDelta::try_days(days).unwrap_or_default();

    let slots = collect_slots(config, from, to).await?;
    let programmes = group_programmes(config, slots).await;
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
            ("generator-info-url", "https://github.com/LuizStSantos/noar"),
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
                slots.push(Slot {
                    start: begin,
                    stop,
                    media,
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

    for slot in slots {
        if is_break(&slot.media, filler.as_deref()) {
            if let Some(previous) = programmes.last_mut()
                && is_contiguous(previous.stop, slot.start)
            {
                previous.stop = slot.stop;
            }

            continue;
        }

        let clip = library.lookup(&slot.media.source).await;
        let playlist_title = slot
            .media
            .title
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());

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
            source: slot.media.source,
            playlist_title,
            clip,
        });
    }

    programmes
}

fn is_contiguous(previous_stop: NaiveDateTime, next_start: NaiveDateTime) -> bool {
    (next_start - previous_stop).num_milliseconds().abs() <= 1000
}

/// The configured filler file or folder, if one is set.
fn filler_path(config: &PlayoutConfig) -> Option<PathBuf> {
    let filler = config.storage.filler_path.clean();

    // An empty filler setting resolves to the storage root itself.
    (!config.storage.filler.trim().is_empty() && filler != config.channel.storage.clean())
        .then_some(filler)
}

fn is_break(media: &Media, filler: Option<&Path>) -> bool {
    media.ad || filler.is_some_and(|filler| Path::new(&media.source).clean().starts_with(filler))
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
    let playlist_title = programme.playlist_title.as_deref();
    let start = start.format("%Y%m%d%H%M%S %z").to_string();
    let stop = stop.format("%Y%m%d%H%M%S %z").to_string();

    let title = program
        .and_then(|nfo| nfo.title.as_deref())
        .or_else(|| episode.and_then(|nfo| nfo.show_title.as_deref()))
        .or(playlist_title)
        .or_else(|| episode.and_then(|nfo| nfo.title.as_deref()))
        .map(str::to_string)
        .unwrap_or_else(|| title_from_file_name(&programme.source));
    let sub_title = episode
        .and_then(|nfo| nfo.title.as_deref())
        .or(program.and(playlist_title))
        .filter(|sub_title| *sub_title != title);
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

    if let Some(sub_title) = sub_title {
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
