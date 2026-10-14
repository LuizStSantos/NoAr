//! Finds the `.nfo` files and artwork that describe a clip in the storage.
//!
//! Layout, following Kodi's conventions:
//!
//! ```text
//! storage/
//!   Morning News/             <- program folder
//!     tvshow.nfo              <- program metadata (any *.nfo name works)
//!     poster.jpg              <- program poster
//!     fanart.jpg              <- program backdrop
//!     2026-10-14.mp4
//!     2026-10-14.nfo          <- optional episode metadata (same name as the clip)
//!     2026-10-14-thumb.jpg    <- optional episode image
//! ```
//!
//! The program `.nfo` is searched in the clip's folder and then in each parent
//! folder up to (not including) the storage root, so seasons can live in
//! subfolders of the program folder.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use log::*;
use path_clean::PathClean;
use tokio::fs;

use super::nfo::{Nfo, NfoKind, decode_nfo_bytes, parse_nfo};

/// Image types that can be used as artwork and served by the XMLTV image route.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

/// Preferred names for the program `.nfo`. Any other `.nfo` in the folder that
/// does not belong to a clip is used when none of these exist.
const PROGRAM_NFO_NAMES: [&str; 6] = [
    "tvshow.nfo",
    "programa.nfo",
    "program.nfo",
    "show.nfo",
    "movie.nfo",
    "folder.nfo",
];

const POSTER_NAMES: [&str; 5] = ["poster", "folder", "cover", "capa", "show"];
const BACKDROP_NAMES: [&str; 5] = ["landscape", "thumb", "fanart", "backdrop", "background"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// Absolute path of a file inside the storage.
    Local(PathBuf),
    /// URL taken from a `<thumb>` entry.
    Remote(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Artwork {
    pub poster: Option<Image>,
    pub backdrop: Option<Image>,
    /// Image of this particular clip, e.g. an episode still.
    pub still: Option<Image>,
}

/// Everything known about one clip of the playlist.
#[derive(Debug, Clone)]
pub struct ClipMetadata {
    /// Clips with the same key belong to the same program: the folder holding
    /// the program `.nfo`, or the clip itself when there is none.
    pub program_key: PathBuf,
    pub program: Option<Arc<Nfo>>,
    /// The clip's own `.nfo`, if it has one and it is not used as the program.
    pub episode: Option<Arc<Nfo>>,
    pub episode_key: Option<PathBuf>,
    pub artwork: Artwork,
}

impl ClipMetadata {
    fn bare(clip: PathBuf) -> Self {
        Self {
            program_key: clip,
            program: None,
            episode: None,
            episode_key: None,
            artwork: Artwork::default(),
        }
    }
}

/// File names of one folder, looked up case-insensitively.
#[derive(Debug, Default)]
struct Listing {
    /// Lowercase name -> name on disk.
    files: HashMap<String, String>,
}

impl Listing {
    fn find(&self, name: &str) -> Option<&str> {
        self.files.get(&name.to_lowercase()).map(String::as_str)
    }

    fn find_image(&self, stem: &str) -> Option<&str> {
        IMAGE_EXTENSIONS
            .iter()
            .find_map(|extension| self.find(&format!("{stem}.{extension}")))
    }

    fn find_any_image(&self, stems: &[&str]) -> Option<&str> {
        stems.iter().find_map(|stem| self.find_image(stem))
    }

    /// The program `.nfo` of this folder. An `.nfo` named after another file in
    /// the folder (`episode.nfo` next to `episode.mp4`) belongs to that file.
    fn program_nfo(&self) -> Option<&str> {
        if let Some(name) = PROGRAM_NFO_NAMES.iter().find_map(|name| self.find(name)) {
            return Some(name);
        }

        let clip_stems: Vec<&str> = self
            .files
            .keys()
            .filter(|name| !name.ends_with(".nfo") && !has_image_extension(Path::new(name)))
            .filter_map(|name| Path::new(name).file_stem().and_then(|stem| stem.to_str()))
            .collect();

        let mut candidates: Vec<&String> = self
            .files
            .keys()
            .filter(|name| name.ends_with(".nfo"))
            .filter(|name| {
                let stem = name.trim_end_matches(".nfo");

                !clip_stems.contains(&stem)
            })
            .collect();

        candidates.sort();

        candidates
            .first()
            .and_then(|name| self.files.get(*name))
            .map(String::as_str)
    }
}

/// Title of the program a clip belongs to, from its `.nfo` files.
pub async fn program_title(storage: &Path, source: &str) -> Option<String> {
    let clip = NfoLibrary::new(storage).lookup(source).await;

    clip.program
        .and_then(|nfo| nfo.title.clone())
        .or_else(|| clip.episode.and_then(|nfo| nfo.show_title.clone()))
}

pub fn has_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            IMAGE_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(extension))
        })
}

