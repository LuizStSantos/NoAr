//! Live sources kept as files in the storage.
//!
//! A live is a folder with a `.live` file holding the source address on its
//! first line, plus the same `tvshow.nfo` and artwork as any program folder,
//! so the XMLTV guide describes it without extra configuration:
//!
//! ```text
//! Lives/
//!   Big Match/
//!     signal.live   srt://203.0.113.10:9000?mode=caller
//!     tvshow.nfo
//!     poster.jpg
//! ```

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use async_walkdir::WalkDir;
use log::info;
use quick_xml::escape::escape;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::Semaphore, task, time::timeout};
use tokio_stream::StreamExt;

use crate::{
    file::{ensure_path_within_root, norm_abs_path},
    utils::{errors::ServiceError, xmltv::nfo::parse_nfo},
};

pub const LIVE_EXTENSION: &str = "live";

/// Folder in the storage where the web interface creates lives.
pub const LIVES_FOLDER: &str = "Lives";

/// File name of the address file the web interface writes.
pub const LIVE_FILE_NAME: &str = "signal.live";

const ALLOWED_SCHEMES: [&str; 8] = [
    "srt", "rtmp", "rtmps", "http", "https", "udp", "rtsp", "rist",
];

const MAX_URL_LENGTH: usize = 2048;

/// Lives listed at most, so a huge storage cannot stall the request.
const MAX_LIVES: usize = 500;

/// How long a signal test waits for the source to answer.
const TEST_TIMEOUT: Duration = Duration::from_secs(12);

/// Signal tests running at once. The server connects where the address says,
/// so tests are few and slow enough that they cannot sweep a network.
static SIGNAL_TESTS: Semaphore = Semaphore::const_new(2);

/// What a failed test says: the same whatever went wrong, so the answers
/// tell nothing about other machines the server can reach. The cause is in
/// the server log.
const SIGNAL_FAILED: &str = "the source did not answer or could not be read";

/// Check a source address and return it trimmed.
pub fn validate_live_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    let scheme = url
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .unwrap_or_default();

    if url.len() > MAX_URL_LENGTH || url.chars().any(char::is_whitespace) {
        return Err("the address must be one line without spaces".to_string());
    }

    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return Err(format!(
            "unsupported address '{url}'; use srt://, rtmp://, rtmps://, http(s):// (HLS), udp://, rtsp:// or rist://"
        ));
    }

    Ok(url.to_string())
}

/// Short protocol name shown in the web interface.
pub fn live_protocol(url: &str) -> &'static str {
    let scheme = url
        .split_once("://")
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .unwrap_or_default();

    match scheme.as_str() {
        "srt" => "SRT",
        "rtmp" | "rtmps" => "RTMP",
        "http" | "https" => "HLS",
        "udp" => "UDP",
        "rtsp" => "RTSP",
        "rist" => "RIST",
        _ => "?",
    }
}

/// Read the address from a `.live` file: its first line that is neither
/// empty nor a `#` comment.
pub async fn read_live_url(path: &Path) -> Result<String, ServiceError> {
    let content = fs::read_to_string(path)
        .await
        .map_err(|e| ServiceError::BadRequest(format!("cannot read {}: {e}", path.display())))?;
    let line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or_else(|| ServiceError::BadRequest(format!("{} is empty", path.display())))?;

    validate_live_url(line).map_err(ServiceError::BadRequest)
}

/// A live as listed in the web interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveSource {
    /// Path of the `.live` file, relative to the storage.
    pub path: String,
    /// Folder name.
    pub name: String,
    pub url: String,
    pub protocol: String,
    /// Title from the folder's `.nfo`, if any.
    pub title: Option<String>,
}

/// Every `.live` file in the storage, by path.
pub async fn list_lives(storage: &Path) -> Vec<LiveSource> {
    let mut entries = WalkDir::new(storage);
    let mut lives = vec![];

    while let Some(Ok(entry)) = entries.next().await {
        let path = entry.path();
        let is_live = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(LIVE_EXTENSION));

        if !is_live || !path.is_file() {
            continue;
        }

        if let Some(live) = describe_live(storage, &path).await {
            lives.push(live);
        }

        if lives.len() >= MAX_LIVES {
            break;
        }
    }

    lives.sort_by(|a, b| a.path.cmp(&b.path));

    lives
}

async fn describe_live(storage: &Path, path: &Path) -> Option<LiveSource> {
    let url = read_live_url(path).await.ok()?;
    let relative = path
        .strip_prefix(storage)
        .ok()?
        .to_string_lossy()
        .to_string();
    let folder = path.parent()?;
    let name = folder
        .file_name()
        .filter(|_| folder != storage)
        .or_else(|| path.file_stem())?
        .to_string_lossy()
        .to_string();
    let title = folder_title(folder).await;

    Some(LiveSource {
        path: relative,
        name,
        protocol: live_protocol(&url).to_string(),
        url,
        title,
    })
}

