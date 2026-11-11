use super::*;

/// The entries recorded so far, newest first; they are written just after
/// each answer, so wait for `count` of them.
async fn recorded(state: &AppState, count: usize) -> Vec<handles::Activity> {
    for _ in 0..100 {
        let entries = handles::select_activity(&state.pool, &Default::default(), 100)
            .await
            .unwrap();
        if entries.len() >= count {
            return entries;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("fewer than {count} activity entries were recorded");
}

#[tokio::test]
#[serial]
async fn changes_sign_ins_and_refusals_are_recorded_for_global_admins() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let operator = access_token(&fixture.state, 1).await;
    let send = |method: &'static str, uri: &'static str, token: Option<String>, body| {
        let app = fixture.app.clone();
        async move {
            app.oneshot(json_request(method, uri, token.as_deref(), body))
                .await
                .unwrap()
                .status()
        }
    };

    // A change, a read, a refused change and a failed sign-in.
    let change =
        json!({"username": "regular", "mail": "new@example.org", "password": "never-shown"});
    assert_eq!(
        send("PUT", "/api/user/1", Some(admin.clone()), change).await,
        StatusCode::OK
    );
    assert_eq!(
        send("GET", "/api/users", Some(admin.clone()), json!({})).await,
        StatusCode::OK
    );
    let attempt = json!({"username": "administrator", "mail": "attacker@example.org"});
    assert_eq!(
        send("PUT", "/api/user/2", Some(operator.clone()), attempt).await,
        StatusCode::FORBIDDEN
    );
    let sign_in = json!({"username": "regular", "password": "wrong"});
    assert_eq!(
        send("POST", "/auth/login", None, sign_in).await,
        StatusCode::FORBIDDEN
    );

    let entries = recorded(&fixture.state, 3).await;
    let summary = entries
        .iter()
        .map(|entry| {
            (
                entry.action.as_str(),
                entry.username.as_str(),
                entry.target.as_str(),
                entry.status,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            ("auth.login", "regular", "regular", 403),
            ("user.update", "regular", "administrator", 403),
            ("user.update", "administrator", "regular", 200),
        ]
    );
    assert_eq!(entries[2].user_id, Some(2));
    assert_eq!(entries[2].role, "global_admin");
    assert_eq!(entries[2].channel_id, None);
    assert!(entries.iter().all(|entry| !entry.target.contains("never")));

    // Only a global admin reads it, with filters.
    assert_eq!(
        send("GET", "/api/activity", Some(operator), json!({})).await,
        StatusCode::FORBIDDEN
    );
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/activity?action=user&user=administrator&limit=5",
            Some(&admin),
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(page["more"], false);
    assert_eq!(page["entries"].as_array().unwrap().len(), 1);
    assert_eq!(page["entries"][0]["action"], "user.update");
    assert_eq!(page["entries"][0]["target"], "regular");
}
