use chrono::NaiveDate;
use log::*;
use tokio::fs;

use crate::{
    file::norm_abs_path,
    player::{
        controller::ChannelManager,
        utils::{JsonPlaylist, json_reader, json_writer, playlist_file_path},
    },
    utils::{config::PlayoutConfig, errors::ServiceError, generator::playlist_generator},
};

pub async fn read_playlist(
    config: &PlayoutConfig,
    date: String,
) -> Result<JsonPlaylist, ServiceError> {
    let parsed_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d")?;
    let year = parsed_date.format("%Y").to_string();
    let month = parsed_date.format("%m").to_string();
    let date = parsed_date.format("%Y-%m-%d").to_string();

    let playlist_path = config
        .channel
        .playlists
        .join(year)
        .join(month)
        .join(date)
        .with_extension("json");

    if !fs::try_exists(&playlist_path).await? {
        return Err(ServiceError::NoContent());
    }

    match json_reader(&playlist_path).await {
        Ok(p) => Ok(p),
        Err(e) => Err(ServiceError::Conflict(e.to_string())),
    }
}

pub async fn write_playlist(
    config: &PlayoutConfig,
    json_data: JsonPlaylist,
) -> Result<String, ServiceError> {
    let date = json_data.date.clone();
    let mut playlist_path = config.channel.playlists.clone();

    if !playlist_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        playlist_path = playlist_file_path(&playlist_path, &date)
            .ok_or_else(|| ServiceError::BadRequest(format!("Invalid playlist date: {date}")))?;
    }

    let mut file_exists = false;

    if let Some(p) = playlist_path.parent() {
        fs::create_dir_all(p).await?;
    }

    if playlist_path.is_file() {
        file_exists = true;
        if let Ok(existing_data) = json_reader(&playlist_path).await
            && json_data == existing_data
        {
            return Err(ServiceError::Conflict(format!(
                "Playlist from {date}, already exists!"
            )));
        }
    }

    match json_writer(&playlist_path, json_data).await {
        Ok(..) => {
            return if file_exists {
                Ok(format!("Update playlist from {date} success!"))
            } else {
                Ok(format!("Write playlist from {date} success!"))
            };
        }
        Err(e) => {
            error!("{e}");
        }
    }

    Err(ServiceError::InternalServerError)
}

/// Generate the playlist of the date set in the channel config. With
/// `write` it is saved (and refused when the day already has one); without,
/// it is only returned.
pub async fn generate_playlist(
    manager: ChannelManager,
    shuffle_override: Option<bool>,
    write: bool,
) -> Result<JsonPlaylist, ServiceError> {
    let (template, storage_root) = {
        let mut config = manager.config.write().await;
        (
            config.general.template.take(),
            config.channel.storage.clone(),
        )
    };

    if let Some(mut template) = template {
        for source in &mut template.sources {
            let mut resolved_paths = Vec::with_capacity(source.paths.len());

            for path in &source.paths {
                let (safe_path, _, _) = norm_abs_path(&storage_root, &path.to_string_lossy())?;
                resolved_paths.push(safe_path);
            }

            source.paths = resolved_paths;
        }

        {
            let mut config = manager.config.write().await;
            config.general.template = Some(template);
        }
    }

    match playlist_generator(&manager, shuffle_override, write).await {
        Ok(playlists) => {
            if playlists.is_empty() {
                Err(ServiceError::Conflict(
                    "The playlist could not be written, maybe it already exists!".into(),
                ))
            } else {
                Ok(playlists[0].clone())
            }
        }
        Err(e) => {
            error!("{e}");
            Err(ServiceError::InternalServerError)
        }
    }
}

pub async fn delete_playlist(config: &PlayoutConfig, date: &str) -> Result<String, ServiceError> {
    let playlist_path = playlist_file_path(&config.channel.playlists, date)
        .ok_or_else(|| ServiceError::BadRequest(format!("Invalid playlist date: {date}")))?;

    if playlist_path.is_file() {
        match fs::remove_file(playlist_path).await {
            Ok(..) => Ok(format!("Delete playlist from {date} success!")),
            Err(e) => {
                error!("{e}");
                Err(ServiceError::InternalServerError)
            }
        }
    } else {
        Ok(format!("No playlist to delete on: {date}"))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A playlist folder inside a scratch root, with a sentinel file next to it.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("noar-playlist-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("playlists")).unwrap();
            std::fs::write(root.join("sentinel-x.json"), "{}").unwrap();

            Self(root)
        }

        fn config(&self) -> PlayoutConfig {
            let mut config = PlayoutConfig::default();
            config.channel.playlists = self.0.join("playlists");

            config
        }

        fn sentinel(&self) -> PathBuf {
            self.0.join("sentinel-x.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const BAD_DATES: [&str; 6] = [
        "../sentinel-x",
        "../../sentinel-x",
        "/tmp/abs-x",
        "2024",
        "",
        "2024-01-01/../../x",
    ];

    #[tokio::test]
    async fn write_playlist_rejects_dates_that_are_not_dates() {
        let scratch = Scratch::new();
        let config = scratch.config();

        for date in BAD_DATES {
            let playlist = JsonPlaylist {
                date: date.to_string(),
                ..JsonPlaylist::default()
            };
            let result = write_playlist(&config, playlist).await;

            assert!(
                matches!(result, Err(ServiceError::BadRequest(_))),
                "date {date:?}"
            );
        }

        assert_eq!(std::fs::read_to_string(scratch.sentinel()).unwrap(), "{}");
        assert!(!scratch.0.join("tmp").exists());
    }

    #[tokio::test]
    async fn delete_playlist_rejects_dates_that_are_not_dates() {
        let scratch = Scratch::new();
        let config = scratch.config();

        for date in BAD_DATES {
            let result = delete_playlist(&config, date).await;

            assert!(
                matches!(result, Err(ServiceError::BadRequest(_))),
                "date {date:?}"
            );
        }

        assert!(scratch.sentinel().is_file());
    }

    #[tokio::test]
    async fn write_playlist_stores_a_dated_playlist_under_year_and_month() {
        let scratch = Scratch::new();
        let config = scratch.config();
        let playlist = JsonPlaylist {
            date: "2024-03-07".to_string(),
            ..JsonPlaylist::default()
        };

        write_playlist(&config, playlist).await.unwrap();

        assert!(
            scratch
                .0
                .join("playlists/2024/03/2024-03-07.json")
                .is_file()
        );
        assert!(
            read_playlist(&config, "2024-03-07".to_string())
                .await
                .is_ok()
        );
    }
}
