use super::*;

async fn send(
    fixture: &SecurityFixture,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(method, uri, token, body))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into()));

    (status, value)
}

#[tokio::test]
#[serial]
async fn schedule_routes_manage_events_and_lives_for_allowed_users_only() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let storage = fixture.root.join("storage");
    let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets/storage/media_mix/ad.mp4");
    tokio::fs::create_dir_all(storage.join("Breaks"))
        .await
        .unwrap();
    tokio::fs::copy(&clip, storage.join("Breaks/ad.mp4"))
        .await
        .unwrap();

    let event = json!({
        "kind": "media",
        "sources": ["Breaks/ad.mp4"],
        "start_date": "2026-10-14",
        "start_time": "13:00",
        "repeat_days": 62,
        "entry_mode": "after_clip",
        "after_mode": "continue"
    });

    let (status, _) = send(&fixture, "POST", "/api/schedule/1", None, event.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, created) = send(
        &fixture,
        "POST",
        "/api/schedule/1",
        Some(&token),
        event.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["title"], "ad");
    assert_eq!(created["start_time"], "13:00:00");
    assert!(created["duration"].as_f64().unwrap() > 0.0);

    // Wednesday 2026-10-14 is one of the weekdays (Monday to Friday).
    let (status, day) = send(
        &fixture,
        "GET",
        "/api/schedule/1?date=2026-10-14",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(day["occurrences"].as_array().unwrap().len(), 1);
    assert_eq!(day["occurrences"][0]["start"], "2026-10-14T13:00:00");

    let (status, grade) = send(
        &fixture,
        "GET",
        "/api/schedule/1/grade?date=2026-10-14",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grade}");
    assert!(
        grade.as_array().unwrap().is_empty(),
        "no playlist in the fixture"
    );

    let (status, weekend) = send(
        &fixture,
        "GET",
        "/api/schedule/1?date=2026-10-17",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(weekend["occurrences"].as_array().unwrap().is_empty());

    for (body, reason) in [
        (
            json!({"kind": "media", "sources": ["../secret.mp4"], "start_date": "2026-10-14", "start_time": "13:00"}),
            "path outside storage",
        ),
        (
            json!({"kind": "media", "sources": ["Breaks/missing.mp4"], "start_date": "2026-10-14", "start_time": "13:00"}),
            "missing file",
        ),
        (
            json!({"kind": "media", "sources": ["Breaks/ad.mp4"], "start_date": "14/10/2026", "start_time": "13:00"}),
            "bad date",
        ),
        (
            json!({"kind": "live", "sources": ["Breaks/ad.mp4"], "start_date": "2026-10-14", "start_time": "13:00", "end_time": "14:00"}),
            "live without a .live file",
        ),
    ] {
        let (status, _) = send(&fixture, "POST", "/api/schedule/1", Some(&token), body).await;
        assert!(status.is_client_error(), "{reason}: {status}");
    }

    let (status, live) = send(
        &fixture,
        "POST",
        "/api/schedule/1/lives",
        Some(&token),
        json!({"name": "Big Match", "url": "srt://203.0.113.10:9000?mode=caller", "guide": {"title": "Big Match live"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{live}");
    assert_eq!(live["path"], "Lives/Big Match/signal.live");

    let (status, live_event) = send(
        &fixture,
        "POST",
        "/api/schedule/1",
        Some(&token),
        json!({"kind": "live", "sources": [live["path"]], "start_date": "2026-10-14",
               "start_time": "23:30", "end_time": "00:30", "entry_mode": "interrupt",
               "after_mode": "clock", "show_in_guide": true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{live_event}");
    assert_eq!(live_event["title"], "Big Match");

    let (status, lives) = send(
        &fixture,
        "GET",
        "/api/schedule/1/lives",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(lives[0]["title"], "Big Match live");

    let (status, changed) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/lives",
        Some(&token),
        json!({"path": live["path"], "url": "rtmp://203.0.113.20/live/key"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["protocol"], "RTMP");
    assert_eq!(changed["title"], "Big Match live");

    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/lives",
        Some(&token),
        json!({"path": "Breaks/ad.mp4", "url": "srt://203.0.113.10:9000"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/schedule/1/lives/test",
        Some(&token),
        json!({"url": "file:///etc/passwd"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The channel is not playing in this fixture.
    let trigger = format!("/api/schedule/1/{}/trigger", created["id"]);
    let (status, _) = send(&fixture, "POST", &trigger, Some(&token), json!(null)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/schedule/2",
        Some(&token),
        event.clone(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "user 1 has no access to channel 2"
    );

    let item = format!("/api/schedule/1/{}", created["id"]);
    let (status, _) = send(&fixture, "DELETE", &item, Some(&token), json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(&fixture, "DELETE", &item, Some(&token), json!(null)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, events) = send(
        &fixture,
        "GET",
        "/api/schedule/1/events",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(events.as_array().unwrap().len(), 1);
}

#[tokio::test]
#[serial]
async fn moving_an_event_lets_it_play_again_at_the_new_time() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let storage = fixture.root.join("storage");
    let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets/storage/media_mix/ad.mp4");
    tokio::fs::create_dir_all(storage.join("Breaks"))
        .await
        .unwrap();
    tokio::fs::copy(&clip, storage.join("Breaks/ad.mp4"))
        .await
        .unwrap();

    let mut event = json!({
        "kind": "media",
        "sources": ["Breaks/ad.mp4"],
        "start_date": "2026-10-14",
        "start_time": "20:30",
        "repeat_days": 127,
        "entry_mode": "interrupt",
        "after_mode": "continue"
    });
    let (status, created) = send(
        &fixture,
        "POST",
        "/api/schedule/1",
        Some(&token),
        event.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let id = created["id"].as_i64().unwrap() as i32;
    let item = format!("/api/schedule/1/{id}");
    let pool = &fixture.state.pool;
    let runs = |pool| async move {
        let mut days: Vec<String> =
            handles::select_schedule_runs(pool, 1, "2000-01-01", "2099-12-31")
                .await
                .unwrap()
                .into_iter()
                .map(|run| run.occurrence)
                .collect();
        days.sort_unstable();
        days
    };

    // One run long ago and one still to come (as if it already failed).
    for day in ["2000-01-01", "2099-01-01"] {
        assert!(
            handles::claim_schedule_run(pool, id, day, "failed")
                .await
                .unwrap()
        );
    }

    // A new title alone keeps the runs.
    event["title"] = json!("Plantão");
    let (status, _) = send(&fixture, "PUT", &item, Some(&token), event.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(runs(pool).await, ["2000-01-01", "2099-01-01"]);

    // A new time forgets the runs from today on, so it can play again.
    event["start_time"] = json!("21:00");
    let (status, updated) = send(&fixture, "PUT", &item, Some(&token), event.clone()).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["start_time"], "21:00:00");
    assert_eq!(runs(pool).await, ["2000-01-01"]);
}
