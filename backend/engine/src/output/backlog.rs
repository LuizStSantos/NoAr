//! Frames waiting for a local output (NDI, SDI) that falls behind for a
//! moment: a short stall drops frames instead of taking the channel off air,
//! and only an output that stops taking frames for [`STALL_LIMIT`] fails.

use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Result, ensure};

use crate::utils::config::OutputConfig;

/// How long a local output may take no frames before the playout fails and
/// restarts it.
pub(super) const STALL_LIMIT: Duration = Duration::from_secs(5);

/// Video a local output may fall behind by before frames are dropped: one
/// second of I420 frames, and never less than the former fixed 32 MiB.
pub(super) fn video_backlog_bytes(cfg: &OutputConfig) -> u64 {
    let frame = u64::from(cfg.width) * u64::from(cfg.height) * 3 / 2;

    (frame * u64::from(cfg.fps.max(1))).max(32 * 1024 * 1024)
}

/// What a local output has queued and not taken yet, for one kind of buffer.
pub(super) struct Backlog {
    output: &'static str,
    kind: &'static str,
    limit: u64,
    /// Since when the output is behind, and the buffers dropped since.
    behind: Mutex<Option<(Instant, u64)>>,
}

impl Backlog {
    pub(super) fn new(output: &'static str, kind: &'static str, limit: u64) -> Self {
        Self {
            output,
            kind,
            limit,
            behind: Mutex::new(None),
        }
    }

    pub(super) fn limit(&self) -> u64 {
        self.limit
    }

    /// Whether to queue the next buffer, given the bytes already queued.
    /// `false` drops it while the output catches up; an error means it has
    /// taken nothing for [`STALL_LIMIT`].
    pub(super) fn admit(&self, queued: u64) -> Result<bool> {
        let mut behind = self.behind.lock().unwrap_or_else(PoisonError::into_inner);

        if queued < self.limit {
            if let Some((since, dropped)) = behind.take() {
                log::info!(
                    "{} output caught up after {:.1} s; {dropped} {} buffers dropped",
                    self.output,
                    since.elapsed().as_secs_f64(),
                    self.kind
                );
            }
            return Ok(true);
        }

        let (since, dropped) = behind.get_or_insert_with(|| {
            log::warn!(
                "{} output is falling behind; dropping {} until it catches up",
                self.output,
                self.kind
            );
            (Instant::now(), 0)
        });
        ensure!(
            since.elapsed() < STALL_LIMIT,
            "{} output stopped consuming {}",
            self.output,
            self.kind
        );
        *dropped += 1;

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_stall_drops_buffers_and_recovers() {
        let backlog = Backlog::new("NDI", "video", 100);

        assert!(backlog.admit(10).unwrap());
        assert!(!backlog.admit(100).unwrap());
        assert!(!backlog.admit(150).unwrap());
        assert!(backlog.admit(50).unwrap());
        assert!(backlog.behind.lock().unwrap().is_none());
    }

    #[test]
    fn an_output_that_takes_nothing_for_the_limit_fails() {
        let backlog = Backlog::new("NDI", "video", 100);

        assert!(!backlog.admit(100).unwrap());
        *backlog.behind.lock().unwrap() = Some((Instant::now() - STALL_LIMIT, 150));
        let error = backlog.admit(100).unwrap_err();

        assert_eq!(error.to_string(), "NDI output stopped consuming video");
    }

    #[test]
    fn the_video_backlog_holds_a_second_of_frames() {
        let mut cfg = OutputConfig::new(1920, 1080, 30, 48_000);

        assert_eq!(video_backlog_bytes(&cfg), 1920 * 1080 * 3 / 2 * 30);
        cfg = OutputConfig::new(64, 36, 25, 48_000);
        assert_eq!(video_backlog_bytes(&cfg), 32 * 1024 * 1024);
    }
}
