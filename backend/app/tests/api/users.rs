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

#[tokio::test]
#[serial]
async fn an_operator_uses_only_the_areas_a_global_admin_gave_them() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let operator = access_token(&fixture.state, 1).await;
    let status = |method: &'static str, uri: &'static str, token: String| {
        let app = fixture.app.clone();
        async move {
            app.oneshot(json_request(method, uri, Some(&token), json!({})))
                .await
                .unwrap()
                .status()
        }
    };

    // Every area by default: the report opens.
    assert_ne!(
        status("GET", "/api/report/1", operator.clone()).await,
        StatusCode::FORBIDDEN
    );

    // A global admin gives the operator the Playlist only.
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/1",
            Some(&admin),
            json!({"username": "regular", "channel_ids": [1], "areas": ["playlist"]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        handles::select_user_areas(&fixture.state.pool, 1)
            .await
            .unwrap(),
        Some(vec![Area::Playlist])
    );

    // The change applies at once: the token issued before it is refused, so
    // the interface renews it and its menu follows the saved pages.
    assert_eq!(
        status("GET", "/api/playlist/1", operator.clone()).await,
        StatusCode::UNAUTHORIZED
    );
    let operator = access_token(&fixture.state, 1).await;

    for (method, uri) in [
        ("GET", "/api/report/1"),
        ("GET", "/api/log/1"),
        ("POST", "/api/schedule/1"),
        ("POST", "/api/graphics/1/take"),
        ("POST", "/api/file/1/remove"),
        ("PUT", "/api/file/1/sheet"),
    ] {
        assert_eq!(
            status(method, uri, operator.clone()).await,
            StatusCode::FORBIDDEN,
            "{method} {uri}"
        );
    }
    // Its own area, and the reads every page shares, still reach the routes.
    for (method, uri) in [
        ("POST", "/api/playlist/1"),
        ("GET", "/api/playlist/1"),
        ("GET", "/api/schedule/1"),
        ("POST", "/api/file/1/browse"),
    ] {
        assert_ne!(
            status(method, uri, operator.clone()).await,
            StatusCode::FORBIDDEN,
            "{method} {uri}"
        );
    }
    // Administrators are never limited.
    assert_ne!(
        status("GET", "/api/report/1", admin.clone()).await,
        StatusCode::FORBIDDEN
    );

    // The operator cannot give themselves more, and an update that leaves
    // `areas` out keeps the saved list.
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/1",
            Some(&operator),
            json!({"username": "regular", "areas": null}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/1",
            Some(&admin),
            json!({"username": "regular", "channel_ids": [1]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        handles::select_user_areas(&fixture.state.pool, 1)
            .await
            .unwrap(),
        Some(vec![Area::Playlist])
    );

    // Unknown areas are refused; null gives every area back.
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/1",
            Some(&admin),
            json!({"username": "regular", "areas": ["everything"]}),
        ))
        .await
        .unwrap();
    assert!(response.status().is_client_error());
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/user/1",
            Some(&admin),
            json!({"username": "regular", "areas": null}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        handles::select_user_areas(&fixture.state.pool, 1)
            .await
            .unwrap(),
        None
    );
    let operator = access_token(&fixture.state, 1).await;
    let report = status("GET", "/api/report/1", operator).await;
    assert!(
        ![StatusCode::FORBIDDEN, StatusCode::UNAUTHORIZED].contains(&report),
        "{report}"
    );
}

#[tokio::test]
#[serial]
async fn a_token_from_before_a_change_to_its_user_is_refused() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let operator = access_token(&fixture.state, 1).await;
    let status = |method: &'static str, uri: String, token: String, body: serde_json::Value| {
        let app = fixture.app.clone();
        async move {
            app.oneshot(json_request(method, &uri, Some(&token), body))
                .await
                .unwrap()
                .status()
        }
    };

    assert_eq!(
        status("GET", "/api/user".into(), operator.clone(), json!({})).await,
        StatusCode::OK
    );

    // A request without `channel_ids` keeps the saved channels.
    assert_eq!(
        status(
            "PUT",
            "/api/user/1".into(),
            admin.clone(),
            json!({"username": "regular", "mail": "kept@example.org"})
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        handles::select_user(&fixture.state.pool, 1)
            .await
            .unwrap()
            .channel_ids,
        Some(vec![1])
    );
    assert_eq!(
        status("GET", "/api/user".into(), operator.clone(), json!({})).await,
        StatusCode::OK
    );

    // Promoted, then demoted: the old token stops counting each time.
    let promoted = json!({"username": "regular", "role_id": 1, "channel_ids": [1]});
    assert_eq!(
        status("PUT", "/api/user/1".into(), admin.clone(), promoted).await,
        StatusCode::OK
    );
    assert_eq!(
        status("GET", "/api/user".into(), operator.clone(), json!({})).await,
        StatusCode::UNAUTHORIZED
    );
    let promoted_token = access_token(&fixture.state, 1).await;
    assert_eq!(
        status(
            "GET",
            "/api/users".into(),
            promoted_token.clone(),
            json!({})
        )
        .await,
        StatusCode::OK
    );
    let demoted = json!({"username": "regular", "role_id": 3, "channel_ids": [1]});
    assert_eq!(
        status("PUT", "/api/user/1".into(), admin.clone(), demoted).await,
        StatusCode::OK
    );
    assert_eq!(
        status("GET", "/api/users".into(), promoted_token, json!({})).await,
        StatusCode::UNAUTHORIZED
    );

    // A channel taken away, and a deleted user, too.
    let operator = access_token(&fixture.state, 1).await;
    assert_eq!(
        status(
            "PUT",
            "/api/user/1".into(),
            admin.clone(),
            json!({"username": "regular", "channel_ids": [2]})
        )
        .await,
        StatusCode::OK
    );
    assert_eq!(
        status("GET", "/api/playlist/1".into(), operator, json!({})).await,
        StatusCode::UNAUTHORIZED
    );
    let operator = access_token(&fixture.state, 1).await;
    assert_eq!(
        status("DELETE", "/api/user/1".into(), admin, json!({})).await,
        StatusCode::OK
    );
    assert_eq!(
        status("GET", "/api/user".into(), operator, json!({})).await,
        StatusCode::UNAUTHORIZED
    );
}
