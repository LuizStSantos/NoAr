//! Reads and writes the `.nfo` sheets and artwork the web interface edits: the
//! sheet of a program folder and the sheet of a single clip. Files are named
//! the way [`super::library`] looks them up, so the guide uses them as soon as
//! they are saved.

use std::path::{Path, PathBuf};

use quick_xml::{
    Reader, Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::utils::errors::ServiceError;

use super::{
    library::{BACKDROP_NAMES, IMAGE_EXTENSIONS, Listing, POSTER_NAMES, read_listing},
    nfo::{NfoKind, decode_nfo_bytes, parse_nfo},
};

/// Largest image accepted as artwork.
pub const MAX_ARTWORK_BYTES: usize = 25 * 1024 * 1024;

/// Largest `.nfo` text accepted.
const MAX_SHEET_BYTES: usize = 512 * 1024;

/// Elements the form fills in. Other elements of an existing file (ids,
/// ratings of other sites, thumbs, ...) are kept as they are.
const FORM_ELEMENTS: [&str; 15] = [
    "title",
    "plot",
    "genre",
    "mpaa",
    "certification",
    "year",
    "premiered",
    "aired",
    "releasedate",
    "country",
    "actor",
    "director",
    "season",
    "episode",
    "showtitle",
];

/// What a sheet describes, which sets the root element of the `.nfo`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetKind {
    /// A program or series: `<tvshow>`.
    #[default]
    Program,
    /// A film: `<movie>`.
    Movie,
    /// One episode: `<episodedetails>`.
    Episode,
}

impl SheetKind {
    fn root(self) -> &'static str {
        match self {
            Self::Program => "tvshow",
            Self::Movie => "movie",
            Self::Episode => "episodedetails",
        }
    }

    fn from_nfo(kind: NfoKind, fallback: Self) -> Self {
        match kind {
            NfoKind::TvShow => Self::Program,
            NfoKind::Movie => Self::Movie,
            NfoKind::Episode => Self::Episode,
            _ => fallback,
        }
    }
}

/// The artwork slots of a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtworkKind {
    /// Upright cover, 2:3.
    Poster,
    /// Wide background, 16:9.
    Backdrop,
    /// Image of one clip, 16:9; clips only.
    Still,
}

/// The fields the form edits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SheetFields {
    pub title: String,
    pub plot: String,
    pub genres: Vec<String>,
    /// Age rating, e.g. "Livre", "12".
    pub rating: String,
    pub year: String,
    /// First air date, `YYYY-MM-DD`.
    pub premiered: String,
    pub countries: Vec<String>,
    pub actors: Vec<String>,
    pub directors: Vec<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// The sheet of a folder or clip, as shown in the web interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MediaSheet {
    /// `true` for a clip's own sheet, `false` for a folder's program sheet.
    pub clip: bool,
    pub kind: SheetKind,
    /// Storage path of the `.nfo` file, when there is one.
    pub file: Option<String>,
    /// Storage path of the folder whose program sheet applies to this folder
    /// when it has none of its own (a season folder, for example).
    pub inherited_from: Option<String>,
    pub fields: SheetFields,
    /// The `.nfo` text as it is on disk.
    pub raw: String,
    /// Storage paths of the images.
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    pub still: Option<String>,
}

/// A change to a sheet: the form fields, or the `.nfo` text as it is.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SheetUpdate {
    #[serde(default)]
    pub kind: SheetKind,
    #[serde(default)]
    pub fields: SheetFields,
    /// When set, saved as the `.nfo` text instead of the fields.
    pub raw: Option<String>,
}

/// Where a sheet lives: a folder, or a clip inside a folder.
struct Target {
    dir: PathBuf,
    /// File name stem of the clip; `None` for a folder.
    clip_stem: Option<String>,
}

