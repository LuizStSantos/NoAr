//! Marks inside a video: its trim, stretches that keep the logo off, ad
//! breaks signalled with SCTE-35, such as a commercial break edited into a
//! programme, and stretches in the L-shape. They live next to the video, in
//! `<name>.marks.json`, like its `.nfo` sheet and `.vtt` subtitles, so they
//! apply wherever the video plays: the playlist, the scheduler or a preview.
//!
//! Before 1.5 the file was `<name>.logo.json`, when it only held logo
//! stretches. That name is still read; saving the marks writes the new one
//! and removes it.
//!
//! ```json
//! { "hide": [{ "from": 750.0, "to": 990.5 }], "breaks": [{ "from": 750.0, "to": 990.5 }] }
//! ```
//!
//! Times are seconds of the video itself, whatever part of it a playlist
//! plays.
//!
//! Videos of the same name with other extensions (`Programa.mp4` and
//! `Programa.mov`) share the file. Renaming, moving or deleting a video in
//! the storage takes its marks along, unless another video still shares them.

use std::path::{Path, PathBuf};

use log::warn;
use noar_engine::MAX_LOGO_GAPS;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::utils::errors::ServiceError;

/// One stretch without logo, in seconds of the video.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct LogoSpan {
    pub from: f64,
    pub to: f64,
}

/// What a graphics cue in a video does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CueAction {
    /// Put the template on air.
    Take,
    /// Take the template off air.
    Out,
}

/// At `at` seconds of the video, put a graphics template on air (for
/// `duration` seconds, or until taken off) or take it off.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GraphicsCue {
    pub at: f64,
    pub action: CueAction,
    pub template_id: i32,
    /// Seconds on air for a take; none or 0 stays until taken off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
}

/// At most this many graphics cues in one video.
pub const MAX_GRAPHICS_CUES: usize = 64;

/// Marks saved next to a video. Fields of older versions (logo and L-shape
/// stretches, the clip's own logo and L-shape) are no longer read: graphics
/// inside a video are now cues that put templates on and off air.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct LogoMarks {
    /// Ad breaks inside the video, signalled with SCTE-35 when the output
    /// carries it: the break leaves at `from` and returns at `to`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<LogoSpan>,
    /// The part of the video that airs by default: where it starts and ends,
    /// in seconds of the video. A playlist entry still sets its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<LogoSpan>,
    /// Graphics templates put on and off air while the video plays.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<GraphicsCue>,
    /// An ident or promo: the guide and the Player count it inside the
    /// programme before it instead of naming it. Kept here rather than in a
    /// sheet, so a clip of a series keeps using its folder's sheet.
    #[serde(default, skip_serializing_if = "is_false")]
    pub insert: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl LogoMarks {
    /// Sorted, and refused when a stretch is empty, reversed or overlaps the
    /// next, or when there are more than the engine takes.
    pub fn validated(mut self) -> Result<Self, ServiceError> {
        self.breaks = validated_spans(self.breaks, "ad breaks")?;

        if let Some(trim) = self.trim {
            validated_spans(vec![trim], "trim")?;
        }

        if self.graphics.len() > MAX_GRAPHICS_CUES {
            return Err(ServiceError::BadRequest(format!(
                "At most {MAX_GRAPHICS_CUES} graphics cues in a video"
            )));
        }

        for cue in &self.graphics {
            if !cue.at.is_finite() || cue.at < 0.0 {
                return Err(ServiceError::BadRequest(
                    "Graphics cue times are seconds from the start of the video".to_string(),
                ));
            }

            if cue.template_id <= 0 {
                return Err(ServiceError::BadRequest(
                    "Each graphics cue names a template".to_string(),
                ));
            }

            if cue.duration.is_some_and(|duration| {
                !duration.is_finite() || !(0.0..=86_400.0).contains(&duration)
            }) {
                return Err(ServiceError::BadRequest(
                    "A cue duration is in seconds, 0 (until taken off) up to a day".to_string(),
                ));
            }
        }

        self.graphics.sort_by(|a, b| a.at.total_cmp(&b.at));

        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.breaks.is_empty() && self.graphics.is_empty() && self.trim.is_none() && !self.insert
    }

    /// Where the video starts and how long it plays, after its saved trim,
    /// for a video of `duration` seconds.
    pub fn trimmed(&self, duration: f64) -> (f64, f64) {
        match self.trim {
            Some(trim) if trim.from < duration => {
                let end = trim.to.min(duration);

                (trim.from, (end - trim.from).max(0.0))
            }
            _ => (0.0, duration),
        }
    }

    /// The cues of a clip that plays from `seek`: when each comes from the
    /// start of the clip as it plays. Cues before `seek` are skipped.
    pub fn cues_in_clip(&self, seek: f64) -> Vec<(f64, GraphicsCue)> {
        self.graphics
            .iter()
            .filter(|cue| cue.at >= seek)
            .map(|cue| (cue.at - seek, cue.clone()))
            .collect()
    }

    /// The ad breaks of a clip that plays from `seek` for `duration`
    /// seconds: when each starts from the start of the clip as it plays,
    /// and how long it lasts there.
    pub fn breaks_in_clip(&self, seek: f64, duration: Option<f64>) -> Vec<(f64, f64)> {
        let end = duration.map_or(f64::INFINITY, |duration| duration.max(0.0));

        self.breaks
            .iter()
            .filter_map(|span| {
                let start = (span.from - seek).max(0.0);
                let stop = (span.to - seek).min(end);

                (stop > start && span.from - seek < end).then_some((start, stop - start))
            })
            .collect()
    }
}

