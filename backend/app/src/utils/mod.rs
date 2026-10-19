use std::{
    env,
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
};

use chrono::{format::ParseErrorKind, prelude::*};
use chrono_tz::Tz;
use log::*;
use path_clean::PathClean;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncSeekExt},
};

use serde::{Deserialize, Deserializer, de};

pub mod args_parse;
pub mod channels;
pub mod commercial;
pub mod config;
pub mod control;
pub mod episodes;
pub mod errors;
pub mod generator;
pub mod logging;
pub mod mail;
pub mod notification;
pub mod paths;
pub mod playlist;
pub mod program;
pub mod schedule;
pub mod server;
pub mod setup;
pub mod system;
pub mod task_runner;
pub mod time_machine;
pub mod xmltv;

use crate::{
    ARGS,
    db::GLOBAL_SETTINGS,
    player::utils::time_to_sec,
    utils::{
        errors::ServiceError,
        logging::{log_file_path, remove_html, timestamps_to_timezone},
    },
};

pub fn public_path() -> PathBuf {
    let config = GLOBAL_SETTINGS.get().unwrap();
    let dev_path = env::current_dir()
        .unwrap_or_default()
        .join("frontend/.output/public/");
    let mut public_path = PathBuf::from(&config.public);

    if let Some(p) = &ARGS.public {
        // When public path is set as argument use this path for serving static files.
        // Works only when feature embed_frontend is not set.
        let public = PathBuf::from(p);

        public_path = if public.is_absolute() {
            public
        } else {
            env::current_dir().unwrap_or_default().join(public)
        }
        .clean();
    } else if cfg!(debug_assertions) && dev_path.is_dir() {
        public_path = dev_path;
    }

    public_path
}

/// Largest part of a log file read at once, unless it is downloaded.
const MAX_LOG_READ: u64 = 5_000_000;

/// Lines of a channel's log file.
#[derive(Debug, PartialEq, Eq)]
pub struct LogChunk {
    /// Complete lines from the requested offset on, timestamps in the
    /// requested time zone.
    pub text: String,
    /// Byte offset after the last complete line, where the next read starts.
    pub next: u64,
}

/// Read a channel's log of `date` (today when empty) from byte `from` on.
/// Only complete lines are returned, so a line still being written comes
/// with the next read.
pub async fn read_log_file(
    channel_id: &i32,
    date: &str,
    timezone: Tz,
    download: bool,
    from: u64,
) -> Result<LogChunk, ServiceError> {
    let date_str = if date.is_empty() {
        String::new()
    } else {
        format!("_{date}")
    };

    let log_path = log_file_path()
        .join(format!("noar_{channel_id}{date_str}.log"))
        .clean();

    // A channel that logged nothing on that day has no file: no lines.
    let size = match fs::metadata(&log_path).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(LogChunk {
                text: String::new(),
                next: 0,
            });
        }
        Err(error) => return Err(error.into()),
    };

    // Today's file starts over at midnight: an offset past its end belonged
    // to yesterday's file.
    let start = if from > size { 0 } else { from };
    let length = size - start;

    if !download && length > MAX_LOG_READ {
        error!("Log file to big: {}", sizeof_fmt(length as f64));

        return Ok(LogChunk {
            text: format!(
                "The log file is larger ({}) than the hard limit of 5MB, the probability is very high that something is wrong with the playout.\nCheck this on the server with `less {log_path:?}`.",
                sizeof_fmt(length as f64)
            ),
            next: size,
        });
    }

    let mut file = fs::File::open(&log_path).await?;
    let mut bytes = Vec::new();

    file.seek(SeekFrom::Start(start)).await?;
    file.take(length).read_to_end(&mut bytes).await?;

    if !download {
        let complete = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);

        bytes.truncate(complete);
    }

    let next = start + bytes.len() as u64;
    let content = timestamps_to_timezone(&String::from_utf8_lossy(&bytes), timezone);
    let text = if download {
        remove_html(&content)
    } else {
        content
    };

    Ok(LogChunk { text, next })
}

/// get human readable file size
pub fn sizeof_fmt(mut num: f64) -> String {
    let suffix = 'B';

    for unit in ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"] {
        if num.abs() < 1024.0 {
            return format!("{num:.1}{unit}{suffix}");
        }
        num /= 1024.0;
    }

    format!("{num:.1}Yi{suffix}")
}

pub fn naive_date_time_from_str<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;

    parse_naive_date_time(&s).map_err(de::Error::custom)
}

pub fn optional_naive_date_time_from_str<'de, D>(
    deserializer: D,
) -> Result<Option<NaiveDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;

    parse_naive_date_time(&s)
        .map(Some)
        .map_err(de::Error::custom)
}

fn parse_naive_date_time(s: &str) -> Result<NaiveDateTime, chrono::ParseError> {
    match NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        Ok(date_time) => Ok(date_time),
        Err(e) => {
            if e.kind() == ParseErrorKind::TooShort {
                NaiveDateTime::parse_from_str(&format!("{s}T00:00:00"), "%Y-%m-%dT%H:%M:%S")
            } else {
                NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%#z")
            }
        }
    }
}

pub fn round_to_nearest_ten(num: i64) -> i64 {
    if num % 10 >= 5 {
        ((num / 10) + 1) * 10
    } else {
        (num / 10) * 10
    }
}

/// Combined function to check if the program is running inside a container.
/// Returns `true` if running inside a container, otherwise `false`.
pub fn is_running_in_container() -> bool {
    // Check for Docker or Podman specific files
    if Path::new("/.dockerenv").exists() || Path::new("/run/.containerenv").exists() {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use chrono_tz::Tz;

    use super::{logging::set_log_dir, read_log_file};

    #[tokio::test]
    async fn log_reads_continue_after_the_last_complete_line() {
        let dir = std::env::temp_dir().join(format!("noar-log-read-{}", std::process::id()));
        let file = dir.join("noar_987654_2001-01-02.log");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, "first\nsecond\npart").unwrap();
        set_log_dir(dir.clone());

        let read = |from| read_log_file(&987_654, "2001-01-02", Tz::UTC, false, from);
        let chunk = read(0).await.unwrap();

        assert_eq!(chunk.text, "first\nsecond\n");
        assert_eq!(chunk.next, 13);

        std::fs::write(&file, "first\nsecond\npartial line\n").unwrap();

        assert_eq!(read(chunk.next).await.unwrap().text, "partial line\n");
        assert_eq!(
            read(500).await.unwrap().next,
            26,
            "an offset past the end starts over"
        );

        let missing = read_log_file(&987_654, "2001-01-01", Tz::UTC, false, 0)
            .await
            .unwrap();

        assert_eq!(missing.text, "");
        std::fs::remove_dir_all(dir).ok();
    }
}
