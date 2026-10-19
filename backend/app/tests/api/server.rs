use std::time::Duration;

use super::*;

fn request(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
#[serial]
async fn only_a_global_admin_sees_and_restarts_the_server() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;

    for (method, uri) in [
        ("GET", "/api/system/server"),
        ("POST", "/api/system/restart"),
    ] {
        let response = fixture
            .app
            .clone()
            .oneshot(request(method, uri, &user))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {uri}");
    }

    assert!(!fixture.state.shutdown.is_cancelled());
}

#[tokio::test]
#[serial]
async fn a_restart_answers_and_then_stops_the_server() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;

    let response = fixture
        .app
        .clone()
        .oneshot(request("GET", "/api/system/server", &admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let info: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1 << 16).await.unwrap()).unwrap();

    assert_eq!(info["channels"], 2);
    assert!(info["started_at"].as_i64().unwrap() > 0);

    let response = fixture
        .app
        .clone()
        .oneshot(request("POST", "/api/system/restart", &admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(3), fixture.state.shutdown.cancelled())
        .await
        .expect("the server is asked to stop after answering");
}
