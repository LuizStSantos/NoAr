//! Assembled programs: one `.program` file in the storage holds a programme's
//! blocks in order and the commercial breaks between them, under the title
//! the operator chose.
//!
//! ```json
//! {
//!   "title": "Jornal da Noite — ep. 12",
//!   "segments": [
//!     { "kind": "block", "source": "Recebidos/jornal/b1.mp4", "in": 0.0, "out": 1320.5 },
//!     { "kind": "break", "clips": [{ "source": "Comerciais/a.mp4", "in": 0.0, "out": 30.0 }] },
//!     { "kind": "block", "source": "Recebidos/jornal/b2.mp4", "in": 0.0, "out": 1104.0 }
//!   ]
//! }
//! ```
//!
//! Playlists and scheduled events point to the file, so a change to it
//! applies to every airing still to come: the playout expands it into its
//! clips when it plays, and playlists read back with its current length and
//! title. The guide shows it as one programme; the as-run log lists every
//! clip. Break clips come from a commercials folder, so they play and are
//! signalled (SCTE-35) like any commercial.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    file::norm_abs_path,
    player::utils::Media,
    utils::{commercial::CommercialFolders, errors::ServiceError},
};

pub const PROGRAM_EXTENSION: &str = "program";
/// A program file is a short list; anything larger is not one.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CLIPS: usize = 500;
const MAX_TITLE_CHARS: usize = 200;
/// Longest program: a whole broadcast day.
const MAX_SECONDS: f64 = 24.0 * 3600.0;

/// A clip of a program, with the part that airs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramClip {
    /// Relative to the storage.
    pub source: String,
    #[serde(rename = "in", default)]
    pub seek: f64,
    #[serde(default)]
    pub out: f64,
}

impl ProgramClip {
    pub fn length(&self) -> f64 {
        (self.out - self.seek).max(0.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Segment {
    /// A part of the programme itself.
    Block {
        #[serde(flatten)]
        clip: ProgramClip,
    },
    /// Commercials between blocks.
    Break { clips: Vec<ProgramClip> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProgramFile {
    pub title: String,
    pub segments: Vec<Segment>,
}

impl ProgramFile {
    /// Every clip in airing order, and whether it is a commercial.
    pub fn clips(&self) -> Vec<(&ProgramClip, bool)> {
        self.segments
            .iter()
            .flat_map(|segment| match segment {
                Segment::Block { clip } => vec![(clip, false)],
                Segment::Break { clips } => clips.iter().map(|clip| (clip, true)).collect(),
            })
            .collect()
    }

    /// How long it airs, breaks included.
    pub fn duration(&self) -> f64 {
        self.clips().iter().map(|(clip, _)| clip.length()).sum()
    }
}

/// Whether a playlist or event source names a program file.
pub fn is_program(source: &str) -> bool {
    Path::new(source)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PROGRAM_EXTENSION))
}

fn parse(contents: &str) -> Result<ProgramFile, String> {
    serde_json::from_str(contents).map_err(|error| format!("not a program file: {error}"))
}

/// Reads a program file; used while a playlist is prepared for airing.
pub fn read_program_sync(path: &Path) -> Result<ProgramFile, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{} is too large for a program file",
            path.display()
        ));
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    parse(&contents)
}

pub async fn read_program(path: &Path) -> Result<ProgramFile, ServiceError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || read_program_sync(&path))
        .await
        .map_err(|error| ServiceError::Conflict(error.to_string()))?
        .map_err(ServiceError::BadRequest)
}

fn absolute(storage: &Path, source: &str) -> PathBuf {
    let path = Path::new(source);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        storage.join(path)
    }
}

/// Replaces the playlist entries that name a program file with its clips:
/// the blocks under the program's title, the break clips as commercials.
/// An entry whose file cannot be read stays, so its slot keeps its length
/// and fails over like any unreadable clip.
pub fn expand_playlist(storage: &Path, entries: Vec<Media>) -> Vec<Media> {
    let mut expanded = Vec::with_capacity(entries.len());

    for entry in entries {
        if !is_program(&entry.source) {
            expanded.push(entry);
            continue;
        }
        let program = match read_program_sync(&absolute(storage, &entry.source)) {
            Ok(program) => program,
            Err(error) => {
                log::error!("Program {} cannot be aired: {error}", entry.source);
                expanded.push(entry);
                continue;
            }
        };
        for (clip, commercial) in program.clips() {
            let mut media = entry.clone();
            media.source = clip.source.clone();
            media.audio = String::new();
            // Each clip keeps its own marks from the storage.
            media.marks = None;
            media.probe = None;
            media.probe_audio = None;
            media.seek = clip.seek;
            media.out = clip.out;
            media.duration = clip.out;
            media.ad = commercial;
            media.title = (!commercial).then(|| program.title.clone());
            expanded.push(media);
        }
    }

    expanded
}