impl Target {
    async fn new(path: &Path) -> Result<Self, ServiceError> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|_| ServiceError::NotFound("File or folder not found".to_string()))?;

        if metadata.is_dir() {
            return Ok(Self {
                dir: path.to_path_buf(),
                clip_stem: None,
            });
        }

        let dir = path.parent().unwrap_or(path).to_path_buf();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .ok_or_else(|| ServiceError::BadRequest("Invalid file name".to_string()))?;

        Ok(Self {
            dir,
            clip_stem: Some(stem),
        })
    }

    /// File name of the `.nfo` that holds this sheet, if it exists.
    fn nfo_name<'a>(&self, listing: &'a Listing) -> Option<&'a str> {
        match &self.clip_stem {
            Some(stem) => listing.find(&format!("{stem}.nfo")),
            None => listing.program_nfo(),
        }
    }

    /// Name of a new `.nfo` for this sheet.
    fn new_nfo_name(&self, kind: SheetKind) -> String {
        match (&self.clip_stem, kind) {
            (Some(stem), _) => format!("{stem}.nfo"),
            (None, SheetKind::Movie) => "movie.nfo".to_string(),
            (None, _) => "tvshow.nfo".to_string(),
        }
    }

    /// Image stems the guide looks for in the slot, best first, and the stem
    /// a new image gets.
    fn artwork_stems(&self, kind: ArtworkKind) -> Result<(Vec<String>, String), ServiceError> {
        let names = |stems: &[&str]| stems.iter().map(ToString::to_string).collect();

        Ok(match (&self.clip_stem, kind) {
            (None, ArtworkKind::Poster) => (names(&POSTER_NAMES), "poster".to_string()),
            (None, ArtworkKind::Backdrop) => (names(&BACKDROP_NAMES), "fanart".to_string()),
            (None, ArtworkKind::Still) => {
                return Err(ServiceError::BadRequest(
                    "Only clips have their own image".to_string(),
                ));
            }
            (Some(stem), ArtworkKind::Poster) => {
                (vec![format!("{stem}-poster")], format!("{stem}-poster"))
            }
            (Some(stem), ArtworkKind::Backdrop) => (
                vec![format!("{stem}-fanart"), format!("{stem}-landscape")],
                format!("{stem}-fanart"),
            ),
            (Some(stem), ArtworkKind::Still) => {
                (vec![format!("{stem}-thumb")], format!("{stem}-thumb"))
            }
        })
    }

    fn artwork(&self, listing: &Listing, kind: ArtworkKind) -> Option<PathBuf> {
        let (stems, _) = self.artwork_stems(kind).ok()?;
        let stems: Vec<&str> = stems.iter().map(String::as_str).collect();

        listing
            .find_any_image(&stems)
            .map(|name| self.dir.join(name))
    }
}

/// The sheet of the folder or clip at `path` (absolute, inside `storage`).
pub async fn read_sheet(storage: &Path, path: &Path) -> Result<MediaSheet, ServiceError> {
    let target = Target::new(path).await?;
    let listing = read_listing(&target.dir).await;
    let fallback = match target.clip_stem {
        Some(_) => SheetKind::Episode,
        None => SheetKind::Program,
    };
    let nfo_path = target.nfo_name(&listing).map(|name| target.dir.join(name));
    let raw = match &nfo_path {
        Some(nfo_path) => decode_nfo_bytes(&fs::read(nfo_path).await?),
        None => String::new(),
    };
    let nfo = parse_nfo(&raw);
    let inherited_from = match (&nfo_path, &target.clip_stem) {
        (None, None) => inherited_program(storage, &target.dir).await,
        _ => None,
    };
    let relative = |path: PathBuf| storage_path(storage, &path);

    Ok(MediaSheet {
        clip: target.clip_stem.is_some(),
        kind: if nfo_path.is_some() {
            SheetKind::from_nfo(nfo.kind, fallback)
        } else {
            fallback
        },
        file: nfo_path.clone().map(relative),
        inherited_from: inherited_from.map(relative),
        fields: SheetFields {
            title: nfo.title.unwrap_or_default(),
            plot: nfo.plot.or(nfo.outline).unwrap_or_default(),
            genres: nfo.genres,
            rating: nfo.rating.unwrap_or_default(),
            year: nfo.year.unwrap_or_default(),
            premiered: nfo.premiered.unwrap_or_default(),
            countries: nfo.countries,
            actors: nfo.actors.into_iter().map(|actor| actor.name).collect(),
            directors: nfo.directors,
            season: nfo.season,
            episode: nfo.episode,
        },
        raw,
        poster: target.artwork(&listing, ArtworkKind::Poster).map(relative),
        backdrop: target
            .artwork(&listing, ArtworkKind::Backdrop)
            .map(relative),
        still: target
            .clip_stem
            .as_ref()
            .and_then(|_| target.artwork(&listing, ArtworkKind::Still))
            .map(relative),
    })
}

