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