/// Gives playlist entries that name a program file its current length and
/// title, so an edit shows in the playlist editor and the guide.
pub fn refresh_entries(storage: &Path, entries: &mut [Media]) {
    for entry in entries.iter_mut().filter(|entry| is_program(&entry.source)) {
        match read_program_sync(&absolute(storage, &entry.source)) {
            Ok(program) => {
                let length = program.duration();
                entry.seek = 0.0;
                entry.out = length;
                entry.duration = length;
                entry.title = Some(program.title);
                entry.ad = false;
            }
            Err(error) => log::warn!("Program {}: {error}", entry.source),
        }
    }
}

/// The clips a scheduled event plays: its sources, with each program file
/// replaced by its clips (relative to the storage).
pub async fn expand_sources(storage: &Path, sources: &[String]) -> Vec<String> {
    let mut expanded = Vec::with_capacity(sources.len());

    for source in sources {
        if !is_program(source) {
            expanded.push(source.clone());
            continue;
        }
        match read_program(&absolute(storage, source)).await {
            Ok(program) => expanded.extend(
                program
                    .clips()
                    .into_iter()
                    .map(|(clip, _)| clip.source.clone()),
            ),
            Err(error) => log::error!("Scheduled program {source} cannot be aired: {error:?}"),
        }
    }

    expanded
}

/// A program as the editor sends it: the clips by path; their lengths are
/// read here.
#[derive(Debug, Clone, Deserialize)]
pub struct ProgramInput {
    pub title: String,
    pub segments: Vec<SegmentInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentInput {
    Block { source: String },
    Break { clips: Vec<String> },
}

fn bad(message: impl Into<String>) -> ServiceError {
    ServiceError::BadRequest(message.into())
}

/// Checks a program and reads the part of each clip that airs (its saved
/// trim): every clip is a media file of the storage, breaks hold only
/// commercials, and it has at least one block.
pub async fn prepare_program(
    storage: &Path,
    input: ProgramInput,
) -> Result<ProgramFile, ServiceError> {
    let title = input.title.trim().to_string();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(bad(format!(
            "a program needs a title of 1 to {MAX_TITLE_CHARS} characters"
        )));
    }
    if !input
        .segments
        .iter()
        .any(|segment| matches!(segment, SegmentInput::Block { .. }))
    {
        return Err(bad("a program needs at least one block"));
    }
    let clip_count: usize = input
        .segments
        .iter()
        .map(|segment| match segment {
            SegmentInput::Block { .. } => 1,
            SegmentInput::Break { clips } => clips.len(),
        })
        .sum();
    if clip_count > MAX_CLIPS {
        return Err(bad(format!("a program holds at most {MAX_CLIPS} clips")));
    }

    let mut commercials = CommercialFolders::new(storage);
    let mut segments = Vec::with_capacity(input.segments.len());

    for segment in input.segments {
        match segment {
            SegmentInput::Block { source } => {
                let clip = read_clip(storage, &source).await?;
                if commercials.contains(&clip.source) {
                    return Err(bad(format!(
                        "'{source}' is a commercial; put it in a break"
                    )));
                }
                segments.push(Segment::Block { clip });
            }
            SegmentInput::Break { clips } => {
                if clips.is_empty() {
                    return Err(bad("a break needs at least one commercial"));
                }
                let mut read = Vec::with_capacity(clips.len());
                for source in clips {
                    let clip = read_clip(storage, &source).await?;
                    if !commercials.contains(&clip.source) {
                        return Err(bad(format!(
                            "'{source}' is not in a commercials folder; breaks hold commercials"
                        )));
                    }
                    read.push(clip);
                }
                segments.push(Segment::Break { clips: read });
            }
        }
    }

    let program = ProgramFile { title, segments };
    if program.duration() > MAX_SECONDS {
        return Err(bad("a program can run 24 hours at most"));
    }
    Ok(program)
}

async fn read_clip(storage: &Path, source: &str) -> Result<ProgramClip, ServiceError> {
    let (path, _, relative) = norm_abs_path(storage, source)?;
    if is_program(&relative) || relative.to_lowercase().ends_with(".live") {
        return Err(bad(format!("'{source}' cannot be part of a program")));
    }
    if !path.is_file() {
        return Err(bad(format!("'{source}' is not a file in the storage")));
    }
    let (seek, length) = crate::utils::schedule::trimmed_clip(&path)
        .await
        .ok_or_else(|| bad(format!("cannot read the length of '{relative}'")))?;
    Ok(ProgramClip {
        source: relative,
        seek,
        out: seek + length,
    })
}