fn validated_spans(mut spans: Vec<LogoSpan>, what: &str) -> Result<Vec<LogoSpan>, ServiceError> {
    spans.sort_by(|a, b| a.from.total_cmp(&b.from));

    if spans.len() > MAX_LOGO_GAPS {
        return Err(ServiceError::BadRequest(format!(
            "At most {MAX_LOGO_GAPS} {what}"
        )));
    }

    for span in &spans {
        if !span.from.is_finite() || !span.to.is_finite() || span.from < 0.0 {
            return Err(ServiceError::BadRequest(
                "Times must be seconds from the start of the video".to_string(),
            ));
        }

        if span.to <= span.from {
            return Err(ServiceError::BadRequest(
                "Each stretch must end after it starts".to_string(),
            ));
        }
    }

    if spans.windows(2).any(|pair| pair[1].from < pair[0].to) {
        return Err(ServiceError::BadRequest(format!(
            "The {what} must not overlap"
        )));
    }

    Ok(spans)
}

/// Ending of the marks file of a video.
pub const MARKS_SUFFIX: &str = ".marks.json";

/// Its name before 1.5, still read.
pub const LEGACY_MARKS_SUFFIX: &str = ".logo.json";

/// `Programa.mp4` -> `Programa.marks.json`, next to it.
pub fn marks_path(video: &Path) -> PathBuf {
    video.with_extension("marks.json")
}

/// `Programa.mp4` -> `Programa.logo.json`, the name before 1.5.
fn legacy_marks_path(video: &Path) -> PathBuf {
    video.with_extension("logo.json")
}

/// The marks file the video has: the new name, else the old one.
async fn existing_marks(video: &Path) -> Option<PathBuf> {
    for path in [marks_path(video), legacy_marks_path(video)] {
        if fs::try_exists(&path).await.unwrap_or(false) {
            return Some(path);
        }
    }

    None
}

/// The marks of the video at `video`; none when it has no file or the file
/// cannot be read.
pub async fn read_marks(video: &Path) -> LogoMarks {
    let Some(path) = existing_marks(video).await else {
        return LogoMarks::default();
    };

    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
            warn!("Logo marks {} cannot be read: {error}", path.display());
            LogoMarks::default()
        }),
        Err(_) => LogoMarks::default(),
    }
}

