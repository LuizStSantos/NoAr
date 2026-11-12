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
//! the day that ended still loaded.

use std::{fs, path::PathBuf, sync::atomic::Ordering};

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

/// An entry that ran over the day start is followed by an init, as after a
/// clip that was missing: it seeks by the clock, so it must seek in the day
/// that began. In the playlist of the day that ended it would air that day
/// again from its top.
#[tokio::test]
#[serial]
async fn an_init_after_the_day_start_leaves_the_day_that_ended() {
    let (manager, _output) = channel_at("06:00:00", 21600.0, "2024-01-31T05:59:50+01:00").await;
    let mut playlist = source_generator(manager.clone()).await;

    next_entry(&mut playlist).await;
    assert_eq!(*manager.current_date.lock().await, "2024-01-30");

    set_mock_time(&Some("2024-01-31T06:00:10+01:00".to_string())).unwrap();
    manager.list_init.store(true, Ordering::SeqCst);
    next_entry(&mut playlist).await;

    assert_eq!(*manager.current_date.lock().await, "2024-01-31");
}

/// The day start passes while the playlist is read, or right after: more
/// than two seconds into the new day, no day change is close any more, and
/// the day that began must be the one loaded.
#[tokio::test]
#[serial]
async fn a_day_start_that_passes_during_an_init_is_not_missed() {
    let (manager, _output) = channel_at("06:00:00", 21600.0, "2024-01-31T05:59:58.500+01:00").await;
    let mut playlist = source_generator(manager.clone()).await;
    let started = std::time::Instant::now();

    next_entry(&mut playlist).await;

    // Until 06:00:02.5, whatever the read took.
    let into_the_new_day = std::time::Duration::from_secs(4);
    tokio::time::sleep(into_the_new_day.saturating_sub(started.elapsed())).await;
    next_entry(&mut playlist).await;

    assert_eq!(*manager.current_date.lock().await, "2024-01-31");
}