struct ProgramNfo {
    dir: PathBuf,
    nfo: Arc<Nfo>,
    listing: Arc<Listing>,
}

/// Looks up clip metadata, caching folder listings and parsed `.nfo` files so
/// a guide with hundreds of entries reads each file only once.
pub struct NfoLibrary {
    storage: PathBuf,
    listings: HashMap<PathBuf, Arc<Listing>>,
    nfos: HashMap<PathBuf, Option<Arc<Nfo>>>,
}

impl NfoLibrary {
    pub fn new(storage: &Path) -> Self {
        Self {
            storage: storage.clean(),
            listings: HashMap::new(),
            nfos: HashMap::new(),
        }
    }

    pub async fn lookup(&mut self, source: &str) -> ClipMetadata {
        let Some(clip) = self.storage_path(source) else {
            return ClipMetadata::bare(PathBuf::from(source));
        };

        let (Some(dir), Some(stem)) = (
            clip.parent().map(Path::to_path_buf),
            clip.file_stem().and_then(|stem| stem.to_str()),
        ) else {
            return ClipMetadata::bare(clip);
        };

        let stem = stem.to_string();
        let listing = self.listing(&dir).await;
        let clip_nfo_path = listing
            .find(&format!("{stem}.nfo"))
            .map(|name| dir.join(name));
        let clip_nfo = match &clip_nfo_path {
            Some(path) => self.nfo(path).await,
            None => None,
        };
        let program = self.program_nfo(&dir).await;
        let mut metadata = ClipMetadata::bare(clip.clone());

        match program {
            Some(program) => {
                let still = match listing.find_any_image(&[&format!("{stem}-thumb"), &stem]) {
                    Some(name) => Some(Image::Local(dir.join(name))),
                    None => {
                        let thumb = clip_nfo.as_ref().and_then(|nfo| nfo.poster_thumb());
                        self.resolve_thumb(&dir, thumb).await
                    }
                };
                let poster = match program.listing.find_any_image(&POSTER_NAMES) {
                    Some(name) => Some(Image::Local(program.dir.join(name))),
                    None => {
                        self.resolve_thumb(&program.dir, program.nfo.poster_thumb())
                            .await
                    }
                };
                let backdrop = match program.listing.find_any_image(&BACKDROP_NAMES) {
                    Some(name) => Some(Image::Local(program.dir.join(name))),
                    None => {
                        self.resolve_thumb(&program.dir, program.nfo.backdrop_thumb())
                            .await
                    }
                };

                metadata.program_key = program.dir;
                metadata.program = Some(program.nfo);
                metadata.episode_key = clip_nfo.as_ref().and(clip_nfo_path);
                metadata.episode = clip_nfo;
                metadata.artwork = Artwork {
                    poster,
                    backdrop,
                    still,
                };
            }
            None => {
                // Without a program folder a clip's own `.nfo` describes the
                // program itself (a movie next to `movie-name.nfo`), unless it
                // is explicitly an episode.
                let promote = clip_nfo
                    .as_ref()
                    .is_some_and(|nfo| nfo.kind != NfoKind::Episode);
                let nfo_thumb = clip_nfo.as_ref().and_then(|nfo| nfo.poster_thumb());
                let nfo_backdrop = clip_nfo.as_ref().and_then(|nfo| nfo.backdrop_thumb());
                let poster = match listing.find_any_image(&[&format!("{stem}-poster"), &stem]) {
                    Some(name) => Some(Image::Local(dir.join(name))),
                    None if promote => self.resolve_thumb(&dir, nfo_thumb).await,
                    None => None,
                };
                let backdrop = match listing
                    .find_any_image(&[&format!("{stem}-fanart"), &format!("{stem}-landscape")])
                {
                    Some(name) => Some(Image::Local(dir.join(name))),
                    None => self.resolve_thumb(&dir, nfo_backdrop).await,
                };
                let still = match listing.find_image(&format!("{stem}-thumb")) {
                    Some(name) => Some(Image::Local(dir.join(name))),
                    None if !promote => self.resolve_thumb(&dir, nfo_thumb).await,
                    None => None,
                };

                metadata.artwork = Artwork {
                    poster,
                    backdrop,
                    still,
                };

                if promote {
                    metadata.program = clip_nfo;
                } else {
                    metadata.episode_key = clip_nfo.as_ref().and(clip_nfo_path);
                    metadata.episode = clip_nfo;
                }
            }
        }

        metadata
    }