/// [`read_marks`] for code that prepares a playlist outside async code.
pub fn read_marks_sync(video: &Path) -> LogoMarks {
    let Some(path) = [marks_path(video), legacy_marks_path(video)]
        .into_iter()
        .find(|path| path.is_file())
    else {
        return LogoMarks::default();
    };

    std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remove `path`; one that is not there is fine.
async fn remove_if_there(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path).await {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Write the marks of `video` under the current name, removing a file of the
/// old name; no marks removes both.
pub async fn write_marks(video: &Path, marks: &LogoMarks) -> Result<(), ServiceError> {
    let path = marks_path(video);

    if marks.is_empty() {
        remove_if_there(&path).await?;
        remove_if_there(&legacy_marks_path(video)).await?;

        return Ok(());
    }

    let text = serde_json::to_string_pretty(marks)
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
    // Written whole, then put in place: a playout reading the marks never
    // sees half a file.
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = path.with_file_name(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));

    fs::write(&temporary, format!("{text}\n")).await?;
    if let Err(error) = fs::rename(&temporary, &path).await {
        let _ = fs::remove_file(&temporary).await;
        return Err(error.into());
    }

    // The old name would only come back if the new file went.
    if let Err(error) = remove_if_there(&legacy_marks_path(video)).await {
        warn!("Old marks file of {} not removed: {error}", video.display());
    }

    Ok(())
}

/// Whether another file in the folder of `video` shares its marks.
async fn marks_shared(video: &Path) -> bool {
    let marks = marks_path(video);
    let legacy = legacy_marks_path(video);
    let Some(folder) = video.parent() else {
        return false;
    };
    let Ok(mut entries) = fs::read_dir(folder).await else {
        return true;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();

        if path != video && path != marks && path != legacy && marks_path(&path) == marks {
            return true;
        }
    }

    false
}

/// After `source` was renamed or moved to `target`: take its marks along.
/// The file move stands even when this fails, which is logged.
pub async fn follow_move(source: &Path, target: &Path) {
    let Some(from) = existing_marks(source).await else {
        return;
    };
    // Under the current name at the new place.
    let to = marks_path(target);

    if from == to {
        return;
    }

    if fs::try_exists(&to).await.unwrap_or(true) {
        warn!(
            "Logo marks {} kept: {} already exists",
            from.display(),
            to.display()
        );
        return;
    }

    let moved = if marks_shared(source).await {
        fs::copy(&from, &to).await.map(|_| ())
    } else {
        fs::rename(&from, &to).await
    };

    if let Err(error) = moved {
        warn!("Logo marks {} not moved: {error}", from.display());
    }
}

/// After `video` was deleted: delete its marks, unless another video shares them.
pub async fn follow_remove(video: &Path) {
    if marks_shared(video).await {
        return;
    }

    for marks in [marks_path(video), legacy_marks_path(video)] {
        if let Err(error) = remove_if_there(&marks).await {
            warn!("Logo marks {} not deleted: {error}", marks.display());
        }
    }
}

/// The marks of the clip at `source`, an absolute path or one relative to
/// `storage`. Remote sources have none.
pub async fn clip_marks(storage: &Path, source: &str) -> LogoMarks {
    if source.is_empty() || source.contains("://") {
        return LogoMarks::default();
    }

    let path = Path::new(source);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        storage.join(path)
    };

    read_marks(&path).await
}

/// The stretches without logo for the clip at `source`.
#[cfg(test)]
mod tests {
    use super::*;

    fn span(from: f64, to: f64) -> LogoSpan {
        LogoSpan { from, to }
    }

    fn cue(at: f64, action: CueAction, template_id: i32) -> GraphicsCue {
        GraphicsCue {
            at,
            action,
            template_id,
            duration: None,
        }
    }

