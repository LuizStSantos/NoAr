use super::*;

#[tokio::test]
#[serial]
async fn file_routes_enforce_tokens_channels_and_exact_range_responses() {
    let fixture = security_fixture().await;
    tokio::fs::write(fixture.root.join("storage/video.mp4"), b"0123456789")
        .await
        .unwrap();
    tokio::fs::write(fixture.root.join("storage/other.mp4"), b"other")
        .await
        .unwrap();
    let token = access_token(&fixture.state, 1).await;
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/file/1/video.mp4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/file/2/video.mp4")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/file/1/video.mp4")
                .header("authorization", format!("Bearer {token}"))
                .header("range", "bytes=2-4")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-range"], "bytes 2-4/10");
    assert_eq!(response.headers()["content-length"], "3");
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"234"
    );
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/file/1/video.mp4")
                .header("authorization", format!("Bearer {token}"))
                .header("range", "bytes=99-")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/file/2/access-token",
            Some(&token),
            json!({"filename":"video.mp4"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/file/1/access-token",
            Some(&token),
            json!({"filename":"video.mp4"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let value: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await.unwrap()).unwrap();
    let access = value["access"].as_str().unwrap();
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/file/1/video.mp4?access={access}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"0123456789"
    );

    for uri in [
        format!("/file/1/other.mp4?access={access}"),
        format!("/file/2/video.mp4?access={access}"),
    ] {
        let response = fixture
            .app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
#[serial]
async fn file_and_public_routes_reject_encoded_traversal_and_external_symlinks() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    tokio::fs::write(fixture.root.join("public/live/stream.m3u8"), b"playlist")
        .await
        .unwrap();
    let response = fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/public/1/live/stream.m3u8")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"playlist"
    );

    for uri in [
        "/file/1/%2e%2e/secret.mp4",
        "/file/1/sub/%2e%2e/%2e%2e/secret.mp4",
        "/public/1/live/%2e%2e/stream.m3u8",
        "/public/1/%2e%2e/secret.m3u8",
        "/public/1/%2Ftmp/secret.m3u8",
    ] {
        let response = fixture
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }

    #[cfg(unix)]
    {
        let outside = fixture.root.join("outside");
        tokio::fs::create_dir_all(&outside).await.unwrap();
        tokio::fs::write(outside.join("secret.mp4"), b"secret")
            .await
            .unwrap();
        tokio::fs::write(outside.join("secret.m3u8"), b"secret")
            .await
            .unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.join("storage/escape")).unwrap();
        std::os::unix::fs::symlink(&outside, fixture.root.join("public/escape")).unwrap();

        for uri in ["/file/1/escape/secret.mp4", "/public/1/escape/secret.m3u8"] {
            let response = fixture
                .app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
        }
    }
}

#[tokio::test]
#[serial]
async fn sheets_and_artwork_are_edited_through_the_storage_routes() {
    let fixture = security_fixture().await;
    let storage = fixture.root.join("storage");
    tokio::fs::create_dir_all(storage.join("Jornal"))
        .await
        .unwrap();
    tokio::fs::write(storage.join("Jornal/bloco1.mp4"), b"")
        .await
        .unwrap();
    let token = access_token(&fixture.state, 1).await;
    let call = |method: &'static str, uri: &'static str, body: serde_json::Value| {
        let app = fixture.app.clone();
        let token = token.clone();

        async move {
            let response = app
                .oneshot(json_request(method, uri, Some(&token), body))
                .await
                .unwrap();
            let status = response.status();
            let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();

            (
                status,
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or_default(),
            )
        }
    };

    let (status, sheet) = call("POST", "/api/file/1/sheet", json!({"path": "Jornal"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sheet["file"], serde_json::Value::Null);
    assert_eq!(sheet["kind"], "program");

    let (status, sheet) = call(
        "PUT",
        "/api/file/1/sheet",
        json!({
            "path": "Jornal",
            "kind": "program",
            "fields": {"title": "Jornal da Noite", "genres": ["Jornalismo"], "rating": "Livre"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sheet}");
    assert_eq!(sheet["file"], "Jornal/tvshow.nfo");
    assert_eq!(sheet["fields"]["title"], "Jornal da Noite");
    assert!(
        tokio::fs::read_to_string(storage.join("Jornal/tvshow.nfo"))
            .await
            .unwrap()
            .contains("<genre>Jornalismo</genre>")
    );

    let (status, _) = call(
        "PUT",
        "/api/file/1/sheet",
        json!({"path": "Jornal", "raw": "<tvshow><title>Broken</tvshow"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, browse) = call("POST", "/api/file/1/browse", json!({"source": "/"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(browse["sheet_folders"], json!(["Jornal"]));

    let boundary = "noar-artwork";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cover.png\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\0");
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let upload = |token: String| {
        Request::builder()
            .method("PUT")
            .uri("/api/file/1/artwork?path=Jornal&kind=poster")
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.clone()))
            .unwrap()
    };
    let response = fixture
        .app
        .clone()
        .oneshot(upload(token.clone()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let sheet: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 20).await.unwrap()).unwrap();
    assert_eq!(sheet["poster"], "Jornal/poster.png");

    let (status, sheet) = call(
        "POST",
        "/api/file/1/artwork/remove",
        json!({"path": "Jornal", "kind": "poster"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(sheet["poster"], serde_json::Value::Null);

    let (status, _) = call("POST", "/api/file/1/sheet", json!({"path": "../outside"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/file/2/sheet",
            Some(&token),
            json!({"path": "Jornal"}),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "no access to channel 2"
    );
}
