//! Marks inside a video: stretches that keep the logo off, and ad breaks
//! signalled with SCTE-35, such as a commercial break edited into a
//! programme. They live next to the video, in `<name>.logo.json`, like its
//! `.nfo` sheet and `.vtt` subtitles, so they apply wherever the video plays:
//! the playlist, the scheduler or a preview.
//!
//! ```json
//! { "hide": [{ "from": 750.0, "to": 990.5 }], "breaks": [{ "from": 750.0, "to": 990.5 }] }
//! ```
//!
//! Times are seconds of the video itself, whatever part of it a playlist
//! plays.

use std::path::{Path, PathBuf};

use log::warn;
use noar_engine::{LogoGaps, MAX_LOGO_GAPS};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::utils::errors::ServiceError;

/// One stretch without logo, in seconds of the video.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct LogoSpan {
    pub from: f64,
    pub to: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct LogoMarks {
    #[serde(default)]
    pub hide: Vec<LogoSpan>,
    /// Ad breaks inside the video, signalled with SCTE-35 when the output
    /// carries it: the break leaves at `from` and returns at `to`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<LogoSpan>,
}

impl LogoMarks {
    /// Sorted, and refused when a stretch is empty, reversed or overlaps the
    /// next, or when there are more than the engine takes.
    pub fn validated(mut self) -> Result<Self, ServiceError> {
        self.hide = validated_spans(self.hide, "stretches without logo")?;
        self.breaks = validated_spans(self.breaks, "ad breaks")?;

        Ok(self)
    }

    pub fn is_empty(&self) -> bool {
        self.hide.is_empty() && self.breaks.is_empty()
    }

    pub fn gaps(&self) -> LogoGaps {
        LogoGaps::new(self.hide.iter().map(|span| (span.from, span.to)))
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

/// `Programa.mp4` -> `Programa.logo.json`, next to it.
pub fn marks_path(video: &Path) -> PathBuf {
    video.with_extension("logo.json")
}

/// The marks of the video at `video`; none when it has no file or the file
/// cannot be read.
pub async fn read_marks(video: &Path) -> LogoMarks {
    let path = marks_path(video);

    match fs::read_to_string(&path).await {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
            warn!("Logo marks {} cannot be read: {error}", path.display());
            LogoMarks::default()
        }),
        Err(_) => LogoMarks::default(),
    }
}

/// Write the marks of `video`; no stretches removes the file.
pub async fn write_marks(video: &Path, marks: &LogoMarks) -> Result<(), ServiceError> {
    let path = marks_path(video);

    if marks.is_empty() {
        return match fs::remove_file(&path).await {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        };
    }

    let text = serde_json::to_string_pretty(marks)
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;

    fs::write(&path, format!("{text}\n")).await?;

    Ok(())
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
pub async fn logo_gaps(storage: &Path, source: &str) -> LogoGaps {
    clip_marks(storage, source).await.gaps()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(from: f64, to: f64) -> LogoSpan {
        LogoSpan { from, to }
    }

    #[test]
    fn marks_are_sorted_and_must_make_sense() {
        let marks = LogoMarks {
            hide: vec![span(300.0, 360.0), span(60.0, 90.0)],
            breaks: Vec::new(),
        }
        .validated()
        .unwrap();

        assert_eq!(marks.hide, [span(60.0, 90.0), span(300.0, 360.0)]);
        assert!(
            LogoMarks {
                hide: vec![span(10.0, 5.0)],
                breaks: Vec::new(),
            }
            .validated()
            .is_err()
        );
        assert!(
            LogoMarks {
                hide: vec![span(10.0, 30.0), span(20.0, 40.0)],
                breaks: Vec::new(),
            }
            .validated()
            .is_err()
        );
        assert!(
            LogoMarks {
                hide: vec![span(-1.0, 5.0)],
                breaks: Vec::new(),
            }
            .validated()
            .is_err()
        );
    }

    #[test]
    fn breaks_are_placed_in_the_part_of_the_video_that_plays() {
        let marks = LogoMarks {
            hide: Vec::new(),
            breaks: vec![span(60.0, 90.0), span(290.0, 320.0), span(900.0, 960.0)],
        };

        // The whole video.
        assert_eq!(
            marks.breaks_in_clip(0.0, None),
            [(60.0, 30.0), (290.0, 30.0), (900.0, 60.0)]
        );
        // From 300 s for 10 minutes: the second break is under way, the
        // third falls after the end.
        assert_eq!(marks.breaks_in_clip(300.0, Some(600.0)), [(0.0, 20.0)]);
        // The clip ends inside a break: only the part that plays.
        assert_eq!(marks.breaks_in_clip(0.0, Some(75.0)), [(60.0, 15.0)]);
    }

    #[tokio::test]
    async fn marks_live_next_to_the_video() {
        let dir = std::env::temp_dir().join(format!("noar-logo-marks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("Programa 01.mp4");
        let marks = LogoMarks {
            hide: vec![span(750.0, 990.5)],
            breaks: Vec::new(),
        };

        assert_eq!(marks_path(&video), dir.join("Programa 01.logo.json"));
        assert!(logo_gaps(&dir, "Programa 01.mp4").await.is_empty());

        write_marks(&video, &marks).await.unwrap();
        assert_eq!(read_marks(&video).await, marks);
        assert_eq!(
            logo_gaps(&dir, "Programa 01.mp4")
                .await
                .iter()
                .collect::<Vec<_>>(),
            [(750_000, 990_500)]
        );
        assert!(logo_gaps(&dir, "srt://127.0.0.1:9000").await.is_empty());

        write_marks(&video, &LogoMarks::default()).await.unwrap();
        assert!(!marks_path(&video).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