    /// Absolute path of a playlist source inside the storage, or `None` for
    /// remote sources and files outside of it.
    fn storage_path(&self, source: &str) -> Option<PathBuf> {
        if source.contains("://") {
            return None;
        }

        let path = Path::new(source);
        let path = if path.is_absolute() {
            path.clean()
        } else {
            self.storage.join(path).clean()
        };

        (path.starts_with(&self.storage) && path != self.storage).then_some(path)
    }

    async fn program_nfo(&mut self, clip_dir: &Path) -> Option<ProgramNfo> {
        let mut dir = clip_dir.to_path_buf();

        while dir.starts_with(&self.storage) && dir != self.storage {
            let listing = self.listing(&dir).await;

            if let Some(name) = listing.program_nfo() {
                let path = dir.join(name);

                if let Some(nfo) = self.nfo(&path).await {
                    return Some(ProgramNfo { dir, nfo, listing });
                }
            }

            if !dir.pop() {
                break;
            }
        }

        None
    }

    async fn listing(&mut self, dir: &Path) -> Arc<Listing> {
        if let Some(listing) = self.listings.get(dir) {
            return listing.clone();
        }

        let mut listing = Listing::default();

        match fs::read_dir(dir).await {
            Ok(mut entries) => {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    if let Ok(name) = entry.file_name().into_string() {
                        listing.files.insert(name.to_lowercase(), name);
                    }
                }
            }
            Err(e) => debug!("XMLTV: cannot read folder {dir:?}: {e}"),
        }

        let listing = Arc::new(listing);
        self.listings.insert(dir.to_path_buf(), listing.clone());

