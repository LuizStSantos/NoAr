use super::*;

async fn send(
    fixture: &SecurityFixture,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = fixture
        .app
        .clone()
        .oneshot(json_request(method, uri, token, body))
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(&bytes).into()));

    (status, value)
}

fn lower_third(name: &str) -> serde_json::Value {
    json!({
        "name": name,
        "group": "Lower thirds",
        "duration": 8,
        "layers": [
            {"id": "bar", "kind": "box", "x": 5, "y": 75, "width": 45, "height": 12, "fill": "#f39223"},
            {"id": "name", "name": "Name", "kind": "text", "x": 7, "y": 76, "width": 41, "height": 6, "text": "{name}"},
            {"id": "role", "kind": "text", "x": 7, "y": 82, "width": 41, "height": 4, "text": "{role} · {name}"},
            {"id": "logo", "kind": "image", "x": 42, "y": 76, "width": 6, "height": 10, "image": "/Graphics/bug.png"}
        ]
    })
}

#[tokio::test]
#[serial]
async fn graphics_templates_are_read_by_the_channel_and_changed_by_admins() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;

    let (status, _) = send(
        &fixture,
        "GET",
        "/api/graphics/1/templates",
        None,
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&user),
        lower_third("Guest"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a regular user does not design templates"
    );

    let (status, created) = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        lower_third("  Guest  "),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["name"], "Guest");
    assert_eq!(created["during_lshape"], "follow");
    assert_eq!(created["fields"], json!(["name", "role"]));
    assert_eq!(created["layers"][3]["image"], "Graphics/bug.png");
    assert_eq!(created["layers"][1]["animation_in"], "fade");
    let id = created["id"].as_i64().unwrap();

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        lower_third("guest"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "names are unique per channel, ignoring case"
    );

    let (status, other_channel) = send(
        &fixture,
        "POST",
        "/api/graphics/2/templates",
        Some(&admin),
        lower_third("Guest"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "another channel may use the same name: {other_channel}"
    );

    let (status, list) = send(
        &fixture,
        "GET",
        "/api/graphics/1/templates",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["fields"], json!(["name", "role"]));

    let (status, _) = send(
        &fixture,
        "GET",
        "/api/graphics/2/templates",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the user is not on channel 2"
    );

    let mut changed = lower_third("Guest");
    changed["during_lshape"] = json!("wait");
    changed["layers"][2]["text"] = json!("{cargo}");
    let (status, updated) = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&admin),
        changed,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");

    let (status, read) = send(
        &fixture,
        "GET",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(read["during_lshape"], "wait");
    assert_eq!(read["fields"], json!(["name", "cargo"]));

    let (status, _) = send(
        &fixture,
        "PUT",
        "/api/graphics/1/templates/9999",
        Some(&admin),
        lower_third("Nothing"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let mut outside = lower_third("Outside");
    outside["layers"][3]["image"] = json!("../secret.png");
    let mut past_edge = lower_third("Past edge");
    past_edge["layers"][0]["width"] = json!(96);
    let mut bad_color = lower_third("Bad color");
    bad_color["layers"][1]["color"] = json!("orange");
    let mut same_id = lower_third("Same id");
    same_id["layers"][1]["id"] = json!("bar");

    for (body, reason) in [
        (outside, "image outside the storage"),
        (past_edge, "layer past the frame"),
        (bad_color, "color is not hex"),
        (same_id, "two layers with one id"),
        (json!({"name": "  ", "layers": []}), "blank name"),
    ] {
        let (status, _) = send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            body,
        )
        .await;
        assert!(status.is_client_error(), "{reason}: {status}");
    }

    let (status, _) = send(
        &fixture,
        "DELETE",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = send(
        &fixture,
        "DELETE",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&admin),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, list) = send(
        &fixture,
        "GET",
        "/api/graphics/1/templates",
        Some(&admin),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().is_empty());
}
