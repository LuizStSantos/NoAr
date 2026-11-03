use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

use log::*;
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncReadExt, sync::Mutex};

use crate::{
    player::{
        controller::ChannelManager,
        utils::{
            Media, PlayoutConfig, is_remote, modified_time, normalize_live_source_timing,
            time_from_header,
        },
    },
    utils::{commercial::CommercialFolders, config::DUMMY_LEN},
};

/// This is our main playlist object, it holds all necessary information for the current day.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct JsonPlaylist {
    #[serde(default = "default_channel")]
    pub channel: String,
    pub date: String,

    #[serde(skip_serializing, skip_deserializing)]
    pub start_sec: Option<f64>,

    #[serde(skip_serializing, skip_deserializing)]
    pub length: Option<f64>,

    #[serde(skip_serializing, skip_deserializing)]
    pub path: Option<String>,

    #[serde(skip_serializing, skip_deserializing)]
    pub modified: Option<String>,

    pub program: Vec<Media>,
}

impl JsonPlaylist {
    pub fn new(date: String, start: f64) -> Self {
        let media = Media {
            begin: Some(start),
            title: None,
            duration: DUMMY_LEN,
            out: DUMMY_LEN,
            ..Media::default()
        };
        Self {
            channel: "Channel 1".into(),
            date,
            start_sec: Some(start),
            length: Some(86400.0),
            path: None,
            modified: None,
            program: vec![media],
        }
    }
}

impl PartialEq for JsonPlaylist {
    fn eq(&self, other: &Self) -> bool {
        self.channel == other.channel && self.date == other.date && self.program == other.program
    }
}

impl Eq for JsonPlaylist {}

fn default_channel() -> String {
    "Channel 1".to_string()
}

/// Lives of the storage (their `.live` file) air at their current address,
/// under their guide title, for the length the playlist gives them. A live
/// whose file cannot be read keeps its slot and fails over like a missing
/// clip.
fn resolve_lives(storage: &Path, entries: &mut [Media]) {
    use crate::utils::schedule::live::{is_live_file, live_guide_sync, read_live_url_sync};

    for entry in entries
        .iter_mut()
        .filter(|entry| is_live_file(&entry.source))
    {
        let path = if Path::new(&entry.source).is_absolute() {
            Path::new(&entry.source).to_path_buf()
        } else {
            storage.join(&entry.source)
        };

        match read_live_url_sync(&path) {
            Ok(url) => {
                if entry.title.is_none() {
                    entry.title = live_guide_sync(&path).0.or_else(|| {
                        path.parent()
                            .and_then(Path::file_name)
                            .map(|name| name.to_string_lossy().to_string())
                    });
                }
                entry.source = url;
            }
            Err(error) => error!("Live {} cannot be aired: {error}", entry.source),
        }
    }
}

pub fn set_defaults(config: &PlayoutConfig, playlist: &mut JsonPlaylist) {
    let mut start_sec = playlist.start_sec.unwrap();
    let mut length = 0.0;
    let mut commercials = CommercialFolders::new(&config.storage.path);
    // Assembled programs air as their blocks and breaks, as they are now;
    // each clip keeps the entry it came from.
    for (entry, item) in playlist.program.iter_mut().enumerate() {
        item.entry = Some((entry, 0.0));
    }
    playlist.program = crate::utils::assembled::expand_playlist(
        &config.storage.path,
        std::mem::take(&mut playlist.program),
    );
    resolve_lives(&config.storage.path, &mut playlist.program);

    let mut entry_start = (usize::MAX, 0.0);

    // Add extra values to every media clip
    for (i, item) in playlist.program.iter_mut().enumerate() {
        if let Some((entry, _)) = item.entry {
            if entry_start.0 != entry {
                entry_start = (entry, start_sec);
            }
            item.entry = Some((entry, start_sec - entry_start.1));
        }
        normalize_live_source_timing(item);
        item.begin = Some(start_sec);
        item.index = Some(i);
        item.last_ad = false;
        item.next_ad = false;
        item.skip = false;

        for path in [&mut item.source, &mut item.audio] {
            if Path::new(path).is_relative() && !path.is_empty() && !is_remote(path) {
                let new_path = config.storage.path.join(&*path);
                *path = new_path.to_string_lossy().to_string();
            }
        }

        // Clips from a commercials folder play as commercials, marked or not.
        item.ad = item.ad || commercials.contains(&item.source);

        let dur = item.out - item.seek;
        start_sec += dur;
        length += dur;
    }

    playlist.length = Some(length);
}

