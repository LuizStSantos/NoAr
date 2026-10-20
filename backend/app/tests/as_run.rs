use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};

use ffplayout::db::{
    handles::{self, CutBy},
    models::{AsRunEntry, Channel, NewAsRunEntry},
};

async fn pool() -> Pool<Sqlite> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();

    handles::db_migrate(&pool).await.unwrap();

    pool
}

fn clip(source: &str, planned_seconds: f64) -> NewAsRunEntry {
    NewAsRunEntry {
        source: source.to_string(),
        kind: "program",
        origin: "playlist",
        planned_seconds,
        ..NewAsRunEntry::default()
    }
}

fn ingest(name: &str) -> NewAsRunEntry {
    NewAsRunEntry {
        source: name.to_string(),
        kind: "ingest",
        origin: "ingest",
        ..NewAsRunEntry::default()
    }
}

#[tokio::test]
async fn an_item_ends_with_its_status_and_reason() {
    let pool = pool().await;
    let id = handles::insert_as_run(
        &pool,
        1,
        &clip("Jornal/ep1.mp4", 60.0),
        1_000,
        CutBy::default(),
    )
    .await
    .unwrap();

    handles::finish_as_run(&pool, id, 31_000, "cut", Some("event"), Some("Plantão"))
        .await
        .unwrap();

    let rows = handles::select_as_run(&pool, 1, 0, 100_000).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source, "Jornal/ep1.mp4");
    assert_eq!(rows[0].ended_at, Some(31_000));
    assert_eq!(rows[0].status, "cut");
    assert_eq!(rows[0].reason.as_deref(), Some("event"));
    assert_eq!(rows[0].detail.as_deref(), Some("Plantão"));
}

#[tokio::test]
async fn a_new_item_closes_the_one_left_open() {
    let pool = pool().await;
    let first = handles::insert_as_run(&pool, 1, &clip("A.mp4", 60.0), 0, CutBy::default())
        .await
        .unwrap();
    let cut_by = CutBy {
        reason: Some("ingest"),
        detail: Some("Estúdio"),
    };

    handles::insert_as_run(&pool, 1, &ingest("Estúdio"), 5_000, cut_by)
        .await
        .unwrap();
    // The programme starts again before the ingest watcher closes its row.
    handles::insert_as_run(&pool, 1, &clip("B.mp4", 30.0), 9_000, CutBy::default())
        .await
        .unwrap();
    // The play loop closing the first clip late changes nothing.
    handles::finish_as_run(&pool, first, 12_000, "cut", Some("ingest"), None)
        .await
        .unwrap();

    let rows = handles::select_as_run(&pool, 1, 0, 100_000).await.unwrap();

    assert_eq!(rows.len(), 3);
    assert_eq!(
        (rows[0].ended_at, rows[0].status.as_str()),
        (Some(5_000), "cut")
    );
    assert_eq!(rows[0].detail.as_deref(), Some("Estúdio"));
    assert_eq!(
        (rows[1].ended_at, rows[1].status.as_str()),
        (Some(9_000), "completed")
    );
    assert_eq!(rows[1].reason, None);
    assert_eq!(
        (rows[2].ended_at, rows[2].status.as_str()),
        (None, "on_air")
    );
}

#[tokio::test]
async fn other_channels_stay_on_air() {
    let pool = pool().await;

    handles::create_channel_records(
        &pool,
        Channel {
            name: "Second".to_string(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    handles::insert_as_run(&pool, 2, &clip("Other.mp4", 60.0), 0, CutBy::default())
        .await
        .unwrap();
    handles::insert_as_run(&pool, 1, &clip("A.mp4", 60.0), 1_000, CutBy::default())
        .await
        .unwrap();

    let rows = handles::select_as_run(&pool, 2, 0, 100_000).await.unwrap();

    assert_eq!(rows[0].ended_at, None);
}

#[tokio::test]
async fn rows_left_open_end_where_the_item_would_have() {
    let pool = pool().await;

    handles::insert_as_run(&pool, 1, &clip("Short.mp4", 60.0), 0, CutBy::default())
        .await
        .unwrap();
    assert_eq!(
        handles::close_open_as_run(&pool, 1, 600_000).await.unwrap(),
        1
    );

    handles::insert_as_run(
        &pool,
        1,
        &clip("Film.mp4", 6_000.0),
        1_000_000,
        CutBy::default(),
    )
    .await
    .unwrap();
    handles::close_open_as_run(&pool, 1, 1_100_000)
        .await
        .unwrap();

    let rows = handles::select_as_run(&pool, 1, 0, 10_000_000)
        .await
        .unwrap();

    assert_eq!(rows[0].ended_at, Some(60_000));
    assert_eq!(rows[0].status, "stopped");
    assert_eq!(rows[0].reason.as_deref(), Some("interrupted"));
    assert_eq!(rows[1].ended_at, Some(1_100_000));
}

#[tokio::test]
async fn only_items_on_air_in_the_range_are_selected() {
    let pool = pool().await;

    for (index, start) in [0, 10_000, 20_000].into_iter().enumerate() {
        let id = handles::insert_as_run(
            &pool,
            1,
            &clip(&format!("{index}.mp4"), 10.0),
            start,
            CutBy::default(),
        )
        .await
        .unwrap();

        if index < 2 {
            handles::finish_as_run(&pool, id, start + 10_000, "completed", None, None)
                .await
                .unwrap();
        }
    }

    let sources =
        |rows: Vec<AsRunEntry>| rows.into_iter().map(|row| row.source).collect::<Vec<_>>();

    assert_eq!(
        sources(
            handles::select_as_run(&pool, 1, 5_000, 15_000)
                .await
                .unwrap()
        ),
        ["0.mp4", "1.mp4"]
    );
    assert_eq!(
        sources(
            handles::select_as_run(&pool, 1, 10_000, 20_000)
                .await
                .unwrap()
        ),
        ["1.mp4"]
    );
    // The item still on air counts until now.
    assert_eq!(
        sources(
            handles::select_as_run(&pool, 1, 25_000, 30_000)
                .await
                .unwrap()
        ),
        ["2.mp4"]
    );
}
