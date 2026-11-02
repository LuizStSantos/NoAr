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

#[tokio::test]
#[serial]
async fn assembled_programs_are_saved_listed_scheduled_and_updated_everywhere() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let storage = fixture.root.join("storage");
    let media = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets/storage/media_mix");
    for folder in ["Shows", "Ads", "Programs"] {
        tokio::fs::create_dir_all(storage.join(folder))
            .await
            .unwrap();
    }
    tokio::fs::copy(media.join("av_sync.mp4"), storage.join("Shows/b1.mp4"))
        .await
        .unwrap();
    tokio::fs::copy(media.join("no_audio.mp4"), storage.join("Shows/b2.mp4"))
        .await
        .unwrap();
    tokio::fs::copy(media.join("ad.mp4"), storage.join("Ads/spot.mp4"))
        .await
        .unwrap();
    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/file/1/commercial",
        Some(&token),
        json!({"path": "Ads", "commercial": true}),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let program = |segments: serde_json::Value| json!({"path": "Programs/Jornal ep 12.program", "title": "Jornal da Noite", "segments": segments});
    let block = |source: &str| json!({"kind": "block", "source": source});

    // Unauthenticated, a commercial outside a commercials folder, a block
    // that is a commercial, and no block at all are refused.
    let valid = program(json!([
        block("Shows/b1.mp4"),
        {"kind": "break", "clips": ["Ads/spot.mp4"]},
        block("Shows/b2.mp4")
    ]));
    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/programs",
        None,
        valid.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    for refused in [
        program(json!([block("Shows/b1.mp4"), {"kind": "break", "clips": ["Shows/b2.mp4"]}])),
        program(json!([block("Ads/spot.mp4")])),
        program(json!([{"kind": "break", "clips": ["Ads/spot.mp4"]}])),
    ] {
        let (status, body) = send(
            &fixture,
            "PUT",
            "/api/schedule/1/programs",
            Some(&token),
            refused,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }

    // A path out of the storage is refused like everywhere else.
    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/programs",
        Some(&token),
        program(json!([block("../secret.mp4")])),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, saved) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/programs",
        Some(&token),
        valid,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["segments"].as_array().unwrap().len(), 3);
    let length = |segment: &serde_json::Value| {
        segment["out"].as_f64().unwrap() - segment["in"].as_f64().unwrap()
    };
    let first = length(&saved["segments"][0]);
    let spot = length(&saved["segments"][1]["clips"][0]);
    let second = length(&saved["segments"][2]);
    assert!(first > 0.0 && spot > 0.0 && second > 0.0, "{saved}");
    let total = first + spot + second;

    let (status, read) = send(
        &fixture,
        "GET",
        "/api/schedule/1/programs?path=Programs/Jornal%20ep%2012.program",
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{read}");
    assert_eq!(read, saved);

    // The storage lists it as a program with its whole length.
    let (_, folder) = send(
        &fixture,
        "POST",
        "/api/file/1/browse",
        Some(&token),
        json!({"source": "/Programs"}),
    )
    .await;
    let listed = &folder["files"][0];
    assert_eq!(listed["name"], "Jornal ep 12.program");
    assert_eq!(listed["program"], true);
    assert!(
        (listed["duration"].as_f64().unwrap() - total).abs() < 0.01,
        "{folder}"
    );

    // A scheduled event takes its length, and the new one after an edit.
    let (status, event) = send(
        &fixture,
        "POST",
        "/api/schedule/1",
        Some(&token),
        json!({
            "kind": "media",
            "sources": ["Programs/Jornal ep 12.program"],
            "start_date": "2026-10-14",
            "start_time": "20:00",
            "entry_mode": "after_clip",
            "after_mode": "continue"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{event}");
    assert!(
        (event["duration"].as_f64().unwrap() - total).abs() < 0.01,
        "{event}"
    );

    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/programs",
        Some(&token),
        program(json!([block("Shows/b1.mp4")])),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, events) = send(
        &fixture,
        "GET",
        "/api/schedule/1/events",
        Some(&token),
        json!(null),
    )
    .await;
    let updated = events
        .as_array()
        .and_then(|events| events.iter().find(|item| item["id"] == event["id"]))
        .cloned()
        .unwrap_or_else(|| panic!("{events}"));
    assert!(
        (updated["duration"].as_f64().unwrap() - first).abs() < 0.01,
        "{updated}"
    );
}

#[tokio::test]
#[serial]
async fn lives_go_into_programs_and_list_with_their_usual_length() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let storage = fixture.root.join("storage");
    let media = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets/storage/media_mix");
    for folder in ["Shows", "Programs", "Lives/Jornal"] {
        tokio::fs::create_dir_all(storage.join(folder))
            .await
            .unwrap();
    }
    tokio::fs::copy(media.join("av_sync.mp4"), storage.join("Shows/b1.mp4"))
        .await
        .unwrap();
    tokio::fs::write(
        storage.join("Lives/Jornal/signal.live"),
        "srt://10.0.0.5:9000?mode=caller\n",
    )
    .await
    .unwrap();
    tokio::fs::write(
        storage.join("Lives/Jornal/tvshow.nfo"),
        "<tvshow><title>Jornal</title><runtime>30</runtime></tvshow>",
    )
    .await
    .unwrap();

    // Storage lists the live with its usual length.
    let (_, folder) = send(
        &fixture,
        "POST",
        "/api/file/1/browse",
        Some(&token),
        json!({"source": "/Lives/Jornal"}),
    )
    .await;
    let live = folder["files"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["name"] == "signal.live"))
        .cloned()
        .unwrap_or_else(|| panic!("{folder}"));
    assert_eq!(live["live"], true);
    assert_eq!(live["duration"], 1800.0);

    let program = |segments: serde_json::Value| json!({"path": "Programs/Com live.program", "title": "Com live", "segments": segments});
    for (refused, why) in [
        (
            json!([{"kind": "live", "source": "Lives/Jornal/signal.live", "duration": 0}]),
            "no length",
        ),
        (
            json!([{"kind": "live", "source": "Shows/b1.mp4", "duration": 60}]),
            "not a live",
        ),
    ] {
        let (status, body) = send(
            &fixture,
            "PUT",
            "/api/schedule/1/programs",
            Some(&token),
            program(refused),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {body}");
    }

    let (status, saved) = send(
        &fixture,
        "PUT",
        "/api/schedule/1/programs",
        Some(&token),
        program(json!([
            {"kind": "block", "source": "Shows/b1.mp4"},
            {"kind": "live", "source": "Lives/Jornal/signal.live", "duration": 1200}
        ])),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["segments"][1]["kind"], "live");
    assert_eq!(saved["segments"][1]["out"], 1200.0);
}

#[tokio::test]
#[serial]
async fn extending_a_live_needs_one_on_air_and_a_clear_request() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/schedule/1/extend",
        None,
        json!({"minutes": 30}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    for request in [json!({"minutes": 30}), json!({"until": "13:30"})] {
        let (status, body) = send(
            &fixture,
            "POST",
            "/api/schedule/1/extend",
            Some(&token),
            request,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "nothing on air: {body}");
        assert!(body.to_string().contains("no live"), "{body}");
    }
}
