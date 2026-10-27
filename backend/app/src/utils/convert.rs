//! Converts uploaded videos to the channel's output format when they arrive
//! (Settings › Playlist and storage), so the playout does no scaling or
//! frame rate conversion on air, and files that do not decode cleanly are
//! rewritten from what decodes.
//!
//! One file at a time per channel, in the background. The converted file is
//! written next to the original under a hidden name and then takes its place
//! in one rename, so a playout that has the original open keeps reading it,
//! and a failed conversion leaves the original as it was.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use log::*;
use noar_engine::{ConvertTarget, convert_to_output, probe_shape, video_matches};
use serde::Serialize;
use tokio::sync::mpsc;

use crate::utils::config::{ENGINE_AUDIO_SAMPLE_RATE, PlayoutConfig};

/// Containers that hold H.264 and AAC: a converted file keeps its name.
/// Others (WebM, AVI, MXF…) become an `.mp4` of the same name.
const KEEP_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "mkv", "ts", "m2ts", "mts"];

/// How long a finished conversion stays listed.
const KEEP_FINISHED_SECONDS: u64 = 30 * 60;

/// A temporary file untouched this long is left from a conversion that
/// stopped (NoAr ended midway); a running one writes to it all the time.
const STALE_TEMPORARY: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConversionState {
    Queued,
    Converting,
    /// Converted; `path` is now the converted file.
    Done,
    /// Already in the output format: nothing done.
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Conversion {
    /// Storage path of the video, as uploaded.
    pub path: String,
    /// Storage path of the result, when it is not `path` (another extension).
    pub converted: Option<String>,
    pub state: ConversionState,
    /// Share done, from 0 to 1.
    pub progress: f64,
    pub error: Option<String>,
    /// Unix seconds of the last change.
    pub updated: u64,
}

struct Job {
    /// Absolute path of the video.
    file: PathBuf,
    /// Its storage path, for the list.
    path: String,
    storage: PathBuf,
    target: ConvertTarget,
}

/// The conversions of one channel.
#[derive(Clone)]
pub struct Conversions {
    sender: mpsc::UnboundedSender<Job>,
    list: Arc<Mutex<BTreeMap<String, Conversion>>>,
    stop: Arc<AtomicBool>,
    /// Counts the handles; the worker holds `stop`, not this.
    handles: Arc<()>,
}

impl std::fmt::Debug for Conversions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conversions").finish_non_exhaustive()
    }
}

impl Conversions {
    /// Also removes, in the background, what conversions that stopped
    /// midway left in `storage`.
    pub fn new(channel_id: i32, storage: &Path) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Job>();
        let list = Arc::new(Mutex::new(BTreeMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_list = list.clone();
        let worker_stop = stop.clone();
        let storage = storage.to_path_buf();

        tokio::task::spawn_blocking(move || {
            for file in stale_temporaries(&storage) {
                match std::fs::remove_file(&file) {
                    Ok(()) => {
                        info!(channel = channel_id; "Removed {} left by a stopped conversion", file.display());
                    }
                    Err(error) => {
                        warn!(channel = channel_id; "Cannot remove {}: {error}", file.display());
                    }
                }
            }
        });

        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let list = worker_list.clone();
                let stop = worker_stop.clone();
                let path = job.path.clone();
                let outcome =
                    tokio::task::spawn_blocking(move || run(job, &list, &stop, channel_id)).await;

                if let Err(error) = outcome {
                    error!(channel = channel_id; "Converting {path} stopped: {error}");
                    update(&worker_list, &path, |entry| {
                        entry.state = ConversionState::Failed;
                        entry.error = Some(error.to_string());
                    });
                }
            }
        });

        Self {
            sender,
            list,
            stop,
            handles: Arc::new(()),
        }
    }

    /// Queue the uploaded video at `file` (inside `storage`), if the channel
    /// converts uploads and it is a video.
    pub fn enqueue(&self, config: &PlayoutConfig, file: &Path) {
        if !config.storage.convert_uploads || !is_video(config, file) {
            return;
        }

        let Some(target) = target(config) else {
            return;
        };
        let storage = config.channel.storage.clone();
        let path = file
            .strip_prefix(&storage)
            .unwrap_or(file)
            .to_string_lossy()
            .to_string();

        self.list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                path.clone(),
                Conversion {
                    path: path.clone(),
                    converted: None,
                    state: ConversionState::Queued,
                    progress: 0.0,
                    error: None,
                    updated: now(),
                },
            );
        let _ = self.sender.send(Job {
            file: file.to_path_buf(),
            path,
            storage,
            target,
        });
    }

    /// The queued, running and recent conversions.
    pub fn list(&self) -> Vec<Conversion> {
        let mut list = self
            .list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = now();

        list.retain(|_, entry| {
            matches!(
                entry.state,
                ConversionState::Queued | ConversionState::Converting
            ) || now.saturating_sub(entry.updated) < KEEP_FINISHED_SECONDS
        });
        list.values().cloned().collect()
    }
}

