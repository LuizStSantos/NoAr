//! Reader for Kodi-style `.nfo` files.
//!
//! Only the fields the XMLTV guide needs are read. Parsing is lenient on
//! purpose: a file that is not XML (Kodi also accepts a bare scraper URL), that
//! is cut off, or that has mismatched tags yields whatever was read up to that
//! point instead of an error, so one broken file never breaks the whole guide.

use quick_xml::{
    Reader,
    escape::resolve_xml_entity,
    events::{BytesRef, BytesStart, Event},
};

/// Root element of the `.nfo` file.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum NfoKind {
    TvShow,
    Movie,
    Episode,
    MusicVideo,
    #[default]
    Other,
}

impl NfoKind {
    fn from_root(name: &str) -> Self {
        match name {
            "tvshow" => Self::TvShow,
            "movie" => Self::Movie,
            "episodedetails" => Self::Episode,
            "musicvideo" => Self::MusicVideo,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NfoActor {
    pub name: String,
    pub role: Option<String>,
}

/// A `<thumb>` entry: a path relative to the `.nfo`, an absolute path or a URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfoThumb {
    pub aspect: Option<String>,
    pub location: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Nfo {
    pub kind: NfoKind,
    pub title: Option<String>,
    pub show_title: Option<String>,
    pub plot: Option<String>,
    pub outline: Option<String>,
    pub genres: Vec<String>,
    pub year: Option<String>,
    pub premiered: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub rating: Option<String>,
    pub directors: Vec<String>,
    pub writers: Vec<String>,
    pub actors: Vec<NfoActor>,
    pub countries: Vec<String>,
    pub thumbs: Vec<NfoThumb>,
    pub fanart: Vec<String>,
}

impl Nfo {
    /// Description text, preferring the full plot over the short outline.
    pub fn description(&self) -> Option<&str> {
        self.plot.as_deref().or(self.outline.as_deref())
    }

    /// First `<thumb>` meant as a poster (Kodi writes posters with
    /// `aspect="poster"`, episode stills and older files without an aspect).
    pub fn poster_thumb(&self) -> Option<&str> {
        self.thumbs
            .iter()
            .find(|thumb| matches!(thumb.aspect.as_deref(), None | Some("poster")))
            .map(|thumb| thumb.location.as_str())
    }

    /// First wide image: a landscape thumb or a fanart entry.
    pub fn backdrop_thumb(&self) -> Option<&str> {
        self.thumbs
            .iter()
            .find(|thumb| matches!(thumb.aspect.as_deref(), Some("landscape" | "fanart")))
            .map(|thumb| thumb.location.as_str())
            .or_else(|| self.fanart.first().map(String::as_str))
    }

    fn set_field(&mut self, name: &str, value: String, aspect: Option<String>) {
        match name {
            "title" => set_once(&mut self.title, value),
            "showtitle" => set_once(&mut self.show_title, value),
            "plot" => set_once(&mut self.plot, value),
            "outline" => set_once(&mut self.outline, value),
            "genre" => push_split(&mut self.genres, &value),
            "year" => set_once(&mut self.year, value),
            "premiered" | "aired" | "releasedate" => set_once(&mut self.premiered, value),
            "season" => self.season = self.season.or_else(|| value.parse().ok()),
            "episode" => self.episode = self.episode.or_else(|| value.parse().ok()),
            "mpaa" | "certification" => set_once(&mut self.rating, value),
            "director" => push_split(&mut self.directors, &value),
            "credits" | "writer" => push_split(&mut self.writers, &value),
            "country" => push_split(&mut self.countries, &value),
            "thumb" => self.thumbs.push(NfoThumb {
                aspect,
                location: value,
            }),
            _ => {}
        }
    }
}

fn set_once(field: &mut Option<String>, value: String) {
    if field.is_none() {
        *field = Some(value);
    }
}

/// Kodi writes one element per value, but older scrapers joined several
/// values into one element with " / ".
fn push_split(list: &mut Vec<String>, value: &str) {
    for part in value.split(" / ").map(str::trim).filter(|p| !p.is_empty()) {
        if !list.iter().any(|existing| existing == part) {
            list.push(part.to_string());
        }
    }
}

struct OpenElement {
    name: String,
    text: String,
    aspect: Option<String>,
}

/// Parse the text of a `.nfo` file.
///
/// Files with several root elements (Kodi's multi-episode files) yield the
/// first one.
pub fn parse_nfo(content: &str) -> Nfo {
    let mut nfo = Nfo::default();
    let mut reader = Reader::from_str(content);
    let config = reader.config_mut();

    // Text is trimmed per element when it closes. Trimming every text event
    // would eat the spaces around entities: "Tom &amp; Jerry" is read as
    // three events, "Tom ", "&amp;" and " Jerry".
    config.trim_text(false);
    config.check_end_names = false;
    config.allow_unmatched_ends = true;
    config.allow_dangling_amp = true;

    let mut open: Vec<OpenElement> = vec![];
    let mut actor: Option<NfoActor> = None;

    loop {
        let event = match reader.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(event) => event,
        };

        match event {
            Event::Start(start) => {
                let element = open_element(&start);

                if open.is_empty() {
                    nfo.kind = NfoKind::from_root(&element.name);
                } else if open.len() == 1 && element.name == "actor" {
                    actor = Some(NfoActor::default());
                }

                open.push(element);
            }
            Event::Text(text) => {
                if let (Some(current), Ok(text)) = (open.last_mut(), text.xml10_content()) {
                    current.text.push_str(&text);
                }
            }
            Event::CData(data) => {
                if let (Some(current), Ok(text)) = (open.last_mut(), data.decode()) {
                    current.text.push_str(&text);
                }
            }
            Event::GeneralRef(reference) => {
                if let Some(current) = open.last_mut() {
                    push_reference(&mut current.text, &reference);
                }
            }
            Event::End(_) => {
                let Some(element) = open.pop() else {
                    continue;
                };

                if open.is_empty() {
                    // The first root element is complete.
                    break;
                }

                let value = element.text.trim().to_string();
                let parent = open.last().map(|parent| parent.name.as_str());

                match (open.len(), parent, element.name.as_str()) {
                    (1, _, "actor") => {
                        if let Some(actor) = actor.take().filter(|a| !a.name.is_empty()) {
                            nfo.actors.push(actor);
                        }
                    }
                    (2, Some("actor"), "name") => {
                        if let Some(actor) = actor.as_mut() {
                            actor.name = value;
                        }
                    }
                    (2, Some("actor"), "role") if !value.is_empty() => {
                        if let Some(actor) = actor.as_mut() {
                            actor.role = Some(value);
                        }
                    }
                    (2, Some("fanart"), "thumb") if !value.is_empty() => {
                        nfo.fanart.push(value);
                    }
                    (1, _, name) if !value.is_empty() => {
                        nfo.set_field(name, value, element.aspect);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    nfo
}

fn open_element(start: &BytesStart) -> OpenElement {
    let name = String::from_utf8_lossy(start.local_name().as_ref()).to_lowercase();
    let aspect = start
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.local_name().as_ref() == b"aspect")
        .map(|attribute| {
            String::from_utf8_lossy(&attribute.value)
                .trim()
                .to_lowercase()
        })
        .filter(|aspect| !aspect.is_empty());

    OpenElement {
        name,
        text: String::new(),
        aspect,
    }
}

fn push_reference(text: &mut String, reference: &BytesRef) {
    if let Ok(Some(character)) = reference.resolve_char_ref() {
        text.push(character);

        return;
    }

    let Ok(name) = reference.decode() else {
        return;
    };

    match resolve_xml_entity(&name) {
        Some(resolved) => text.push_str(resolved),
        // Unknown entities (e.g. HTML ones) are kept as written.
        None => {
            text.push('&');
            text.push_str(&name);
            text.push(';');
        }
    }
}

/// Turn the raw bytes of an `.nfo` file into text.
///
/// Most files are UTF-8, but files written by Windows tools are often UTF-16
/// (with a byte order mark) or Windows-1252, which older editors still use
/// for accented Latin text.
pub fn decode_nfo_bytes(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }

    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }

    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| windows_1252_char(*byte)).collect(),
    }
}

fn decode_utf16(bytes: &[u8], to_unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes
        .chunks_exact(2)
        .map(|pair| to_unit([pair[0], pair[1]]));

    char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Windows-1252 equals Latin-1 except for the 0x80..=0x9F block.
fn windows_1252_char(byte: u8) -> char {
    const HIGH_BLOCK: [char; 32] = [
        '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}',
        '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    ];

    match byte {
        0x80..=0x9F => HIGH_BLOCK[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tvshow_fields_and_artwork() {
        let nfo = parse_nfo(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<tvshow>
    <title>Jornal da Manhã</title>
    <plot>Notícias &amp; entrevistas ao vivo.</plot>
    <outline>Notícias</outline>
    <genre>Jornalismo</genre>
    <genre>Notícias / Variedades</genre>
    <mpaa>BR:L</mpaa>
    <premiered>2019-03-04</premiered>
    <season>-1</season>
    <thumb aspect="poster">poster.jpg</thumb>
    <thumb aspect="landscape">https://example.com/wide.jpg</thumb>
    <fanart><thumb>fanart.jpg</thumb></fanart>
    <actor><name>Ana Souza</name><role>Apresentadora</role></actor>
    <actor><name></name></actor>
</tvshow>"#,
        );

        assert_eq!(nfo.kind, NfoKind::TvShow);
        assert_eq!(nfo.title.as_deref(), Some("Jornal da Manhã"));
        assert_eq!(nfo.description(), Some("Notícias & entrevistas ao vivo."));
        assert_eq!(nfo.genres, ["Jornalismo", "Notícias", "Variedades"]);
        assert_eq!(nfo.rating.as_deref(), Some("BR:L"));
        assert_eq!(nfo.premiered.as_deref(), Some("2019-03-04"));
        assert_eq!(nfo.season, None);
        assert_eq!(nfo.poster_thumb(), Some("poster.jpg"));
        assert_eq!(nfo.backdrop_thumb(), Some("https://example.com/wide.jpg"));
        assert_eq!(nfo.fanart, ["fanart.jpg"]);
        assert_eq!(
            nfo.actors,
            [NfoActor {
                name: "Ana Souza".to_string(),
                role: Some("Apresentadora".to_string()),
            }]
        );
    }

    #[test]
    fn reads_the_first_episode_of_a_multi_episode_file() {
        let nfo = parse_nfo(
            "<episodedetails><title>Parte 1</title><season>2</season><episode>5</episode>\
             <aired>2026-10-01</aired><director>Diretor</director></episodedetails>\
             <episodedetails><title>Parte 2</title></episodedetails>",
        );

        assert_eq!(nfo.kind, NfoKind::Episode);
        assert_eq!(nfo.title.as_deref(), Some("Parte 1"));
        assert_eq!((nfo.season, nfo.episode), (Some(2), Some(5)));
        assert_eq!(nfo.premiered.as_deref(), Some("2026-10-01"));
        assert_eq!(nfo.directors, ["Diretor"]);
    }

    #[test]
    fn tolerates_broken_and_non_xml_files() {
        let truncated = parse_nfo("<movie><title>Filme</title><plot>Sem fim");
        let url_only = parse_nfo("https://www.themoviedb.org/movie/603\n");
        let html_entity = parse_nfo("<movie><title>A&nbsp;B &#233; C</title></movie>");

        assert_eq!(truncated.kind, NfoKind::Movie);
        assert_eq!(truncated.title.as_deref(), Some("Filme"));
        assert_eq!(url_only, Nfo::default());
        assert_eq!(html_entity.title.as_deref(), Some("A&nbsp;B é C"));
    }

    #[test]
    fn decodes_common_nfo_encodings() {
        let utf8_bom = [&[0xEF, 0xBB, 0xBF][..], "ção".as_bytes()].concat();
        let utf16_le = [0xFF, 0xFE, b'a', 0x00, 0xE7, 0x00];
        let windows_1252 = [
            b'P', b'r', b'o', 0xE7, 0xE3, b'o', b' ', 0x96, b' ', 0x93, 0x94,
        ];

        assert_eq!(decode_nfo_bytes(&utf8_bom), "ção");
        assert_eq!(decode_nfo_bytes(&utf16_le), "aç");
        assert_eq!(decode_nfo_bytes(&windows_1252), "Proção – \u{201C}\u{201D}");
    }
}
