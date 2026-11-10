//! XMLTV document serialization, escaping and public artwork URLs.
//!
//! The parent chooses programmes and their timeline. This module writes the
//! existing XML contract without loading playlists, inspecting files or querying DB.

use std::path::{Component, PathBuf};

use chrono::DateTime;
use chrono_tz::Tz;
use quick_xml::escape::escape;

use super::{
    Programme,
    library::{GuideCredit, Image},
    nfo::{CreditRole, Nfo},
};

pub(super) fn write_programme(
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

    if !programme.clip.credits.is_empty() {
        write_credits(xml, &programme.clip.credits, images);
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

/// The people in DTD order: `role` and `guest` on actors, and a person's
/// photo as an `<image type="person">`.
fn write_credits(xml: &mut XmlWriter, credits: &[GuideCredit], images: &ImageUrls) {
    xml.open(2, "credits", &[]);

    for entry in credits {
        let credit = &entry.credit;
        let mut attributes = vec![];

        if credit.role == CreditRole::Actor {
            if let Some(character) = &credit.character {
                attributes.push(("role", character.as_str()));
            }

            attributes.push(("guest", if credit.guest { "yes" } else { "no" }));
        }

        let photo = entry.photo.as_ref().and_then(|image| images.url(image));

        xml.person(
            3,
            credit.role.xmltv_element(),
            &attributes,
            &credit.name,
            photo.as_deref(),
        );
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
pub(super) fn xmltv_date(nfo: &Nfo) -> Option<String> {
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
pub(super) fn episode_numbers(nfo: &Nfo) -> Option<(String, String)> {
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
pub(super) fn ratings(rating: &str) -> Vec<(Option<&str>, &str)> {
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

/// Local images go through the image route,
/// remote `<thumb>` URLs are used as they are.
/// Turns artwork into URLs: storage images go through the image route,
/// remote `<thumb>` URLs are used as they are.
pub(super) struct ImageUrls<'a> {
    pub(super) base: &'a str,
    pub(super) storage: PathBuf,
}

impl ImageUrls<'_> {
    pub(super) fn url(&self, image: &Image) -> Option<String> {
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
pub(super) struct XmlWriter<'a> {
    out: String,
    lang: Option<&'a str>,
}

impl<'a> XmlWriter<'a> {
    pub(super) fn new(lang: Option<&'a str>) -> Self {
        Self {
            out: String::new(),
            lang,
        }
    }

    pub(super) fn raw(&mut self, text: &str) {
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

    pub(super) fn open(&mut self, depth: usize, name: &str, attributes: &[(&str, &str)]) {
        self.start_tag(depth, name, attributes);
        self.out.push_str(">\n");
    }

    pub(super) fn close(&mut self, depth: usize, name: &str) {
        self.out.push_str(&"  ".repeat(depth));
        self.out.push_str("</");
        self.out.push_str(name);
        self.out.push_str(">\n");
    }

    /// Element with text, or self-closing when `text` is `None`.
    pub(super) fn element(
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

    /// A person of the credits, with their photo after the name. Written on
    /// one line, so the name keeps no layout whitespace.
    fn person(
        &mut self,
        depth: usize,
        name: &str,
        attributes: &[(&str, &str)],
        text: &str,
        photo: Option<&str>,
    ) {
        let Some(photo) = photo else {
            return self.element(depth, name, attributes, Some(text));
        };

        self.start_tag(depth, name, attributes);
        self.out.push('>');
        self.out.push_str(&escape(xml_safe(text)));
        self.out.push_str("<image type=\"person\">");
        self.out.push_str(&escape(xml_safe(photo)));
        self.out.push_str("</image></");
        self.out.push_str(name);
        self.out.push_str(">\n");
    }

    /// Element with text and the `lang` attribute, when a language is set.
    pub(super) fn localized(
        &mut self,
        depth: usize,
        name: &str,
        attributes: &[(&str, &str)],
        text: &str,
    ) {
        let mut attributes = attributes.to_vec();

        if let Some(lang) = self.lang {
            attributes.push(("lang", lang));
        }

        self.element(depth, name, &attributes, Some(text));
    }

    pub(super) fn finish(self) -> String {
        self.out
    }
}

/// Drop characters XML 1.0 does not allow, such as stray control characters
/// in hand-edited `.nfo` files.
pub(super) fn xml_safe(text: &str) -> String {
    text.chars()
        .filter(|c| {
            matches!(c, '\t' | '\n' | '\r') || (*c >= ' ' && !matches!(c, '\u{FFFE}' | '\u{FFFF}'))
        })
        .collect()
}