    #[test]
    fn an_ident_mark_alone_is_kept_and_read_back() {
        let marks: LogoMarks = serde_json::from_str(r#"{"insert": true}"#).unwrap();

        assert!(marks.insert);
        assert!(
            !marks.is_empty(),
            "a file holding only the mark must be kept"
        );
        assert_eq!(serde_json::to_string(&LogoMarks::default()).unwrap(), "{}");
    }

    #[test]
    fn older_logo_and_lshape_marks_are_no_longer_read() {
        let marks: LogoMarks = serde_json::from_str(
            r#"{"hide": [{"from": 1, "to": 2}], "lshape": [{"from": 3, "to": 4}],
                "logo_image": "a.png", "trim": {"from": 5, "to": 50}}"#,
        )
        .unwrap();

        assert_eq!(marks.trim, Some(span(5.0, 50.0)));
        assert!(marks.graphics.is_empty());
        assert_eq!(
            serde_json::to_value(&marks).unwrap(),
            serde_json::json!({"trim": {"from": 5.0, "to": 50.0}})
        );
    }

    #[test]
    fn graphics_cues_are_sorted_checked_and_placed_in_the_clip() {
        let marks = LogoMarks {
            graphics: vec![
                GraphicsCue {
                    duration: Some(50.0),
                    ..cue(120.0, CueAction::Take, 3)
                },
                cue(30.0, CueAction::Out, 1),
                cue(90.0, CueAction::Take, 1),
            ],
            ..LogoMarks::default()
        }
        .validated()
        .unwrap();

        assert_eq!(
            marks.graphics.iter().map(|cue| cue.at).collect::<Vec<_>>(),
            [30.0, 90.0, 120.0]
        );
        // Played from 60 s: the cue at 30 s is past, the others shift.
        let placed = marks.cues_in_clip(60.0);
        assert_eq!(
            placed
                .iter()
                .map(|(at, cue)| (*at, cue.template_id))
                .collect::<Vec<_>>(),
            [(30.0, 1), (60.0, 3)]
        );

        for bad in [
            cue(-1.0, CueAction::Take, 1),
            cue(f64::NAN, CueAction::Take, 1),
            cue(1.0, CueAction::Take, 0),
            GraphicsCue {
                duration: Some(-5.0),
                ..cue(1.0, CueAction::Take, 1)
            },
        ] {
            assert!(
                LogoMarks {
                    graphics: vec![bad],
                    ..LogoMarks::default()
                }
                .validated()
                .is_err()
            );
        }

        assert!(
            LogoMarks {
                graphics: (0..=MAX_GRAPHICS_CUES as i32)
                    .map(|index| cue(f64::from(index), CueAction::Take, 1))
                    .collect(),
                ..LogoMarks::default()
            }
            .validated()
            .is_err()
        );
    }

    #[test]
    fn breaks_are_sorted_must_make_sense_and_are_placed_in_the_part_that_plays() {
        let marks = LogoMarks {
            breaks: vec![span(290.0, 320.0), span(60.0, 90.0), span(900.0, 960.0)],
            ..LogoMarks::default()
        }
        .validated()
        .unwrap();

        assert_eq!(
            marks.breaks_in_clip(0.0, None),
            [(60.0, 30.0), (290.0, 30.0), (900.0, 60.0)]
        );
        assert_eq!(marks.breaks_in_clip(300.0, Some(600.0)), [(0.0, 20.0)]);
        assert_eq!(marks.breaks_in_clip(0.0, Some(75.0)), [(60.0, 15.0)]);

        for spans in [
            vec![span(10.0, 5.0)],
            vec![span(10.0, 30.0), span(20.0, 40.0)],
            vec![span(-1.0, 5.0)],
        ] {
            assert!(
                LogoMarks {
                    breaks: spans,
                    ..LogoMarks::default()
                }
                .validated()
                .is_err()
            );
        }
    }

