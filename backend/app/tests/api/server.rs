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

fn json_request(method: &str, uri: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[tokio::test]
#[serial]
async fn only_a_global_admin_sees_the_update_check() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;

    for request in [
        request("GET", "/api/system/update", &user),
        request("POST", "/api/system/update/check", &user),
        json_request(
            "PUT",
            "/api/system/update/settings",
            &user,
            r#"{"auto_check":false}"#,
        ),
    ] {
        let uri = request.uri().to_string();
        let response = fixture.app.clone().oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{uri}");
    }
}

#[tokio::test]
#[serial]
async fn the_update_check_can_be_turned_off() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let read = |response: axum::response::Response| async move {
        serde_json::from_slice::<serde_json::Value>(
            &to_bytes(response.into_body(), 1 << 16).await.unwrap(),
        )
        .unwrap()
    };

    let response = fixture
        .app
        .clone()
        .oneshot(request("GET", "/api/system/update", &admin))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let status = read(response).await;

    assert_eq!(status["auto_check"], true);
    assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));

    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/system/update/settings",
            &admin,
            r#"{"auto_check":false}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(read(response).await["auto_check"], false);

    let response = fixture
        .app
        .clone()
        .oneshot(request("GET", "/api/system/update", &admin))
        .await
        .unwrap();

    assert_eq!(read(response).await["auto_check"], false);
}
