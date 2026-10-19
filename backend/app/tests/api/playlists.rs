use super::*;

#[tokio::test]
#[serial]
async fn a_generated_draft_is_returned_without_being_saved() {
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

    let generate = |body: serde_json::Value| {
        let app = fixture.app.clone();
        let request = json_request(
            "POST",
            "/api/playlist/1/generate/2026-10-20",
            Some(&token),
            body,
        );

        async move {
            let response = app.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 64 << 20).await.unwrap();

            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
            )
        }
    };
    let saved = fixture.root.join("playlists/2026/10/2026-10-20.json");

    for _ in 0..2 {
        let (status, playlist) = generate(json!({"paths": ["Breaks"], "draft": true})).await;

        assert_eq!(
            status,
            StatusCode::OK,
            "a draft never conflicts: {playlist}"
        );
        assert!(!playlist["program"].as_array().unwrap().is_empty());
        assert!(!saved.exists(), "a draft is not written");
    }

    let (status, _) = generate(json!({"paths": ["Breaks"]})).await;

    assert_eq!(status, StatusCode::OK);
    assert!(saved.exists());

    let (status, _) = generate(json!({"paths": ["Breaks"]})).await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a saved day is not overwritten"
    );
}

#[tokio::test]
#[serial]
async fn the_next_episode_follows_the_last_one_that_aired() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let folder = fixture.root.join("storage/Show");
    let clip = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests_assets/storage/media_mix/ad.mp4");
    tokio::fs::create_dir_all(&folder).await.unwrap();

    // Day-first dates: name order would put 01.11.26 first.
    for name in [
        "show-01.11.26.mp4",
        "show-16.10.26.mp4",
        "show-31.10.26.mp4",
    ] {
        tokio::fs::copy(&clip, folder.join(name)).await.unwrap();
    }

    let aired = folder
        .join("show-16.10.26.mp4")
        .to_string_lossy()
        .to_string();
    let saved = fixture.root.join("playlists/2026/10/2026-10-19.json");
    tokio::fs::create_dir_all(saved.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(
        &saved,
        json!({
            "channel": "Channel 1",
            "date": "2026-10-19",
            "program": [{ "in": 0.0, "out": 1.0, "duration": 1.0, "source": aired }],
        })
        .to_string(),
    )
    .await
    .unwrap();

    let episodes = |body: serde_json::Value| {
        let app = fixture.app.clone();
        let request = json_request("POST", "/api/playlist/1/episodes", Some(&token), body);

        async move {
            let response = app.oneshot(request).await.unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();

            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
            )
        }
    };

    let (status, found) = episodes(json!({ "folder": "Show", "date": "2026-10-20" })).await;
    let names = found["episodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|episode| episode["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(
        names,
        [
            "show-16.10.26.mp4",
            "show-31.10.26.mp4",
            "show-01.11.26.mp4"
        ]
    );
    assert_eq!(found["last"]["source"], aired.as_str());
    assert_eq!(found["last"]["date"], "2026-10-19");
    assert_eq!(found["next"], 1);

    let last_one = folder
        .join("show-01.11.26.mp4")
        .to_string_lossy()
        .to_string();
    let (_, found) =
        episodes(json!({ "folder": "Show", "date": "2026-10-20", "after": last_one })).await;

    assert_eq!(found["next"], 0, "after the last episode it starts over");

    let (status, _) = episodes(json!({ "folder": "../outside", "date": "2026-10-20" })).await;

    assert_eq!(status, StatusCode::FORBIDDEN, "only folders in the storage");
}