/// Read json playlist file, fills JsonPlaylist struct and set some extra values,
/// which we need to process.
pub async fn read_json(
    manager: &ChannelManager,
    config: &mut PlayoutConfig,
    current_list: Arc<Mutex<Vec<Media>>>,
    path: Option<String>,
    is_alive: Arc<AtomicBool>,
    date: String,
) -> JsonPlaylist {
    let id = config.general.channel_id;
    let config_clone = config.clone();
    let mut playlist_path = config.channel.playlists.clone();
    let start_sec = config.playlist.start_sec.unwrap();

    if playlist_path.is_dir() || is_remote(&config.channel.playlists.to_string_lossy()) {
        let d: Vec<&str> = date.split('-').collect();
        playlist_path = playlist_path
            .join(d[0])
            .join(d[1])
            .join(date.clone())
            .with_extension("json");
    }

    let mut current_file = playlist_path.as_path().display().to_string();

    if let Some(p) = path {
        Path::new(&p).clone_into(&mut playlist_path);
        current_file = p;
    }

    if is_remote(&current_file) {
        if let Ok(resp) = super::REMOTE_CLIENT.get(&current_file).send().await
            && resp.status().is_success()
        {
            let headers = resp.headers().clone();

            if let Ok(body) = resp.text().await {
                let mut playlist: JsonPlaylist = match serde_json::from_str(&body) {
                    Ok(p) => p,
                    Err(e) => {
                        error!(channel = id; "Could't read remote json playlist. {e:?}");
                        JsonPlaylist::new(date, start_sec)
                    }
                };

                playlist.path = Some(current_file);
                playlist.start_sec = Some(start_sec);

                if let Some(time) = time_from_header(&headers) {
                    playlist.modified = Some(time.to_string());
                }

                set_defaults(config, &mut playlist);
                let list_clone = playlist.clone();

                if !config.general.skip_validation {
                    manager
                        .spawn_validation(config_clone, current_list, list_clone, is_alive)
                        .await;
                }

                return playlist;
            }
        }
    } else if playlist_path.is_file() {
        let modified = modified_time(&current_file).await;

        let mut f = match File::options()
            .read(true)
            .write(false)
            .open(&current_file)
            .await
        {
            Ok(file) => file,
            Err(error) => {
                error!(channel = id; "Could not open playlist {current_file}: {error}");
                return JsonPlaylist::new(date, start_sec);
            }
        };
        let mut contents = String::new();
        if let Err(error) = f.read_to_string(&mut contents).await {
            error!(channel = id; "Could not read playlist {current_file}: {error}");
            return JsonPlaylist::new(date, start_sec);
        }
        let mut playlist: JsonPlaylist = match serde_json::from_str(&contents) {
            Ok(p) => p,
            Err(e) => {
                error!(channel = id; "Playlist file not readable! {e}");
                JsonPlaylist::new(date.clone(), start_sec)
            }
        };

        // catch empty program list
        if playlist.program.is_empty() {
            error!("Playlist from <span class=\"log-number\">{date}</span> is empty!");
            playlist = JsonPlaylist::new(date, start_sec);
        }

        playlist.path = Some(current_file);
        playlist.start_sec = Some(start_sec);
        playlist.modified = modified;

        set_defaults(config, &mut playlist);
        let list_clone = playlist.clone();

        if !config.general.skip_validation {
            manager
                .spawn_validation(config_clone, current_list, list_clone, is_alive)
                .await;
        }

        return playlist;
    }

    error!(channel = id; "Playlist <span class=\"log-addr\">{current_file}</span> not exist!");

    JsonPlaylist::new(date, start_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lives_air_at_their_current_address_under_their_title() {
        let storage =
            std::env::temp_dir().join(format!("noar-live-entries-{}", std::process::id()));
        let folder = storage.join("Lives/Jornal");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("signal.live"),
            "# studio\nsrt://10.0.0.5:9000?mode=caller\n",
        )
        .unwrap();
        std::fs::write(
            folder.join("tvshow.nfo"),
            "<tvshow><title>Jornal Ao Vivo</title><runtime>30</runtime></tvshow>",
        )
        .unwrap();

        let mut entries = vec![
            Media {
                source: "Lives/Jornal/signal.live".to_string(),
                out: 600.0,
                duration: 600.0,
                ..Media::default()
            },
            Media {
                source: "Lives/Gone/signal.live".to_string(),
                out: 60.0,
                ..Media::default()
            },
        ];
        resolve_lives(&storage, &mut entries);

        assert_eq!(entries[0].source, "srt://10.0.0.5:9000?mode=caller");
        assert_eq!(entries[0].title.as_deref(), Some("Jornal Ao Vivo"));
        assert_eq!(entries[0].out, 600.0);
        // An unreadable live keeps its slot.
        assert_eq!(entries[1].source, "Lives/Gone/signal.live");

        std::fs::remove_dir_all(storage).unwrap();
    }

    #[test]
    fn clips_of_a_program_count_as_their_playlist_entry() {
        let storage =
            std::env::temp_dir().join(format!("noar-program-entries-{}", std::process::id()));
        std::fs::create_dir_all(&storage).unwrap();
        std::fs::write(
            storage.join("show.program"),
            r#"{"title": "Show", "segments": [
                {"kind": "block", "source": "b1.mp4", "in": 0.0, "out": 30.0},
                {"kind": "block", "source": "b2.mp4", "in": 0.0, "out": 20.0}]}"#,
        )
        .unwrap();
        let clip = |source: &str, out: f64| Media {
            source: source.to_string(),
            out,
            duration: out,
            ..Media::default()
        };
        let mut config = PlayoutConfig::default();
        config.storage.path = storage.clone();
        let mut playlist = JsonPlaylist {
            start_sec: Some(100.0),
            program: vec![
                clip("a.mp4", 10.0),
                clip("show.program", 50.0),
                clip("c.mp4", 5.0),
            ],
            ..JsonPlaylist::new("2026-11-03".to_string(), 100.0)
        };

        set_defaults(&config, &mut playlist);

        let entries: Vec<_> = playlist.program.iter().map(|media| media.entry).collect();
        // The program airs as its two blocks, both counted as entry 1; the
        // second starts 30 s into it. The clip after it stays entry 2.
        assert_eq!(
            entries,
            [
                Some((0, 0.0)),
                Some((1, 0.0)),
                Some((1, 30.0)),
                Some((2, 0.0))
            ]
        );
        assert_eq!(playlist.program[3].index, Some(3));

        std::fs::remove_dir_all(storage).unwrap();
    }
}