impl Drop for Conversions {
    fn drop(&mut self) {
        // The last handle: stop what runs.
        if Arc::strong_count(&self.handles) == 1 {
            self.stop.store(true, Ordering::Release);
        }
    }
}

/// The output's size and frame rate, and the engine's sample rate.
pub fn target(config: &PlayoutConfig) -> Option<ConvertTarget> {
    let fps = config.output.fps;

    (fps.is_finite() && fps > 0.0 && config.output.width >= 2 && config.output.height >= 2).then(
        || ConvertTarget {
            width: config.output.width,
            height: config.output.height,
            fps: fps.round() as u32,
            sample_rate: ENGINE_AUDIO_SAMPLE_RATE,
        },
    )
}

fn is_video(config: &PlayoutConfig, file: &Path) -> bool {
    let Some(extension) = file
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    else {
        return false;
    };

    // Images and sound are not converted.
    !matches!(
        extension.as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "bmp"
            | "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "m4a"
            | "ogg"
            | "opus"
    ) && config
        .storage
        .extensions
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(&extension))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn update(
    list: &Mutex<BTreeMap<String, Conversion>>,
    path: &str,
    change: impl FnOnce(&mut Conversion),
) {
    if let Some(entry) = list
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_mut(path)
    {
        change(entry);
        entry.updated = now();
    }
}

/// Where the result goes: the same name when its container takes H.264, or
/// the same name with `.mp4`.
fn result_path(file: &Path) -> PathBuf {
    let keep = file
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            KEEP_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        });

    if keep {
        file.to_path_buf()
    } else {
        file.with_extension("mp4")
    }
}

/// Whether `file` is a conversion's temporary file: never media, the
/// playout and the storage list leave it out.
pub fn is_temporary(file: &Path) -> bool {
    file.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with('.')
                && name
                    .rsplit_once('.')
                    .is_some_and(|(rest, _)| rest.ends_with(".converting"))
        })
}

/// Temporary files under `storage` that no conversion writes to any more.
fn stale_temporaries(storage: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![storage.to_path_buf()];

    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();

            // Links are not followed: they can lead out of the storage.
            if kind.is_dir() {
                folders.push(path);
            } else if kind.is_file()
                && is_temporary(&path)
                && entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age >= STALE_TEMPORARY)
            {
                found.push(path);
            }
        }
    }

    found
}

/// The file a conversion put in place of `source`: a WebM, AVI… in a
/// playlist made before it was converted is gone, and its `.mp4` is there.
pub fn converted_in_place_of(source: &str) -> Option<String> {
    let path = Path::new(source);

    if source.is_empty() || source.contains("://") || path.is_file() {
        return None;
    }

    let result = result_path(path);

    (result != path && result.is_file()).then(|| result.to_string_lossy().to_string())
}

/// `.Name.converting.mp4` next to the result.
fn temporary_path(result: &Path) -> PathBuf {
    let name = result
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let (stem, extension) = name.rsplit_once('.').unwrap_or((&name, "mp4"));

    result.with_file_name(format!(".{stem}.converting.{extension}"))
}

