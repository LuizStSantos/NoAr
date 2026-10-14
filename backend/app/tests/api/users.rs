use super::*;

#[tokio::test]
#[serial]
async fn self_updates_cannot_grant_privileges_or_modify_another_user() {
    let fixture = security_fixture().await;
    let token = access_token(&fixture.state, 1).await;
    let response = fixture.app.clone().oneshot(json_request("PUT", "/api/user/1", Some(&token), json!({
        "username":"administrator", "mail":"updated@example.org", "role_id":1, "channel_ids":[1,2], "two_factor":false
    }))).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let user = handles::select_login(&fixture.state.pool, "regular")
        .await
        .unwrap();

    assert_eq!(user.mail.as_deref(), Some("updated@example.org"));
    assert_eq!(user.role_id, Some(3));
    assert_eq!(user.channel_ids, Some(vec![1]));
    assert!(user.two_factor);
    assert_eq!(user.password, "unchanged");
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/2",
            Some(&token),
            json!({"username":"administrator", "mail":"attacker@example.org"}),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        handles::select_user(&fixture.state.pool, 2)
            .await
            .unwrap()
            .mail
            .as_deref(),
        Some("admin@example.org")
    );
    let admin = access_token(&fixture.state, 2).await;
    let response = fixture.app.clone().oneshot(json_request("PUT", "/api/user/1", Some(&admin), json!({"username":"regular", "mail":"updated@example.org", "channel_ids":[1,2], "two_factor":false}))).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let user = handles::select_user(&fixture.state.pool, 1).await.unwrap();

    assert_eq!(user.channel_ids, Some(vec![1, 2]));
    assert!(!user.two_factor);
}

#[tokio::test]
#[serial]
async fn global_admins_change_roles_of_others_and_list_users() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let update = |id: i32, role: i32| {
        json_request(
            "PUT",
            &format!("/api/user/{id}"),
            Some(&admin),
            json!({"username": "ignored", "role_id": role, "channel_ids": [1]}),
        )
    };

    let response = fixture.app.clone().oneshot(update(1, 2)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        handles::select_user(&fixture.state.pool, 1)
            .await
            .unwrap()
            .role_id,
        Some(2)
    );

    // Their own role stays as it is, and unknown roles are refused.
    let response = fixture.app.clone().oneshot(update(2, 3)).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        handles::select_user(&fixture.state.pool, 2)
            .await
            .unwrap()
            .role_id,
        Some(1)
    );
    let response = fixture.app.clone().oneshot(update(1, 9)).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = fixture
        .app
        .clone()
        .oneshot(json_request("GET", "/api/users", Some(&admin), json!({})))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let users: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let regular = users
        .as_array()
        .unwrap()
        .iter()
        .find(|user| user["id"] == 1)
        .unwrap();

    assert_eq!(regular["username"], "regular");
    assert_eq!(regular["mail"], "user@example.org");
    assert_eq!(regular["role_id"], 2);
    assert_eq!(regular["channel_ids"], json!([1]));
}