async fn folder_title(folder: &Path) -> Option<String> {
    let bytes = fs::read(folder.join("tvshow.nfo")).await.ok()?;

    parse_nfo(&String::from_utf8_lossy(&bytes)).title
}

/// Guide data written to the live's `tvshow.nfo`.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct LiveGuide {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub plot: String,
    #[serde(default)]
    pub genre: String,
    /// e.g. `BR:L` or `BR:14`
    #[serde(default)]
    pub rating: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewLive {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub guide: LiveGuide,
}

/// Create (or update) the folder `Lives/<name>` with its address file and
/// guide data. Artwork is uploaded into the folder separately.
pub async fn save_live(storage: &Path, live: NewLive) -> Result<LiveSource, ServiceError> {
    let url = validate_live_url(&live.url).map_err(ServiceError::BadRequest)?;
    let name = sanitize_filename::sanitize(live.name.trim());

    if name.is_empty() || name.starts_with('.') {
        return Err(ServiceError::BadRequest(
            "the live needs a name".to_string(),
        ));
    }

    let folder = live_folder(storage, &name)?;
    let live_file = folder.join(LIVE_FILE_NAME);

    fs::create_dir_all(&folder).await?;
    fs::write(&live_file, format!("{url}\n")).await?;
    fs::write(folder.join("tvshow.nfo"), guide_nfo(&name, &live.guide)).await?;

    describe_live(storage, &live_file)
        .await
        .ok_or_else(|| ServiceError::Conflict("the live was saved but cannot be read".into()))
}

/// Change the address of an existing live, wherever its `.live` file is,
/// leaving its guide data and artwork as they are.
pub async fn update_live_address(
    storage: &Path,
    path: &str,
    url: &str,
) -> Result<LiveSource, ServiceError> {
    let url = validate_live_url(url).map_err(ServiceError::BadRequest)?;
    let (live_file, _, _) = norm_abs_path(storage, path)?;

    ensure_path_within_root(storage, &live_file)?;

    let is_live = live_file
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(LIVE_EXTENSION));

    if !is_live
        || !fs::metadata(&live_file)
            .await
            .is_ok_and(|meta| meta.is_file())
    {
        return Err(ServiceError::NotFound(format!("{path} is not a live")));
    }

    fs::write(&live_file, format!("{url}\n")).await?;

    describe_live(storage, &live_file)
        .await
        .ok_or_else(|| ServiceError::Conflict("the live was saved but cannot be read".into()))
}

fn live_folder(storage: &Path, name: &str) -> Result<PathBuf, ServiceError> {
    let (folder, _, _) = norm_abs_path(storage, &format!("{LIVES_FOLDER}/{name}"))?;

    ensure_path_within_root(storage, &folder)?;

    Ok(folder)
}

fn guide_nfo(name: &str, guide: &LiveGuide) -> String {
    let title = match guide.title.trim() {
        "" => name,
        title => title,
    };
    let mut nfo = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tvshow>\n");
    let fields = [
        ("title", title),
        ("plot", guide.plot.trim()),
        ("genre", guide.genre.trim()),
        ("mpaa", guide.rating.trim()),
    ];

    for (element, value) in fields {
        if !value.is_empty() {
            nfo.push_str(&format!("  <{element}>{}</{element}>\n", escape(value)));
        }
    }

    nfo.push_str("</tvshow>\n");

    nfo
}

/// What a signal test found at the address.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SignalTest {
    pub ok: bool,
    pub error: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub sample_rate: Option<i64>,
}

/// Open the source once and report its streams.
pub async fn test_signal(url: &str) -> Result<SignalTest, ServiceError> {
    let url = validate_live_url(url).map_err(ServiceError::BadRequest)?;
    let _permit = SIGNAL_TESTS
        .acquire()
        .await
        .map_err(|_| ServiceError::InternalServerError)?;
    let probe = task::spawn_blocking({
        let url = url.clone();
        move || noar_engine::probe_media(&url)
    });
    let failed = |cause: String| {
        info!("Signal test of {} failed: {cause}", live_protocol(&url));
        SignalTest {
            error: Some(SIGNAL_FAILED.to_string()),
            ..SignalTest::default()
        }
    };

    let probe = match timeout(TEST_TIMEOUT, probe).await {
        Err(_) => return Ok(failed("no answer".to_string())),
        Ok(Err(join_error)) => return Ok(failed(join_error.to_string())),
        Ok(Ok(Err(error))) => return Ok(failed(error.to_string())),
        Ok(Ok(Ok(probe))) => probe,
    };
    let video = probe.video.first();
    let audio = probe.audio.first();

    Ok(SignalTest {
        ok: video.is_some() || audio.is_some(),
        error: (video.is_none() && audio.is_none()).then(|| "no audio or video".to_string()),
        width: video.and_then(|v| v.width),
        height: video.and_then(|v| v.height),
        fps: video.and_then(|v| frame_rate(&v.frame_rate)),
        video_codec: video.and_then(|v| v.codec_name.clone()),
        audio_codec: audio.and_then(|a| a.codec_name.clone()),
        sample_rate: audio.and_then(|a| a.sample_rate),
    })
}

