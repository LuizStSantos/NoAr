//! The videos of a program folder in episode order, and which one airs next.
//!
//! Episodes are ordered by the date in their file name when they have one
//! (`show-16.10.26.mp4`, `show_2026-10-16.mkv`), since a name-based order
//! would put 01.11.26 before 16.10.26, and by natural name order otherwise.

use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use chrono::{NaiveDate, TimeDelta};
use lexical_sort::natural_lexical_cmp;
use log::*;
use regex::Regex;
use serde::Serialize;
use tokio::fs;

use crate::{
    file::norm_abs_path,
    player::utils::{file_extension, probe_media},
    utils::{
        config::PlayoutConfig,
        errors::ServiceError,
        logo_marks::{LogoSpan, read_marks},
        playlist::read_playlist,
    },
};

/// How far back the last aired episode is looked up.
const LOOKBACK_DAYS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Episode {
    /// Absolute path, as the playlist stores it.
    pub source: String,
    /// Path inside the program folder.
    pub name: String,
    pub duration: f64,
    /// The part that airs by default, when the video has a saved trim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim: Option<LogoSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LastAired {
    pub source: String,
    /// Day of the playlist it was in; `None` when it came from the caller.
    pub date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProgramEpisodes {
    pub episodes: Vec<Episode>,
    pub last: Option<LastAired>,
    /// Index in `episodes` of the one to air next.
    pub next: usize,
}

static YEAR_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])(\d{4})[-._](\d{1,2})[-._](\d{1,2})(?:[^0-9]|$)").unwrap()
});
static DAY_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])(\d{1,2})[-._](\d{1,2})[-._](\d{4}|\d{2})(?:[^0-9]|$)").unwrap()
});

/// A date written in a file name: `2026-10-16`, or day first as in Brazil
/// (`16.10.26`, `16-10-2026`).
pub fn date_in_name(name: &str) -> Option<NaiveDate> {
    let number = |text: &str| text.parse::<u32>().ok();

    if let Some(found) = YEAR_FIRST.captures(name) {
        let year = number(&found[1])? as i32;

        return NaiveDate::from_ymd_opt(year, number(&found[2])?, number(&found[3])?);
    }

    let found = DAY_FIRST.captures(name)?;
    let mut year = number(&found[3])? as i32;

    if found[3].len() == 2 {
        year += 2000;
    }

    NaiveDate::from_ymd_opt(year, number(&found[2])?, number(&found[1])?)
}