fn run(job: Job, list: &Mutex<BTreeMap<String, Conversion>>, stop: &AtomicBool, channel_id: i32) {
    let path = job.path.clone();

    match probe_shape(&job.file) {
        Ok(shape) if shape.matches(&job.target) => {
            info!(channel = channel_id; "{path} is already in the output format; not converted");
            update(list, &path, |entry| {
                entry.state = ConversionState::Skipped;
                entry.progress = 1.0;
            });
            return;
        }
        Ok(_) => {}
        Err(error) => {
            warn!(channel = channel_id; "Cannot read {path} to convert it: {error:#}");
            update(list, &path, |entry| {
                entry.state = ConversionState::Failed;
                entry.error = Some(format!("{error:#}"));
            });
            return;
        }
    }

    let result = result_path(&job.file);
    let temporary = temporary_path(&result);
    let target = job.target;
    if video_matches(&job.file, &target).unwrap_or(false) {
        info!(channel = channel_id;
            "{path} has the output's picture: copying it and converting only its sound"
        );
    } else {
        info!(channel = channel_id;
            "Converting {path} to {}x{} at {} fps for the playout",
            target.width, target.height, target.fps
        );
    }
    update(list, &path, |entry| {
        entry.state = ConversionState::Converting;
    });

    let converted = convert_to_output(&job.file, &temporary, target, stop, |share| {
        update(list, &path, |entry| entry.progress = share);
    })
    .map_err(|error| format!("{error:#}"))
    .and_then(|()| std::fs::rename(&temporary, &result).map_err(|error| error.to_string()));

    // The converted file is in place: an original that stays is in the way,
    // not a failed conversion.
    if converted.is_ok()
        && result != job.file
        && let Err(error) = std::fs::remove_file(&job.file)
    {
        warn!(channel = channel_id; "{path} converted, but the original cannot be removed: {error}");
    }

    match converted {
        Ok(()) => {
            let converted_path = (result != job.file).then(|| {
                result
                    .strip_prefix(&job.storage)
                    .unwrap_or(&result)
                    .to_string_lossy()
                    .to_string()
            });
            info!(channel = channel_id; "{path} converted for the playout");
            update(list, &path, |entry| {
                entry.state = ConversionState::Done;
                entry.progress = 1.0;
                entry.converted = converted_path;
            });
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            error!(channel = channel_id; "Converting {path} failed; the original stays: {error}");
            update(list, &path, |entry| {
                entry.state = ConversionState::Failed;
                entry.error = Some(error);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn results_keep_the_name_when_the_container_takes_h264() {
        assert_eq!(result_path(Path::new("/m/a.mov")), Path::new("/m/a.mov"));
        assert_eq!(result_path(Path::new("/m/a.TS")), Path::new("/m/a.TS"));
        assert_eq!(result_path(Path::new("/m/a.webm")), Path::new("/m/a.mp4"));
        assert_eq!(
            temporary_path(Path::new("/m/Programa 01.mp4")),
            Path::new("/m/.Programa 01.converting.mp4")
        );
        assert!(is_temporary(&temporary_path(Path::new("/m/a.b.mkv"))));
        assert!(!is_temporary(Path::new("/m/a.converting.mp4")));
        assert!(!is_temporary(Path::new("/m/.hidden.mp4")));
    }

    #[test]
    fn a_converted_file_stands_in_for_its_original() {
        let dir =
            std::env::temp_dir().join(format!("noar_convert_in_place_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let webm = dir.join("Programa.webm");
        let mov = dir.join("Outro.mov");
        std::fs::write(dir.join("Programa.mp4"), b"x").unwrap();
        let path = |file: &Path| file.to_string_lossy().to_string();

        assert_eq!(
            converted_in_place_of(&path(&webm)),
            Some(path(&dir.join("Programa.mp4")))
        );
        // Still there: it plays as it is.
        std::fs::write(&webm, b"x").unwrap();
        assert_eq!(converted_in_place_of(&path(&webm)), None);
        // Containers that keep their name have nothing to stand in.
        assert_eq!(converted_in_place_of(&path(&mov)), None);
        assert_eq!(converted_in_place_of("srt://127.0.0.1:9000"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_old_temporary_files_are_stale() {
        let dir = std::env::temp_dir().join(format!("noar_convert_sweep_{}", std::process::id()));
        let nested = dir.join("folder");
        std::fs::create_dir_all(&nested).unwrap();
        let old = nested.join(".a.converting.mp4");
        let fresh = dir.join(".b.converting.mp4");
        let media = dir.join("c.mp4");
        for file in [&old, &fresh, &media] {
            std::fs::write(file, b"x").unwrap();
        }
        let long_ago = SystemTime::now() - STALE_TEMPORARY - Duration::from_secs(60);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        std::fs::File::options()
            .write(true)
            .open(&media)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();

        assert_eq!(stale_temporaries(&dir), vec![old]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
