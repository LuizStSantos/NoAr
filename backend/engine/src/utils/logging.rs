//! Forward native GStreamer diagnostics to NoAr's logger without installing
//! FFmpeg or libsrt process callbacks. Asynchronous native messages use the
//! console channel; pipeline owners report operational errors with channel IDs.

use std::{
    sync::{Mutex, Once, PoisonError},
    time::{Duration, Instant},
};

use ::gstreamer as gst;
use log::{debug, error, info, trace, warn};

use super::config::LogLevel;

const NATIVE_LOG_TARGET: &str = "gstreamer";
const REPEAT_INTERVAL: Duration = Duration::from_secs(10);
const MAX_REPEAT_ENTRIES: usize = 128;

static INSTALLED: Once = Once::new();
static IGNORED_LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static REPEATS: Mutex<RepeatFilter> = Mutex::new(RepeatFilter::new());

/// The existing settings field names are retained at the application boundary.
/// GStreamer has one process-wide debug threshold, so use the more verbose of
/// the program and ingest levels. Native worker logs cannot inherit Rust TLS.
pub(crate) fn init(
    media_level: LogLevel,
    ingest_level: LogLevel,
    ignore_lines: &[String],
    _channel_id: Option<i32>,
) {
    if let Err(error) = crate::gstreamer::init() {
        error!("Cannot initialize native media logging: {error:#}");
        return;
    }

    *IGNORED_LINES.lock().unwrap_or_else(PoisonError::into_inner) = ignore_lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    let level = max_level(native_level(media_level), native_level(ingest_level));
    gst::log::set_default_threshold(level);
    gst::log::set_active(level != gst::DebugLevel::None);
    INSTALLED.call_once(|| {
        gst::log::remove_default_log_function();
        gst::log::add_log_function(
            |category, level, _file, _function, _line, _object, message| {
                let Some(message) = message.get() else {
                    return;
                };
                let message = message.trim();
                if message.is_empty() || ignored_line(message) {
                    return;
                }

                // Formatting/logging must never unwind into the native streaming thread.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let category = category.name();
                    let Some(line) = REPEATS
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .record(level, category, message, Instant::now())
                    else {
                        return;
                    };
                    write_line(level, category, &line);
                }));
            },
        );
    });
}

fn ignored_line(message: &str) -> bool {
    IGNORED_LINES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|line| message.contains(line))
}

fn native_level(level: LogLevel) -> gst::DebugLevel {
    match level {
        LogLevel::Quiet => gst::DebugLevel::None,
        LogLevel::Panic | LogLevel::Fatal | LogLevel::Error => gst::DebugLevel::Error,
        LogLevel::Warning => gst::DebugLevel::Warning,
        LogLevel::Info => gst::DebugLevel::Info,
        LogLevel::Verbose => gst::DebugLevel::Debug,
        LogLevel::Debug => gst::DebugLevel::Log,
        LogLevel::Trace => gst::DebugLevel::Trace,
    }
}

fn rank(level: gst::DebugLevel) -> u8 {
    match level {
        gst::DebugLevel::None => 0,
        gst::DebugLevel::Error => 1,
        gst::DebugLevel::Warning => 2,
        gst::DebugLevel::Fixme => 3,
        gst::DebugLevel::Info => 4,
        gst::DebugLevel::Debug => 5,
        gst::DebugLevel::Log => 6,
        gst::DebugLevel::Trace => 7,
        gst::DebugLevel::Memdump => 8,
        _ => 9,
    }
}

fn max_level(left: gst::DebugLevel, right: gst::DebugLevel) -> gst::DebugLevel {
    if rank(left) >= rank(right) {
        left
    } else {
        right
    }
}