/// Episode order of two names inside a program folder.
pub fn episode_order(a: &str, b: &str) -> Ordering {
    match (date_in_name(a), date_in_name(b)) {
        (Some(first), Some(second)) => first.cmp(&second).then_with(|| natural_lexical_cmp(a, b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_lexical_cmp(a, b),
    }
}

/// The videos under `folder` (with season subfolders), in episode order.
async fn list_episodes(folder: &Path, extensions: &[String]) -> Result<Vec<PathBuf>, ServiceError> {
    let mut pending = vec![folder.to_path_buf()];
    let mut found = vec![];

    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(&dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let hidden = entry.file_name().to_string_lossy().starts_with('.');
            let kind = entry.file_type().await?;

            if hidden {
                continue;
            }

            if kind.is_dir() {
                pending.push(path);
                continue;
            }

            let extension = file_extension(&path).map(str::to_lowercase);

            if kind.is_file() && extension.is_some_and(|extension| extensions.contains(&extension))
            {
                found.push(path);
            }
        }
    }

    found.sort_by(|a, b| {
        let a = a.strip_prefix(folder).unwrap_or(a).to_string_lossy();
        let b = b.strip_prefix(folder).unwrap_or(b).to_string_lossy();

        episode_order(&a, &b)
    });

    Ok(found)
}

/// The last clip from `folder` in the saved playlists of the days before
/// `date`, newest first.
async fn last_aired_before(
    config: &PlayoutConfig,
    folder: &Path,
    date: NaiveDate,
) -> Option<LastAired> {
    for days in 1..=LOOKBACK_DAYS {
        let day = date - TimeDelta::days(days);
        let Ok(playlist) = read_playlist(config, day.format("%Y-%m-%d").to_string()).await else {
            continue;
        };

        if let Some(media) = playlist
            .program
            .iter()
            .rev()
            .find(|media| Path::new(&media.source).starts_with(folder))
        {
            return Some(LastAired {
                source: media.source.clone(),
                date: Some(day.format("%Y-%m-%d").to_string()),
            });
        }
    }

    None
}

/// The episodes of a program folder and the one to air next: the one after
/// `after` (the folder's last clip in the day being edited, when it has
/// one), else after the last one in the previous days' playlists, else the
/// first. After the last episode it starts over.
pub async fn program_episodes(
    config: &PlayoutConfig,
    folder: &str,
    date: NaiveDate,
    after: Option<&str>,
) -> Result<ProgramEpisodes, ServiceError> {
    let (folder, _, _) = norm_abs_path(&config.channel.storage, folder)?;

    if !folder.is_dir() {
        return Err(ServiceError::BadRequest(format!(
            "{} is not a folder",
            folder.display()
        )));
    }

    let mut episodes = vec![];

    for path in list_episodes(&folder, &config.storage.extensions).await? {
        let source = path.to_string_lossy().to_string();
        let duration = match probe_media(&source).await {
            Ok(probe) => probe.format.duration.unwrap_or_default(),
            Err(error) => {
                warn!(channel = config.general.channel_id; "Skip episode {source}: {error:?}");
                continue;
            }
        };

        let trim = read_marks(&path).await.trim;

        episodes.push(Episode {
            trim,
            name: path
                .strip_prefix(&folder)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string(),
            source,
            duration,
        });
    }

    let last = match after.filter(|source| Path::new(source).starts_with(&folder)) {
        Some(source) => Some(LastAired {
            source: source.to_string(),
            date: None,
        }),
        None => last_aired_before(config, &folder, date).await,
    };
    let next = last
        .as_ref()
        .and_then(|last| {
            episodes
                .iter()
                .position(|episode| episode.source == last.source)
        })
        .map_or(0, |index| (index + 1) % episodes.len().max(1));

    Ok(ProgramEpisodes {
        episodes,
        last,
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_in_file_names() {
        let date = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);

        assert_eq!(
            date_in_name("pgm-SucessoNoCampo-16.10.26.mp4"),
            date(2026, 10, 16)
        );
        assert_eq!(date_in_name("jornal_2026-10-16.mkv"), date(2026, 10, 16));
        assert_eq!(date_in_name("capitulo 03-11-2026.mp4"), date(2026, 11, 3));
        assert_eq!(date_in_name("Episódio 12.mp4"), None);
        assert_eq!(date_in_name("S01E02.mp4"), None);
        assert_eq!(date_in_name("v1.2.3.mp4"), None, "not a valid date");
    }

    #[test]
    fn dated_episodes_follow_the_calendar() {
        let mut names = vec![
            "pgm-01.11.26.mp4",
            "pgm-16.10.26.mp4",
            "extra.mp4",
            "pgm-31.10.26.mp4",
        ];

        names.sort_by(|a, b| episode_order(a, b));

        assert_eq!(
            names,
            [
                "pgm-16.10.26.mp4",
                "pgm-31.10.26.mp4",
                "pgm-01.11.26.mp4",
                "extra.mp4"
            ]
        );
    }

    #[test]
    fn other_episodes_use_the_natural_order() {
        let mut names = vec!["Ep 10.mp4", "Ep 2.mp4", "Ep 1.mp4"];

        names.sort_by(|a, b| episode_order(a, b));

        assert_eq!(names, ["Ep 1.mp4", "Ep 2.mp4", "Ep 10.mp4"]);
    }
}