/// Saves a program file at `path` (relative to the storage, ending in
/// `.program`) after checking it, replacing an older version.
pub async fn save_program(
    storage: &Path,
    path: &str,
    input: ProgramInput,
) -> Result<ProgramFile, ServiceError> {
    if !is_program(path) {
        return Err(bad(format!(
            "a program file name ends in .{PROGRAM_EXTENSION}"
        )));
    }
    let (target, _, _) = norm_abs_path(storage, path)?;
    let folder = target
        .parent()
        .filter(|folder| folder.is_dir())
        .ok_or_else(|| bad("the program's folder does not exist"))?;
    let program = prepare_program(storage, input).await?;
    let json = serde_json::to_string_pretty(&program)
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
    // Written beside it and renamed, so a playout never reads half a file.
    let temporary = folder.join(format!(
        ".{}.tmp",
        target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    tokio::fs::write(&temporary, json).await?;
    tokio::fs::rename(&temporary, &target).await?;
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program() -> ProgramFile {
        let clip = |source: &str, seek: f64, out: f64| ProgramClip {
            source: source.to_string(),
            seek,
            out,
        };
        ProgramFile {
            title: "Jornal".to_string(),
            segments: vec![
                Segment::Block {
                    clip: clip("show/b1.mp4", 2.0, 602.0),
                },
                Segment::Break {
                    clips: vec![clip("ads/a.mp4", 0.0, 30.0), clip("ads/b.mp4", 0.0, 15.0)],
                },
                Segment::Block {
                    clip: clip("show/b2.mp4", 0.0, 500.0),
                },
            ],
        }
    }

    #[test]
    fn the_file_format_reads_back_and_totals_its_clips() {
        let program = program();
        let json = serde_json::to_string(&program).unwrap();
        assert!(json.contains("\"kind\":\"block\""), "{json}");
        assert!(json.contains("\"in\":2.0"), "{json}");
        assert_eq!(parse(&json).unwrap(), program);
        assert_eq!(program.duration(), 600.0 + 45.0 + 500.0);
        let kinds: Vec<bool> = program.clips().iter().map(|(_, ad)| *ad).collect();
        assert_eq!(kinds, [false, true, true, false]);
        assert!(is_program("Programas/Jornal ep 12.program"));
        assert!(is_program("x.PROGRAM"));
        assert!(!is_program("x.mp4"));
    }

    #[test]
    fn playlists_air_the_clips_and_show_the_programme() {
        let storage = std::env::temp_dir().join(format!("noar-assembled-{}", std::process::id()));
        std::fs::create_dir_all(storage.join("Programas")).unwrap();
        std::fs::write(
            storage.join("Programas/Jornal.program"),
            serde_json::to_string(&program()).unwrap(),
        )
        .unwrap();

        let entry = Media {
            source: "Programas/Jornal.program".to_string(),
            out: 10.0,
            duration: 10.0,
            ..Media::default()
        };
        let other = Media {
            source: "clip.mp4".to_string(),
            out: 5.0,
            duration: 5.0,
            ..Media::default()
        };

        let mut entries = vec![entry.clone(), other.clone()];
        refresh_entries(&storage, &mut entries);
        assert_eq!(entries[0].out, 1145.0);
        assert_eq!(entries[0].title.as_deref(), Some("Jornal"));
        assert_eq!(entries[1], other);

        let expanded = expand_playlist(&storage, vec![entry, other.clone()]);
        let sources: Vec<&str> = expanded.iter().map(|m| m.source.as_str()).collect();
        assert_eq!(
            sources,
            [
                "show/b1.mp4",
                "ads/a.mp4",
                "ads/b.mp4",
                "show/b2.mp4",
                "clip.mp4"
            ]
        );
        assert_eq!((expanded[0].seek, expanded[0].out), (2.0, 602.0));
        assert!(expanded[1].ad && expanded[2].ad && !expanded[3].ad);
        assert_eq!(expanded[0].title.as_deref(), Some("Jornal"));
        assert_eq!(expanded[1].title, None);

        // A missing program stays as it was, keeping its slot.
        let missing = Media {
            source: "Programas/Gone.program".to_string(),
            out: 10.0,
            ..Media::default()
        };
        assert_eq!(
            expand_playlist(&storage, vec![missing.clone()]),
            vec![missing]
        );

        std::fs::remove_dir_all(storage).unwrap();
    }
}