        listing
    }

    async fn nfo(&mut self, path: &Path) -> Option<Arc<Nfo>> {
        if let Some(cached) = self.nfos.get(path) {
            return cached.clone();
        }

        let nfo = match fs::read(path).await {
            Ok(bytes) => Some(Arc::new(parse_nfo(&decode_nfo_bytes(&bytes)))),
            Err(e) => {
                warn!("XMLTV: cannot read {path:?}: {e}");
                None
            }
        };

        self.nfos.insert(path.to_path_buf(), nfo.clone());

        nfo
    }

    /// Resolve a `<thumb>` value: URLs are used as they are, paths are
    /// resolved against the folder of the `.nfo` and must be existing images
    /// inside the storage.
    async fn resolve_thumb(&self, nfo_dir: &Path, location: Option<&str>) -> Option<Image> {
        let location = location?.trim();

        if location.starts_with("https://") || location.starts_with("http://") {
            return Some(Image::Remote(location.to_string()));
        }

        let path = nfo_dir.join(location).clean();
        let is_file = fs::metadata(&path)
            .await
            .is_ok_and(|metadata| metadata.is_file());

        (path.starts_with(&self.storage) && has_image_extension(&path) && is_file)
            .then_some(Image::Local(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStorage(PathBuf);

    impl TempStorage {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("ffplayout-xmltv-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();

            Self(root)
        }

        fn file(&self, relative: &str, content: &str) -> PathBuf {
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

    #[tokio::test]
    async fn program_folder_nfo_applies_to_clips_in_season_subfolders() {
        let storage = TempStorage::new();
        storage.file(
            "Novela/programa.nfo",
            "<tvshow><title>Novela</title></tvshow>",
        );
        storage.file("Novela/poster.JPG", "");
        storage.file("Novela/fanart.png", "");
        let clip = storage.file("Novela/Temporada 1/cap01.mp4", "");
        storage.file(
            "Novela/Temporada 1/cap01.nfo",
            "<episodedetails><title>Capítulo 1</title></episodedetails>",
        );
        storage.file("Novela/Temporada 1/cap01-thumb.jpg", "");
        let other = storage.file("Novela/Temporada 1/cap02.mp4", "");
        let mut library = NfoLibrary::new(&storage.0);

        let first = library.lookup(&clip.to_string_lossy()).await;
        let second = library.lookup(&other.to_string_lossy()).await;

        assert_eq!(first.program_key, storage.0.join("Novela"));
        assert_eq!(
            first.program.unwrap().title.as_deref(),
            Some("Novela"),
            "the clip's own .nfo must not be taken as the program"
        );
        assert_eq!(first.episode.unwrap().title.as_deref(), Some("Capítulo 1"));
        assert_eq!(
            first.artwork,
            Artwork {
                poster: Some(Image::Local(storage.0.join("Novela/poster.JPG"))),
                backdrop: Some(Image::Local(storage.0.join("Novela/fanart.png"))),
                still: Some(Image::Local(
                    storage.0.join("Novela/Temporada 1/cap01-thumb.jpg")
                )),
            }
        );
        assert_eq!(second.program_key, first.program_key);
        assert!(second.episode.is_none());
        assert!(second.artwork.still.is_none());
        assert_eq!(
            program_title(&storage.0, &other.to_string_lossy())
                .await
                .as_deref(),
            Some("Novela")
        );
        assert_eq!(program_title(&storage.0, "/elsewhere/a.mp4").await, None);
    }

    #[tokio::test]
    async fn clip_nfo_becomes_the_program_without_a_program_folder() {
        let storage = TempStorage::new();
        let movie = storage.file("Filmes/O Filme.mkv", "");
        storage.file(
            "Filmes/O Filme.nfo",
            "<movie><title>O Filme</title><thumb>capa/o-filme.jpg</thumb></movie>",
        );
        storage.file("Filmes/capa/o-filme.jpg", "");
        let episode = storage.file("Avulsos/ep.mp4", "");
        storage.file(
            "Avulsos/ep.nfo",
            "<episodedetails><showtitle>Série</showtitle><title>Ep</title></episodedetails>",
        );
        let mut library = NfoLibrary::new(&storage.0);

        let movie = library.lookup(&movie.to_string_lossy()).await;
        let episode = library.lookup(&episode.to_string_lossy()).await;

        assert_eq!(movie.program.unwrap().title.as_deref(), Some("O Filme"));
        assert_eq!(movie.program_key, storage.0.join("Filmes/O Filme.mkv"));
        assert_eq!(
            movie.artwork.poster,
            Some(Image::Local(storage.0.join("Filmes/capa/o-filme.jpg")))
        );
        assert!(episode.program.is_none());
        assert_eq!(
            episode.episode.unwrap().show_title.as_deref(),
            Some("Série")
        );
    }

    #[tokio::test]
    async fn ignores_remote_sources_files_outside_storage_and_unsafe_thumbs() {
        let storage = TempStorage::new();
        storage.file(
            "Show/tvshow.nfo",
            "<tvshow><title>Show</title><thumb>../../etc/passwd</thumb></tvshow>",
        );
        let clip = storage.file("Show/a.mp4", "");
        let mut library = NfoLibrary::new(&storage.0);

        let remote = library.lookup("https://example.com/live.m3u8").await;
        let outside = library.lookup("/somewhere/else/a.mp4").await;
        let inside = library.lookup(&clip.to_string_lossy()).await;

        assert!(remote.program.is_none());
        assert!(outside.program.is_none());
        assert_eq!(inside.program.unwrap().title.as_deref(), Some("Show"));
        assert!(inside.artwork.poster.is_none());
    }

    #[test]
    fn program_nfo_prefers_known_names_and_skips_clip_nfos() {
        let listing = |names: &[&str]| Listing {
            files: names
                .iter()
                .map(|name| (name.to_lowercase(), name.to_string()))
                .collect(),
        };

        assert_eq!(
            listing(&["b.nfo", "a.nfo", "TVShow.nfo"]).program_nfo(),
            Some("TVShow.nfo")
        );
        assert_eq!(
            listing(&["ep1.mp4", "ep1.nfo", "Meu Programa.nfo", "poster.jpg"]).program_nfo(),
            Some("Meu Programa.nfo")
        );
        assert_eq!(listing(&["ep1.mp4", "ep1.nfo"]).program_nfo(), None);
    }
}