/// Save the sheet of the folder or clip at `path` and return it as stored.
pub async fn save_sheet(
    storage: &Path,
    path: &Path,
    update: SheetUpdate,
) -> Result<MediaSheet, ServiceError> {
    let target = Target::new(path).await?;
    let listing = read_listing(&target.dir).await;
    let existing_name = target.nfo_name(&listing).map(str::to_string);
    let nfo_path = target.dir.join(
        existing_name
            .clone()
            .unwrap_or_else(|| target.new_nfo_name(update.kind)),
    );
    let text = match update.raw {
        Some(raw) => {
            check_xml(&raw)?;
            raw
        }
        None => {
            let existing = match existing_name {
                Some(_) => Some(decode_nfo_bytes(&fs::read(&nfo_path).await?)),
                None => None,
            };

            sheet_document(existing.as_deref(), update.kind, &update.fields)
        }
    };

    if text.len() > MAX_SHEET_BYTES {
        return Err(ServiceError::BadRequest(
            "The sheet is too large".to_string(),
        ));
    }

    fs::write(&nfo_path, text).await?;

    read_sheet(storage, path).await
}

/// Store an image in an artwork slot of the folder or clip at `path`. The
/// images the guide would find before it in the slot are removed, so the new
/// one is used.
pub async fn save_artwork(
    storage: &Path,
    path: &Path,
    kind: ArtworkKind,
    bytes: &[u8],
) -> Result<MediaSheet, ServiceError> {
    let extension = image_extension(bytes)
        .ok_or_else(|| ServiceError::BadRequest("Use a JPG, PNG, WebP or GIF image".to_string()))?;

    if bytes.len() > MAX_ARTWORK_BYTES {
        return Err(ServiceError::BadRequest(
            "The image is too large".to_string(),
        ));
    }

    let target = Target::new(path).await?;
    let (_, stem) = target.artwork_stems(kind)?;

    remove_artwork_files(&target, kind).await?;
    fs::write(target.dir.join(format!("{stem}.{extension}")), bytes).await?;

    read_sheet(storage, path).await
}

/// Remove the images of an artwork slot of the folder or clip at `path`.
pub async fn remove_artwork(
    storage: &Path,
    path: &Path,
    kind: ArtworkKind,
) -> Result<MediaSheet, ServiceError> {
    let target = Target::new(path).await?;

    remove_artwork_files(&target, kind).await?;

    read_sheet(storage, path).await
}

async fn remove_artwork_files(target: &Target, kind: ArtworkKind) -> Result<(), ServiceError> {
    let (stems, _) = target.artwork_stems(kind)?;
    let listing = read_listing(&target.dir).await;

    for name in listing.names() {
        let path = Path::new(name);
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let is_image = path
            .extension()
            .map(|extension| extension.to_string_lossy().to_lowercase())
            .is_some_and(|extension| IMAGE_EXTENSIONS.contains(&extension.as_str()));

        if is_image
            && stems
                .iter()
                .any(|candidate| candidate.to_lowercase() == stem)
        {
            fs::remove_file(target.dir.join(name)).await?;
        }
    }

    Ok(())
}

/// Whether a folder has a program sheet of its own.
pub async fn has_program_sheet(dir: &Path) -> bool {
    read_listing(dir).await.program_nfo().is_some()
}

/// The kind of a folder's own program sheet: a program or series (one
/// sheet for every episode), or a film.
pub async fn program_sheet_kind(dir: &Path) -> Option<SheetKind> {
    let listing = read_listing(dir).await;
    let name = listing.program_nfo()?;
    let raw = fs::read(dir.join(name))
        .await
        .map(|bytes| decode_nfo_bytes(&bytes))
        .unwrap_or_default();

    Some(SheetKind::from_nfo(
        parse_nfo(&raw).kind,
        SheetKind::Program,
    ))
}

/// The nearest parent folder (inside the storage) with a program sheet.
async fn inherited_program(storage: &Path, dir: &Path) -> Option<PathBuf> {
    let mut parent = dir.parent()?.to_path_buf();

    while parent.starts_with(storage) && parent != storage {
        if has_program_sheet(&parent).await {
            return Some(parent);
        }

        if !parent.pop() {
            break;
        }
    }

    None
}

