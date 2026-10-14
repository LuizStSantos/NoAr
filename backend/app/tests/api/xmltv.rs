use super::*;

use ffplayout::utils::time_machine::time_now;

async fn get(fixture: &SecurityFixture, uri: &str) -> axum::response::Response {
    fixture
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("host", "tv.test:8787")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn write(path: std::path::PathBuf, content: &[u8]) {
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(path, content).await.unwrap();
}

#[tokio::test]
#[serial]
async fn xmltv_guide_and_images_are_public_and_confined_to_image_files_in_storage() {
    let fixture = security_fixture().await;
    let storage = fixture.root.join("storage");
    let timezone = {
        let manager = fixture.state.controller.read().await.get(1).unwrap();
        manager.config.read().await.channel.timezone
    };
    let today = time_now(&timezone).date_naive();

    write(
        storage.join("Show/tvshow.nfo"),
        "<tvshow><title>Meu Show</title><plot>Ao vivo</plot></tvshow>".as_bytes(),
    )
    .await;
    write(storage.join("Show/poster.jpg"), b"jpeg-bytes").await;
    write(storage.join("Show/ep.mp4"), b"video").await;
    let playlist = json!({
        "date": today.format("%Y-%m-%d").to_string(),
        "program": [{
            "source": storage.join("Show/ep.mp4").to_string_lossy(),
            "in": 0.0,
            "out": 600.0,
            "duration": 600.0,
        }]
    });
    write(
        fixture
            .root
            .join("playlists")
            .join(today.format("%Y/%m/%Y-%m-%d.json").to_string()),
        playlist.to_string().as_bytes(),
    )
    .await;

    let response = get(&fixture, "/xmltv/1/guide.xml?days=1&tvg_id=meu.canal").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/xml; charset=utf-8"
    );

    let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let xml = String::from_utf8(body.to_vec()).unwrap();

    assert!(xml.contains("<channel id=\"meu.canal\">"), "{xml}");
    assert!(xml.contains("<title>Meu Show</title>"), "{xml}");
    assert!(xml.contains("<desc>Ao vivo</desc>"), "{xml}");
    assert!(
        xml.contains("<icon src=\"http://tv.test:8787/xmltv/1/images/Show/poster.jpg\"/>"),
        "{xml}"
    );

    let response = get(&fixture, "/xmltv/1/images/Show/poster.jpg").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/jpeg");
    assert!(response.headers().get("content-disposition").is_none());
    assert_eq!(
        to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
        b"jpeg-bytes"
    );

    for (uri, status) in [
        ("/xmltv/1/images/Show/ep.mp4", StatusCode::NOT_FOUND),
        ("/xmltv/1/images/Show/tvshow.nfo", StatusCode::NOT_FOUND),
        ("/xmltv/1/images/Show/missing.jpg", StatusCode::NOT_FOUND),
        ("/xmltv/1/images/%2e%2e/secret.jpg", StatusCode::FORBIDDEN),
        ("/xmltv/99/guide.xml", StatusCode::NOT_FOUND),
        ("/xmltv/99/images/Show/poster.jpg", StatusCode::NOT_FOUND),
    ] {
        assert_eq!(get(&fixture, uri).await.status(), status, "{uri}");
    }

    #[cfg(unix)]
    {
        let outside = fixture.root.join("outside");
        write(outside.join("secret.jpg"), b"secret").await;
        std::os::unix::fs::symlink(&outside, storage.join("escape")).unwrap();

        assert_eq!(
            get(&fixture, "/xmltv/1/images/escape/secret.jpg")
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
}
