use std::time::Duration;

use tokio_stream::StreamExt;

use super::*;

fn preview_request(channel: i32, token: Option<&str>) -> Request<Body> {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!("/api/preview/{channel}"));

    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }

    request.body(Body::empty()).unwrap()
}

#[tokio::test]
#[serial]
async fn the_preview_needs_access_to_the_channel() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;

    let response = fixture
        .app
        .clone()
        .oneshot(preview_request(1, None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = fixture
        .app
        .clone()
        .oneshot(preview_request(2, Some(&token)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[serial]
async fn the_preview_streams_what_the_engine_sends() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let manager = fixture.state.controller.read().await.get(1).unwrap();
    let address = manager
        .preview
        .url()
        .trim_start_matches("udp://")
        .split('?')
        .next()
        .unwrap()
        .to_string();

    let response = fixture
        .app
        .clone()
        .oneshot(preview_request(1, Some(&token)))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "video/mp2t");

    let engine = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    engine.send_to(&[0x47; 1316], &address).await.unwrap();

    let mut body = response.into_body().into_data_stream();
    let chunk = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("the preview sends the datagram")
        .unwrap()
        .unwrap();

    assert_eq!(chunk.len(), 1316);
    assert_eq!(chunk[0], 0x47);
}