fn storage_path(storage: &Path, path: &Path) -> String {
    path.strip_prefix(storage)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

/// The file extension for the image in `bytes`, from its signature.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

/// Refuse text that is not well-formed XML with one root element.
fn check_xml(text: &str) -> Result<(), ServiceError> {
    let mut reader = Reader::from_str(text);
    let mut depth = 0_usize;
    let mut roots = 0_usize;

    loop {
        let event = reader.read_event().map_err(|error| {
            ServiceError::BadRequest(format!(
                "The .nfo is not valid XML (position {}): {error}",
                reader.error_position()
            ))
        })?;

        match event {
            Event::Eof => break,
            Event::Start(_) => {
                if depth == 0 {
                    roots += 1;
                }

                depth += 1;
            }
            Event::Empty(_) if depth == 0 => roots += 1,
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    if roots == 0 {
        return Err(ServiceError::BadRequest(
            "The .nfo has no root element, like <tvshow>".to_string(),
        ));
    }

    Ok(())
}

/// The `.nfo` text for `fields`. Elements of `existing` the form does not
/// fill in are kept as they were written; actors are kept with their roles
/// when the list of names did not change.
pub fn sheet_document(existing: Option<&str>, kind: SheetKind, fields: &SheetFields) -> String {
    let existing_actors: Option<Vec<String>> = existing.map(|text| {
        parse_nfo(text)
            .actors
            .into_iter()
            .map(|actor| actor.name)
            .collect()
    });
    let keep_actors = existing_actors.as_ref() == Some(&fields.actors);
    let replaced: Vec<&str> = FORM_ELEMENTS
        .iter()
        .copied()
        .filter(|name| !(keep_actors && *name == "actor"))
        .collect();
    let kept = existing
        .and_then(|text| other_elements(text, &replaced))
        .unwrap_or_default();

    write_document(kind, fields, keep_actors, &kept).unwrap_or_default()
}

fn write_document(
    kind: SheetKind,
    fields: &SheetFields,
    keep_actors: bool,
    kept: &[String],
) -> std::io::Result<String> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    let root = kind.root();
    let date_element = match kind {
        SheetKind::Episode => "aired",
        _ => "premiered",
    };

    writer.write_event(Event::Decl(BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        Some("yes"),
    )))?;
    writer.write_event(Event::Start(BytesStart::new(root)))?;

    write_text(&mut writer, "title", &fields.title)?;

    if kind == SheetKind::Episode {
        write_number(&mut writer, "season", fields.season)?;
        write_number(&mut writer, "episode", fields.episode)?;
    }

    write_text(&mut writer, "plot", &fields.plot)?;

    for genre in &fields.genres {
        write_text(&mut writer, "genre", genre)?;
    }

    write_text(&mut writer, "mpaa", &fields.rating)?;
    write_text(&mut writer, "year", &fields.year)?;
    write_text(&mut writer, date_element, &fields.premiered)?;

    for country in &fields.countries {
        write_text(&mut writer, "country", country)?;
    }

    for director in &fields.directors {
        write_text(&mut writer, "director", director)?;
    }

    if !keep_actors {
        for actor in fields.actors.iter().map(|name| name.trim()) {
            if actor.is_empty() {
                continue;
            }

            writer
                .create_element("actor")
                .write_inner_content(|writer| write_text(writer, "name", actor))?;
        }
    }

    for element in kept {
        writer.write_indent()?;
        writer.get_mut().extend_from_slice(element.as_bytes());
    }

    writer.write_event(Event::End(BytesEnd::new(root)))?;

    let mut text = String::from_utf8_lossy(&writer.into_inner()).to_string();
    text.push('\n');

    Ok(text)
}

fn write_text(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> std::io::Result<()> {
    let value = value.trim();

    if !value.is_empty() {
        writer
            .create_element(name)
            .write_text_content(BytesText::new(value))?;
    }

    Ok(())
}

fn write_number(
    writer: &mut Writer<Vec<u8>>,
    name: &str,
    value: Option<u32>,
) -> std::io::Result<()> {
    match value {
        Some(value) => write_text(writer, name, &value.to_string()),
        None => Ok(()),
    }
}

/// The child elements of the root of `text`, as written, leaving out those
/// named in `replaced`. `None` when the text is not well-formed XML.
fn other_elements(text: &str, replaced: &[&str]) -> Option<Vec<String>> {
    let mut reader = Reader::from_str(text);
    let mut in_root = false;
    let mut kept = vec![];

    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event().ok()?;

        match event {
            Event::Eof => break,
            Event::Start(_) if !in_root => in_root = true,
            Event::Start(start) => {
                let name = element_name(&start);

                reader.read_to_end(start.name()).ok()?;

                let after = usize::try_from(reader.buffer_position()).ok()?;

                if !replaced.contains(&name.as_str()) {
                    kept.push(text.get(before..after)?.trim().to_string());
                }
            }
            Event::Empty(element) if in_root => {
                let after = usize::try_from(reader.buffer_position()).ok()?;

                if !replaced.contains(&element_name(&element).as_str()) {
                    kept.push(text.get(before..after)?.trim().to_string());
                }
            }
            Event::End(_) if in_root => break,
            _ => {}
        }
    }

    Some(kept)
}

fn element_name(start: &BytesStart) -> String {
    String::from_utf8_lossy(start.local_name().as_ref()).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStorage(PathBuf);

    impl TempStorage {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("noar-sheets-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();

            Self(root)
        }

        fn file(&self, relative: &str, content: &[u8]) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();

            path
        }
    }

    impl Drop for TempStorage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0";

    fn fields() -> SheetFields {
        SheetFields {
            title: "Programa da Tarde".to_string(),
            plot: "Variedades & entrevistas".to_string(),
            genres: vec!["Variedades".to_string(), "Entrevistas".to_string()],
            rating: "Livre".to_string(),
            year: "2026".to_string(),
            actors: vec!["Ana".to_string()],
            ..SheetFields::default()
        }
    }

    #[test]
    fn a_new_sheet_is_a_kodi_file_the_reader_understands() {
        let text = sheet_document(None, SheetKind::Program, &fields());
        let nfo = parse_nfo(&text);

        assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>"));
        assert!(text.contains("<plot>Variedades &amp; entrevistas</plot>"));
        assert_eq!(nfo.kind, NfoKind::TvShow);
        assert_eq!(nfo.title.as_deref(), Some("Programa da Tarde"));
        assert_eq!(nfo.genres, ["Variedades", "Entrevistas"]);
        assert_eq!(nfo.rating.as_deref(), Some("Livre"));
        assert_eq!(nfo.actors[0].name, "Ana");
    }

    #[test]
    fn saving_keeps_what_the_form_does_not_edit() {
        let existing = r#"<?xml version="1.0"?>
<tvshow>
  <title>Old</title>
  <uniqueid type="tmdb" default="true">42</uniqueid>
  <genre>Old genre</genre>
  <thumb aspect="poster">https://example.org/p.jpg</thumb>
  <actor><name>Ana</name><role>Host</role></actor>
  <namedseason number="1"/>
</tvshow>"#;
        let text = sheet_document(Some(existing), SheetKind::Program, &fields());
        let nfo = parse_nfo(&text);

        assert_eq!(nfo.title.as_deref(), Some("Programa da Tarde"));
        assert_eq!(nfo.genres, ["Variedades", "Entrevistas"]);
        assert!(text.contains(r#"<uniqueid type="tmdb" default="true">42</uniqueid>"#));
        assert!(text.contains(r#"<thumb aspect="poster">https://example.org/p.jpg</thumb>"#));
        assert!(text.contains(r#"<namedseason number="1"/>"#));
        assert_eq!(
            nfo.actors[0].role.as_deref(),
            Some("Host"),
            "same names keep roles"
        );
        assert!(!text.contains("Old"));

        let mut other_actors = fields();
        other_actors.actors = vec!["Bruno".to_string()];
        let nfo = parse_nfo(&sheet_document(
            Some(existing),
            SheetKind::Movie,
            &other_actors,
        ));

        assert_eq!(nfo.kind, NfoKind::Movie);
        assert_eq!(nfo.actors.len(), 1);
        assert_eq!(nfo.actors[0].name, "Bruno");
    }

    #[test]
    fn episodes_get_season_episode_and_air_date() {
        let mut episode = fields();
        episode.season = Some(1);
        episode.episode = Some(12);
        episode.premiered = "2026-10-14".to_string();

        let text = sheet_document(None, SheetKind::Episode, &episode);
        let nfo = parse_nfo(&text);

        assert_eq!(nfo.kind, NfoKind::Episode);
        assert_eq!((nfo.season, nfo.episode), (Some(1), Some(12)));
        assert!(text.contains("<aired>2026-10-14</aired>"));
    }

    #[test]
    fn raw_sheets_must_be_xml_with_a_root() {
        assert!(check_xml("<tvshow><title>A</title></tvshow>").is_ok());
        assert!(check_xml("<tvshow><title>A</tvshow>").is_err());
        assert!(check_xml("just text").is_err());
    }

    #[tokio::test]
    async fn folder_sheets_tell_series_from_films() {
        let storage = TempStorage::new();
        storage.file("Serie/tvshow.nfo", b"<tvshow><title>Serie</title></tvshow>");
        storage.file("Filme/movie.nfo", b"<movie><title>Filme</title></movie>");
        storage.file("Avulso/clip.mp4", b"");

        assert_eq!(
            program_sheet_kind(&storage.0.join("Serie")).await,
            Some(SheetKind::Program)
        );
        assert_eq!(
            program_sheet_kind(&storage.0.join("Filme")).await,
            Some(SheetKind::Movie)
        );
        assert_eq!(program_sheet_kind(&storage.0.join("Avulso")).await, None);
    }

    #[tokio::test]
    async fn folder_sheets_are_created_found_and_inherited() {
        let storage = TempStorage::new();
        let program = storage.0.join("Novela");
        storage.file("Novela/Temporada 1/cap01.mp4", b"");

        let empty = read_sheet(&storage.0, &program).await.unwrap();

        assert_eq!(empty.file, None);
        assert_eq!(empty.kind, SheetKind::Program);

        let saved = save_sheet(
            &storage.0,
            &program,
            SheetUpdate {
                kind: SheetKind::Program,
                fields: fields(),
                raw: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(saved.file.as_deref(), Some("Novela/tvshow.nfo"));
        assert_eq!(saved.fields.title, "Programa da Tarde");

        let season = read_sheet(&storage.0, &program.join("Temporada 1"))
            .await
            .unwrap();

        assert_eq!(season.file, None);
        assert_eq!(season.inherited_from.as_deref(), Some("Novela"));

        let clip = read_sheet(&storage.0, &program.join("Temporada 1/cap01.mp4"))
            .await
            .unwrap();

        assert!(clip.clip);
        assert_eq!(clip.kind, SheetKind::Episode);
        assert_eq!(clip.inherited_from, None);
    }

    #[tokio::test]
    async fn new_artwork_replaces_what_the_guide_would_pick_first() {
        let storage = TempStorage::new();
        let program = storage.0.join("Jornal");
        storage.file("Jornal/folder.png", PNG);
        storage.file("Jornal/landscape.jpg", JPEG);
        let clip = storage.file("Jornal/bloco1.mp4", b"");

        let sheet = read_sheet(&storage.0, &program).await.unwrap();

        assert_eq!(sheet.poster.as_deref(), Some("Jornal/folder.png"));
        assert_eq!(sheet.backdrop.as_deref(), Some("Jornal/landscape.jpg"));

        let sheet = save_artwork(&storage.0, &program, ArtworkKind::Poster, JPEG)
            .await
            .unwrap();

        assert_eq!(sheet.poster.as_deref(), Some("Jornal/poster.jpg"));
        assert!(!program.join("folder.png").exists());

        let sheet = remove_artwork(&storage.0, &program, ArtworkKind::Backdrop)
            .await
            .unwrap();

        assert_eq!(sheet.backdrop, None);

        let sheet = save_artwork(&storage.0, &clip, ArtworkKind::Still, PNG)
            .await
            .unwrap();

        assert_eq!(sheet.still.as_deref(), Some("Jornal/bloco1-thumb.png"));
        assert!(
            save_artwork(&storage.0, &program, ArtworkKind::Still, PNG)
                .await
                .is_err()
        );
        assert!(
            save_artwork(&storage.0, &program, ArtworkKind::Poster, b"not an image")
                .await
                .is_err()
        );
    }
}
