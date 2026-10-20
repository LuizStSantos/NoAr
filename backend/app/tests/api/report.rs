use chrono::{TimeDelta, Timelike};
use serde_json::Value;

use ffplayout::{
    db::{handles::CutBy, models::NewAsRunEntry},
    utils::time_machine::time_now,
};

use super::*;

async fn write(path: std::path::PathBuf, content: &[u8]) {
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(path, content).await.unwrap();
}

async fn report(fixture: &SecurityFixture, token: &str, query: &str) -> (StatusCode, Value) {
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "GET",
            &format!("/api/report/{query}"),
            Some(token),
            Value::Null,
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
#[serial]
async fn the_report_lists_what_aired_the_gaps_and_what_is_planned() {
    let fixture = security_fixture().await;
    let storage = fixture.root.join("storage");
    let token = access_token(&fixture.state, 1).await;
    let config = {
        let manager = fixture.state.controller.read().await.get(1).unwrap();

        manager.config.read().await.clone()
    };
    let now = time_now(&config.channel.timezone);
    let start_sec = config.playlist.start_sec.unwrap();
    // The playlist day now belongs to.
    let date = match f64::from(now.num_seconds_from_midnight()) < start_sec {
        true => now.date_naive() - TimeDelta::try_days(1).unwrap(),
        false => now.date_naive(),
    };
    let date = date.format("%Y-%m-%d").to_string();

    write(
        storage.join("Show/tvshow.nfo"),
        "<tvshow><title>Meu Show</title></tvshow>".as_bytes(),
    )
    .await;
    write(storage.join("Show/ep.mp4"), b"video").await;
    write(storage.join("Breaks/.comercial"), b"").await;
    write(storage.join("Breaks/spot.mp4"), b"video").await;

    // Ten-minute clips all day long, every other one a commercial.
    let program: Vec<Value> = (0..144)
        .map(|index| {
            let source = match index % 2 {
                0 => storage.join("Show/ep.mp4"),
                _ => storage.join("Breaks/spot.mp4"),
            };

            json!({"source": source.to_string_lossy(), "in": 0.0, "out": 600.0, "duration": 600.0})
        })
        .collect();
    write(
        fixture
            .root
            .join("playlists")
            .join(format!("{}/{}/{date}.json", &date[..4], &date[5..7])),
        json!({"date": date, "program": program})
            .to_string()
            .as_bytes(),
    )
    .await;

    // A clip that aired from ten to five minutes ago; nothing since.
    let aired_at = now.timestamp_millis() - 600_000;
    let id = handles::insert_as_run(
        &fixture.state.pool,
        1,
        &NewAsRunEntry {
            source: "Show/ep.mp4".to_string(),
            kind: "program",
            origin: "playlist",
            planned_seconds: 300.0,
            ..NewAsRunEntry::default()
        },
        aired_at,
        CutBy::default(),
    )
    .await
    .unwrap();
    handles::finish_as_run(
        &fixture.state.pool,
        id,
        aired_at + 300_000,
        "completed",
        None,
        None,
    )
    .await
    .unwrap();

    let (status, body) = report(&fixture, &token, &format!("1?from={date}")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["days"][0]["date"], date);
    assert_eq!(body["days"][0]["playlist"], true);
    assert_eq!(body["recorded_since"], aired_at);

    let items = body["items"].as_array().unwrap();
    let now = body["now"].as_i64().unwrap();
    let aired = items
        .iter()
        .find(|item| item["state"] == "aired" && item["kind"] == "program")
        .unwrap();

    assert_eq!(aired["title"], "Meu Show");
    assert_eq!(aired["program"], "Show");
    assert_eq!(aired["status"], "completed");
    assert_eq!(aired["day"], date);

    // The five minutes since then have no record.
    let gap = items.iter().find(|item| item["kind"] == "gap").unwrap();

    assert_eq!(gap["start"], aired_at + 300_000);
    assert_eq!(gap["end"], now);

    // What is still to come starts from now, commercials included.
    let planned: Vec<&Value> = items
        .iter()
        .filter(|item| item["state"] == "planned")
        .collect();

    assert!(!planned.is_empty());
    assert!(
        planned
            .iter()
            .all(|item| item["start"].as_i64().unwrap() >= now - 1_000)
    );
    assert!(planned.iter().any(|item| item["kind"] == "commercial"));
    assert!(planned.iter().any(|item| item["title"] == "Meu Show"));

    // Before the first record there is only the plan.
    assert!(
        items
            .iter()
            .filter(|item| item["state"] == "unrecorded")
            .all(|item| item["end"].as_i64().unwrap() <= aired_at)
    );
}

#[tokio::test]
#[serial]
async fn a_report_needs_access_to_the_channel_and_valid_days() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;

    for (query, expected) in [
        ("2?from=2026-10-20", StatusCode::FORBIDDEN),
        ("1?from=20-10-2026", StatusCode::BAD_REQUEST),
        ("1?from=2026-10-20&to=2026-10-19", StatusCode::BAD_REQUEST),
        ("1?from=2026-01-01&to=2026-12-31", StatusCode::BAD_REQUEST),
        ("1?from=2026-10-01&to=2026-10-31", StatusCode::OK),
    ] {
        let (status, _) = report(&fixture, &token, query).await;

        assert_eq!(status, expected, "{query}");
    }
}
