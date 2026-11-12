//! Which day's playlist a channel airs across a day change, with the real
//! player.
//!
//! Each test starts the playout shortly before a day ends, with the clock set
//! to that moment, lets it run past the day start and checks which playlist
//! it reads then. The clock is the process-wide one of `--fake-time`, so the
//! tests run one at a time, in real time.
//!
//! The last tests ask the playlist for its entries without a player, to put
//! the clock where a playout cannot be made to be: past the day start with
//! the day that ended still loaded, or while a playlist is being read.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use serial_test::serial;
use sqlx::sqlite::SqlitePoolOptions;

use noar::{
    db::handles,
    player::{
        controller::ChannelManager,
        input::{SourceIterator, source_generator},
        output::player,
    },
    utils::{
        config::{PlayoutConfig, ProcessMode::Playlist},
        system::SystemStat,
        time_machine::set_mock_time,
    },
};

/// The folder one test's HLS output is written to, removed with the test.
struct OutputFolder(PathBuf);

impl OutputFolder {
    fn new() -> Self {
        let folder = std::env::temp_dir().join(format!("noar-playout-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&folder).expect("test output folder should be created");

        Self(folder)
    }
}

impl Drop for OutputFolder {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

async fn prepare_config() -> (PlayoutConfig, ChannelManager, OutputFolder) {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    handles::db_migrate(&pool).await.unwrap();

    let current_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets")
        .canonicalize()
        .unwrap();
    let output = OutputFolder::new();
    let log = current_path.join("log");
    let playlists = current_path.join("playlists");
    let storage = current_path.join("storage");
    let filler = current_path.join("storage/media_filler/filler_0.mp4");

    // The times of these tests are Central European: without a time zone of
    // its own, the channel would use the one of the machine that runs them.
    // HLS needs no receiver and no device; it is found by its name, since the
    // ids of the outputs follow the migrations.
    sqlx::query(
        r#"
        UPDATE config_global SET public = $1, logs = $2, playlists = $3, storage = $4;
        UPDATE channels SET public = $1, playlists = $3, storage = $4, timezone = 'Europe/Berlin';
        UPDATE config_storage SET filler = $5;
        UPDATE config_output SET width = 1024, height = 576, active = (name = 'hls');
        "#,
    )
    .bind(output.0.to_string_lossy())
    .bind(log.to_string_lossy())
    .bind(playlists.to_string_lossy())
    .bind(storage.to_string_lossy())
    .bind(filler.to_string_lossy())
    .execute(&pool)
    .await
    .unwrap();

    let mut config = PlayoutConfig::new(&pool, 1, None).await.unwrap();
    let channel = handles::select_channel(&pool, &1).await.unwrap();

    config.general.skip_validation = true;
    config.mail.recipient = "".into();
    config.processing.mode = Playlist;

    let manager = ChannelManager::new(
        pool,
        channel,
        config.clone(),
        tokio_util::sync::CancellationToken::new(),
        SystemStat::new(),
    )
    .await
    .expect("test storage should initialize");

    (config, manager, output)
}

async fn timed_stop(sec: u64, manager: ChannelManager) {
    tokio::time::sleep(tokio::time::Duration::from_secs(sec)).await;

    println!("Timed stop of process, after {sec:?} seconds");

    manager.channel.lock().await.active = false;
    manager.stop_all(false).await;
}

/// A channel about to start at `fake_time`, with a 24-hour day starting at
/// `start_sec` (`day_start` as the settings spell it).
async fn channel_at(
    day_start: &str,
    start_sec: f64,
    fake_time: &str,
) -> (ChannelManager, OutputFolder) {
    let (mut config, manager, output) = prepare_config().await;

    config.playlist.day_start = day_start.into();
    config.playlist.start_sec = Some(start_sec);
    config.playlist.length = "24:00:00".into();
    config.playlist.length_sec = Some(86400.0);

    manager.update_config(config).await;

    manager.is_alive.store(true, Ordering::SeqCst);
    manager.list_init.store(true, Ordering::SeqCst);

    set_mock_time(&Some(fake_time.to_string())).unwrap();

    (manager, output)
}

/// Play for `seconds` from `fake_time` and return the day of the playlist
/// the channel reads at the end.
async fn playlist_date_after(
    day_start: &str,
    start_sec: f64,
    fake_time: &str,
    seconds: u64,
) -> String {
    let (manager, _output) = channel_at(day_start, start_sec, fake_time).await;

    tokio::spawn(timed_stop(seconds, manager.clone()));

    // A playout that cannot start (an output that is not there) would leave
    // the date as it was and pass for a day that did not change.
    player(manager.clone())
        .await
        .expect("the playout should run until it is stopped");

    manager.current_date.lock().await.clone()
}

#[tokio::test]
#[serial]
async fn playlist_missing() {
    let date = playlist_date_after("00:00:00", 0.0, "2023-02-07T23:59:45+01:00", 17).await;

    assert_eq!(date, "2023-02-08");
}

#[tokio::test]
#[serial]
async fn playlist_next_missing() {
    let date = playlist_date_after("00:00:00", 0.0, "2023-02-09T23:59:45+01:00", 17).await;

    assert_eq!(date, "2023-02-10");
}

#[tokio::test]
#[serial]
async fn playlist_to_short() {
    let date = playlist_date_after("06:00:00", 21600.0, "2024-01-31T05:59:40+01:00", 22).await;

    assert_eq!(date, "2024-01-31");
}

#[tokio::test]
#[serial]
async fn playlist_to_short_last_out() {
    let date = playlist_date_after("06:00:00", 21600.0, "2025-10-06T05:59:25+02:00", 37).await;

    assert_eq!(date, "2025-10-06");
}

#[tokio::test]
#[serial]
async fn playlist_init_after_list_end() {
    let date = playlist_date_after("06:00:00", 21600.0, "2024-01-31T05:59:47+01:00", 15).await;

    assert_eq!(date, "2024-01-31");
}

#[tokio::test]
#[serial]
async fn playlist_change_at_midnight() {
    let date = playlist_date_after("00:00:00", 0.0, "2023-02-08T23:59:45+01:00", 17).await;

    assert_eq!(date, "2023-02-09");
}

/// The day still starts at midnight (`start_sec`): nothing changes before it.
#[tokio::test]
#[serial]
async fn playlist_change_before_midnight() {
    let date = playlist_date_after("23:59:45", 0.0, "2023-02-09T23:59:30+01:00", 17).await;

    assert_eq!(date, "2023-02-09");
}

#[tokio::test]
#[serial]
async fn playlist_change_at_six() {
    let date = playlist_date_after("06:00:00", 21600.0, "2023-02-09T05:59:45+01:00", 17).await;

    assert_eq!(date, "2023-02-09");
}

/// The next entry of the playlist, as the play loop asks for it.
async fn next_entry(playlist: &mut SourceIterator) {
    playlist
        .next()
        .await
        .expect("the playlist should give an entry");
}

/// The filler that fills a short day runs over the day start, by more than
/// the two seconds in which a day change counts as close. The entry after a
/// filler is an init, which seeks by the clock: it must seek in the day that
/// began. In the playlist of the day that ended, still loaded, it would air
/// that day again from its top.
#[tokio::test]
#[serial]
async fn an_init_after_the_day_start_leaves_the_day_that_ended() {
    let fake_time = |time: &str| set_mock_time(&Some(time.to_string())).unwrap();
    let (manager, _output) = channel_at("06:00:00", 21600.0, "2024-01-31T05:59:40+01:00").await;
    let mut playlist = source_generator(manager.clone()).await;

    // The last clip of the 30th, then the filler up to the day start.
    next_entry(&mut playlist).await;
    fake_time("2024-01-31T05:59:47+01:00");
    next_entry(&mut playlist).await;
    assert_eq!(*manager.current_date.lock().await, "2024-01-30");
    assert!(manager.list_init.load(Ordering::SeqCst));

    fake_time("2024-01-31T06:00:05+01:00");
    next_entry(&mut playlist).await;

    assert_eq!(*manager.current_date.lock().await, "2024-01-31");
}

/// A playlist server on this machine that serves `tests_assets/playlists`.
/// When the playlist of `slow_day` is asked for the first time, it sets the
/// clock to `then` before it answers: the clock moves while that playlist is
/// being read, whatever the reading takes.
fn playlist_server(slow_day: &'static str, then: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a free port on this machine");
    let address = format!("http://{}", listener.local_addr().unwrap());
    let playlists = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/playlists");
    let moved = Arc::new(AtomicBool::new(false));

    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());

            if reader.read_line(&mut request).is_err() {
                continue;
            }

            // The rest of the request is headers, up to an empty line.
            let mut line = String::new();
            while reader.read_line(&mut line).is_ok_and(|read| read > 2) {
                line.clear();
            }

            let mut parts = request.split_whitespace();
            let method = parts.next().unwrap_or_default();
            let path = parts.next().unwrap_or_default().trim_start_matches('/');

            if path.ends_with(&format!("{slow_day}.json")) && !moved.swap(true, Ordering::SeqCst) {
                set_mock_time(&Some(then.to_string())).unwrap();
            }

            let answer = match fs::read(playlists.join(path)) {
                Ok(body) if method == "GET" => [
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes(),
                    body,
                ]
                .concat(),
                Ok(_) => b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                Err(_) => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
            };
            let _ = stream.write_all(&answer);
        }
    });

    address
}

/// The day start passes while an init reads the playlist of the day that is
/// ending, as with a long playlist or one on a slow server. The init must not
/// seek in that playlist, now past its day: it would air that day again from
/// its top. It reads the day that began instead.
#[tokio::test]
#[serial]
async fn a_day_start_that_passes_during_an_init_is_not_missed() {
    let (manager, _output) = channel_at("06:00:00", 21600.0, "2024-01-31T05:59:58.500+01:00").await;
    let mut config = manager.config.read().await.clone();

    // Set here, not in the database: a playout configuration makes the
    // folder of a local playlist path, and this one is an address.
    config.channel.playlists =
        playlist_server("2024-01-30", "2024-01-31T06:00:00.500+01:00").into();
    manager.update_config(config).await;

    let mut playlist = source_generator(manager.clone()).await;

    next_entry(&mut playlist).await;

    assert_eq!(*manager.current_date.lock().await, "2024-01-31");
}
