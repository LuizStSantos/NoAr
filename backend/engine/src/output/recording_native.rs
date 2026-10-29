//! Recording file lifecycle; GStreamer owns all encoding and muxing.

use super::recording_retention::{ensure_minimum_free_space, remove_expired_segments};
use crate::RecordingConfig;
use anyhow::{Result, ensure};
use chrono::Local;
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(super) struct RecordingMonitor {
    config: RecordingConfig,
    next_check: Instant,
}

pub(super) fn prepare(config: &RecordingConfig) -> Result<RecordingMonitor> {
    ensure!(
        !config.path.trim().is_empty() && config.segment_duration > 0,
        "recording path and duration must be set"
    );
    fs::create_dir_all(&config.path)?;
    let mut monitor = RecordingMonitor {
        config: config.clone(),
        next_check: Instant::now(),
    };
    monitor.check()?;
    Ok(monitor)
}

pub(super) fn next_segment(config: &RecordingConfig) -> String {
    Path::new(&config.path)
        .join(format!(
            "recording-ch{}-{}-{}-{}.mkv",
            config.channel_id.unwrap_or_default(),
            Local::now().format("%Y-%m-%d_%H-%M-%S"),
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
        .to_string_lossy()
        .to_string()
}

impl RecordingMonitor {
    pub(super) fn check(&mut self) -> Result<()> {
        if Instant::now() < self.next_check {
            return Ok(());
        }
        self.next_check =
            Instant::now() + Duration::from_secs(u64::from(self.config.segment_duration));
        if self.config.retention_days > 0 {
            remove_expired_segments(
                Path::new(&self.config.path),
                self.config.retention_days,
                self.config.channel_id.unwrap_or_default(),
            )?;
        }
        ensure_minimum_free_space(
            Path::new(&self.config.path),
            self.config.minimum_free_space_gb,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_and_periodic_retention_preserve_other_channels_and_unmanaged_files() {
        let directory =
            std::env::temp_dir().join(format!("noar-native-retention-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(2 * 86400);
        let mut files = Vec::new();
        for channel in [1, 10, 2] {
            for suffix in ["2000-01-01_00-00-00-1-0", "1786617288287263738-42-000001"] {
                let path = directory.join(format!("recording-ch{channel}-{suffix}.mkv"));
                fs::File::create(&path).unwrap().set_modified(old).unwrap();
                files.push((channel, path));
            }
        }
        let archive = directory.join("archive.mkv");
        fs::File::create(&archive)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let config =
            crate::RecordingConfig::new(directory.to_string_lossy(), 60).with_channel_id(Some(1));
        let mut config = config;
        config.retention_days = 1;
        config.minimum_free_space_gb = 0;
        let mut monitor = prepare(&config).unwrap();
        for (channel, path) in &files {
            assert_eq!(path.exists(), *channel != 1);
            if *channel == 1 {
                fs::File::create(path).unwrap().set_modified(old).unwrap();
            }
        }
        monitor.next_check = Instant::now();
        monitor.check().unwrap();
        for (channel, path) in files {
            assert_eq!(path.exists(), channel != 1);
        }
        assert!(archive.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recording_rejects_an_unattainable_free_space_limit() {
        let mut config = crate::RecordingConfig::new(std::env::temp_dir().to_string_lossy(), 60);
        config.minimum_free_space_gb = u32::MAX;
        config.retention_days = 0;
        assert!(prepare(&config).is_err());
    }
}
