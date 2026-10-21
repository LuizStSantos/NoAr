//! Stretches of a video that keep the logo off, such as a commercial break
//! edited into a programme. They live next to the video, in
//! `<name>.logo.json`, like its `.nfo` sheet and `.vtt` subtitles, so they
//! apply wherever the video plays: the playlist, the scheduler or a preview.
//!
//! ```json
//! { "hide": [{ "from": 750.0, "to": 990.5 }] }
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
}

impl LogoMarks {
    /// Sorted, and refused when a stretch is empty, reversed or overlaps the
    /// next, or when there are more than the engine takes.
    pub fn validated(mut self) -> Result<Self, ServiceError> {
        self.hide.sort_by(|a, b| a.from.total_cmp(&b.from));

        if self.hide.len() > MAX_LOGO_GAPS {
            return Err(ServiceError::BadRequest(format!(
                "At most {MAX_LOGO_GAPS} stretches without logo"
            )));
        }

        for span in &self.hide {
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

        if self.hide.windows(2).any(|pair| pair[1].from < pair[0].to) {
            return Err(ServiceError::BadRequest(
                "Stretches without logo must not overlap".to_string(),
            ));
        }

        Ok(self)
    }

    pub fn gaps(&self) -> LogoGaps {
        LogoGaps::new(self.hide.iter().map(|span| (span.from, span.to)))
    }
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

    if marks.hide.is_empty() {
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

/// The stretches without logo for the clip at `source`, an absolute path or
/// one relative to `storage`. Remote sources have none.
pub async fn logo_gaps(storage: &Path, source: &str) -> LogoGaps {
    if source.is_empty() || source.contains("://") {
        return LogoGaps::default();
    }

    let path = Path::new(source);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        storage.join(path)
    };

    read_marks(&path).await.gaps()
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
        }
        .validated()
        .unwrap();

        assert_eq!(marks.hide, [span(60.0, 90.0), span(300.0, 360.0)]);
        assert!(
            LogoMarks {
                hide: vec![span(10.0, 5.0)]
            }
            .validated()
            .is_err()
        );
        assert!(
            LogoMarks {
                hide: vec![span(10.0, 30.0), span(20.0, 40.0)]
            }
            .validated()
            .is_err()
        );
        assert!(
            LogoMarks {
                hide: vec![span(-1.0, 5.0)]
            }
            .validated()
            .is_err()
        );
    }

    #[tokio::test]
    async fn marks_live_next_to_the_video() {
        let dir = std::env::temp_dir().join(format!("noar-logo-marks-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let video = dir.join("Programa 01.mp4");
        let marks = LogoMarks {
            hide: vec![span(750.0, 990.5)],
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