fn write_line(level: gst::DebugLevel, category: &str, message: &str) {
    match level {
        gst::DebugLevel::None => {}
        gst::DebugLevel::Error => {
            error!(target: NATIVE_LOG_TARGET, channel = 0; "[gstreamer:{category}] {message}");
        }
        gst::DebugLevel::Warning => {
            warn!(target: NATIVE_LOG_TARGET, channel = 0; "[gstreamer:{category}] {message}");
        }
        gst::DebugLevel::Fixme | gst::DebugLevel::Info => {
            info!(target: NATIVE_LOG_TARGET, channel = 0; "[gstreamer:{category}] {message}");
        }
        gst::DebugLevel::Debug | gst::DebugLevel::Log => {
            debug!(target: NATIVE_LOG_TARGET, channel = 0; "[gstreamer:{category}] {message}");
        }
        _ => trace!(target: NATIVE_LOG_TARGET, channel = 0; "[gstreamer:{category}] {message}"),
    }
}

struct RepeatEntry {
    level: gst::DebugLevel,
    category: String,
    message: String,
    last_emitted: Instant,
    suppressed: usize,
}

struct RepeatFilter(Vec<RepeatEntry>);

impl RepeatFilter {
    const fn new() -> Self {
        Self(Vec::new())
    }

    fn record(
        &mut self,
        level: gst::DebugLevel,
        category: &str,
        message: &str,
        now: Instant,
    ) -> Option<String> {
        if let Some(entry) = self.0.iter_mut().find(|entry| {
            entry.level == level && entry.category == category && entry.message == message
        }) {
            if now.saturating_duration_since(entry.last_emitted) < REPEAT_INTERVAL {
                entry.suppressed = entry.suppressed.saturating_add(1);
                return None;
            }

            entry.last_emitted = now;
            let repeated = std::mem::take(&mut entry.suppressed);
            return Some(if repeated == 0 {
                message.to_owned()
            } else {
                format!(
                    "{message} ({repeated} repeated native messages suppressed in the last 10 s)"
                )
            });
        }

        if self.0.len() >= MAX_REPEAT_ENTRIES {
            let oldest = self
                .0
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_emitted)
                .map_or(0, |(index, _)| index);
            self.0.remove(oldest);
        }

        self.0.push(RepeatEntry {
            level,
            category: category.to_owned(),
            message: message.to_owned(),
            last_emitted: now,
            suppressed: 0,
        });
        Some(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_levels_preserve_quiet_errors_and_verbose_settings() {
        assert_eq!(native_level(LogLevel::Quiet), gst::DebugLevel::None);
        assert_eq!(native_level(LogLevel::Error), gst::DebugLevel::Error);
        assert_eq!(
            max_level(
                native_level(LogLevel::Warning),
                native_level(LogLevel::Info)
            ),
            gst::DebugLevel::Info
        );
        assert_eq!(native_level(LogLevel::Trace), gst::DebugLevel::Trace);
    }

    #[test]
    fn repeat_filter_keeps_categories_and_levels_separate() {
        let now = Instant::now();
        let mut filter = RepeatFilter::new();
        assert_eq!(
            filter
                .record(gst::DebugLevel::Warning, "srt", "socket stalled", now)
                .as_deref(),
            Some("socket stalled")
        );
        assert!(
            filter
                .record(
                    gst::DebugLevel::Warning,
                    "srt",
                    "socket stalled",
                    now + Duration::from_secs(1)
                )
                .is_none()
        );
        assert!(
            filter
                .record(gst::DebugLevel::Error, "srt", "socket stalled", now)
                .is_some()
        );
        assert!(
            filter
                .record(gst::DebugLevel::Warning, "rtmp", "socket stalled", now)
                .is_some()
        );
        assert!(
            filter
                .record(
                    gst::DebugLevel::Warning,
                    "srt",
                    "socket stalled",
                    now + REPEAT_INTERVAL
                )
                .unwrap()
                .contains("1 repeated native messages")
        );
    }

    #[test]
    fn repeat_filter_memory_remains_bounded_when_pipelines_churn() {
        let mut filter = RepeatFilter::new();
        let now = Instant::now();
        for index in 0..(MAX_REPEAT_ENTRIES * 4) {
            filter.record(
                gst::DebugLevel::Warning,
                "pipeline",
                &format!("message {index}"),
                now,
            );
        }
        assert_eq!(filter.0.len(), MAX_REPEAT_ENTRIES);
    }
}