/// `25/1` or `30000/1001` as a number.
fn frame_rate(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/').unwrap_or((value, "1"));
    let numerator: f64 = numerator.trim().parse().ok()?;
    let denominator: f64 = denominator.trim().parse().ok()?;

    (denominator > 0.0 && numerator > 0.0)
        .then(|| (numerator / denominator * 100.0).round() / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStorage(PathBuf);

    impl TempStorage {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("noar-lives-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();

            Self(root)
        }
    }

    impl Drop for TempStorage {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn accepts_stream_addresses_and_rejects_others() {
        assert_eq!(
            validate_live_url("  srt://10.0.0.1:9000?mode=caller  ").unwrap(),
            "srt://10.0.0.1:9000?mode=caller"
        );
        assert!(validate_live_url("https://cdn.example/live/index.m3u8").is_ok());
        assert!(validate_live_url("file:///etc/passwd").is_err());
        assert!(validate_live_url("/var/media/clip.mp4").is_err());
        assert!(validate_live_url("rtmp://a b").is_err());
        assert_eq!(live_protocol("https://x/y.m3u8"), "HLS");
        assert_eq!(live_protocol("RTMPS://x/live"), "RTMP");
        assert_eq!(frame_rate("30000/1001"), Some(29.97));
        assert_eq!(frame_rate("0/0"), None);
    }

    #[tokio::test]
    async fn saved_lives_are_listed_with_their_guide_title() {
        let storage = TempStorage::new();
        let live = NewLive {
            name: "Big Match".to_string(),
            url: "srt://203.0.113.10:9000?mode=caller".to_string(),
            guide: LiveGuide {
                title: "Big Match — live".to_string(),
                plot: "Teams & fans".to_string(),
                genre: "Sports".to_string(),
                rating: "BR:L".to_string(),
            },
        };

        let saved = save_live(&storage.0, live).await.unwrap();
        std::fs::write(storage.0.join("broken.live"), "# nothing here\n").unwrap();
        let listed = list_lives(&storage.0).await;
        let nfo = std::fs::read_to_string(storage.0.join("Lives/Big Match/tvshow.nfo")).unwrap();

        assert_eq!(saved.path, "Lives/Big Match/signal.live");
        assert_eq!(saved.protocol, "SRT");
        assert_eq!(listed, [saved]);
        assert_eq!(listed[0].title.as_deref(), Some("Big Match — live"));
        assert!(nfo.contains("<plot>Teams &amp; fans</plot>"));
        assert!(nfo.contains("<mpaa>BR:L</mpaa>"));
    }

    #[tokio::test]
    async fn a_live_address_changes_without_touching_its_guide() {
        let storage = TempStorage::new();
        let saved = save_live(
            &storage.0,
            NewLive {
                name: "Big Match".to_string(),
                url: "srt://203.0.113.10:9000".to_string(),
                guide: LiveGuide {
                    plot: "Teams".to_string(),
                    ..LiveGuide::default()
                },
            },
        )
        .await
        .unwrap();
        let nfo = storage.0.join("Lives/Big Match/tvshow.nfo");
        let before = std::fs::read_to_string(&nfo).unwrap();

        let changed =
            update_live_address(&storage.0, &saved.path, " rtmp://203.0.113.20/live/key ")
                .await
                .unwrap();

        assert_eq!(changed.url, "rtmp://203.0.113.20/live/key");
        assert_eq!(changed.protocol, "RTMP");
        assert_eq!(std::fs::read_to_string(&nfo).unwrap(), before);
        assert!(
            update_live_address(&storage.0, "Lives/Big Match/tvshow.nfo", "srt://x:1")
                .await
                .is_err()
        );
        assert!(
            update_live_address(&storage.0, &saved.path, "file:///etc/passwd")
                .await
                .is_err()
        );
        assert!(
            update_live_address(&storage.0, "../outside.live", "srt://x:1")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn live_names_cannot_leave_the_lives_folder() {
        let storage = TempStorage::new();
        let live = |name: &str| NewLive {
            name: name.to_string(),
            url: "srt://203.0.113.10:9000".to_string(),
            guide: LiveGuide::default(),
        };

        let saved = save_live(&storage.0, live("Show/../../x")).await.unwrap();

        assert!(
            storage
                .0
                .join(&saved.path)
                .starts_with(storage.0.join(LIVES_FOLDER))
        );
        assert!(save_live(&storage.0, live("../../etc")).await.is_err());
        assert!(save_live(&storage.0, live("  ")).await.is_err());
    }
}