    #[test]
    fn a_saved_trim_sets_where_the_video_starts_and_how_long_it_plays() {
        let mut marks = LogoMarks::default();
        assert_eq!(marks.trimmed(600.0), (0.0, 600.0));

        marks.trim = Some(span(12.5, 590.0));
        assert_eq!(marks.trimmed(600.0), (12.5, 577.5));
        assert_eq!(marks.trimmed(300.0), (12.5, 287.5));
        assert_eq!(marks.trimmed(10.0), (0.0, 10.0));

        assert!(
            LogoMarks {
                trim: Some(span(30.0, 10.0)),
                ..LogoMarks::default()
            }
            .validated()
            .is_err()
        );
        assert!(!marks.is_empty());
    }

    #[tokio::test]
    async fn marks_live_next_to_the_video() {
        let dir = std::env::temp_dir().join(format!("noar-logo-marks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("Programa 01.mp4");
        let marks = LogoMarks {
            graphics: vec![cue(750.0, CueAction::Take, 2)],
            ..LogoMarks::default()
        };

        assert_eq!(marks_path(&video), dir.join("Programa 01.marks.json"));
        assert!(clip_marks(&dir, "Programa 01.mp4").await.is_empty());

        write_marks(&video, &marks).await.unwrap();
        assert_eq!(read_marks(&video).await, marks);
        assert_eq!(clip_marks(&dir, "Programa 01.mp4").await, marks);
        assert!(clip_marks(&dir, "srt://127.0.0.1:9000").await.is_empty());

        write_marks(&video, &LogoMarks::default()).await.unwrap();
        assert!(!marks_path(&video).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn marks_of_the_old_name_are_read_and_replaced_on_save() {
        let dir = std::env::temp_dir().join(format!("noar-logo-legacy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("Programa.mp4");
        std::fs::write(&video, b"x").unwrap();
        std::fs::write(
            dir.join("Programa.logo.json"),
            r#"{ "breaks": [{ "from": 1.0, "to": 2.0 }] }"#,
        )
        .unwrap();

        let marks = read_marks(&video).await;
        assert_eq!(marks.breaks, [span(1.0, 2.0)]);

        let moved = dir.join("Outro.mp4");
        std::fs::rename(&video, &moved).unwrap();
        follow_move(&video, &moved).await;
        assert!(dir.join("Outro.marks.json").exists());
        assert!(!dir.join("Programa.logo.json").exists());

        std::fs::write(dir.join("Outro.logo.json"), "{}").unwrap();
        write_marks(&moved, &marks).await.unwrap();
        assert!(!dir.join("Outro.logo.json").exists());
        assert_eq!(read_marks(&moved).await.breaks, [span(1.0, 2.0)]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn marks_follow_the_video_unless_another_shares_them() {
        let dir = std::env::temp_dir().join(format!("noar-logo-follow-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("folder")).unwrap();
        let marks = LogoMarks {
            breaks: vec![span(1.0, 2.0)],
            ..LogoMarks::default()
        };
        let video = dir.join("A.mp4");
        std::fs::write(&video, b"x").unwrap();
        write_marks(&video, &marks).await.unwrap();

        let renamed = dir.join("B.mp4");
        std::fs::rename(&video, &renamed).unwrap();
        follow_move(&video, &renamed).await;
        assert!(!marks_path(&video).exists());
        assert_eq!(read_marks(&renamed).await, marks);

        std::fs::write(dir.join("B.mov"), b"x").unwrap();
        let moved = dir.join("folder").join("B.mp4");
        std::fs::rename(&renamed, &moved).unwrap();
        follow_move(&renamed, &moved).await;
        assert_eq!(read_marks(&moved).await, marks);
        assert_eq!(read_marks(&dir.join("B.mov")).await, marks);

        std::fs::write(dir.join("folder").join("B.mkv"), b"x").unwrap();
        std::fs::remove_file(&moved).unwrap();
        follow_remove(&moved).await;
        assert!(marks_path(&moved).exists());
        std::fs::remove_file(dir.join("folder").join("B.mkv")).unwrap();
        follow_remove(&dir.join("folder").join("B.mkv")).await;
        assert!(!marks_path(&moved).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
