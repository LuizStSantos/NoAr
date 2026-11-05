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

    let bound = handles::select_graphics_bindings(&fixture.state.pool, 1)
        .await
        .unwrap();
    let (status, message) = send(
        &fixture,
        "DELETE",
        &format!("/api/graphics/1/templates/{}", bound[0].template_id),
        Some(&admin),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(message.as_str().unwrap().contains("bound"));

    for binding in &bound {
        let (status, _) = send(
            &fixture,
            "POST",
            "/api/graphics/1/take",
            Some(&user),
            json!({"template_id": binding.template_id}),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "assigned graphics must not stack a second logo or L-shape"
        );
    }

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
    assert_eq!(list.as_array().unwrap().len(), 4);
    assert_eq!(
        list.as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == id)
            .unwrap()["fields"],
        json!(["name", "role"])
    );

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
    assert_eq!(list.as_array().unwrap().len(), 3);
}

#[tokio::test]
#[serial]
async fn graphics_go_on_and_off_air_and_follow_their_lshape_rule() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;
    let mut template = lower_third("Guest");
    template["layers"].as_array_mut().unwrap().pop();

    let (status, created) = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        template.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let template_id = created["id"].as_i64().unwrap();

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/graphics/1/take",
        None,
        json!({"template_id": template_id}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, taken) = send(
        &fixture,
        "POST",
        "/api/graphics/1/take",
        Some(&user),
        json!({"template_id": template_id, "fields": {"name": "Maria", "role": "Farmer"}, "duration": 0}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert_eq!(taken["state"], "on_air");
    let take_id = taken["take_id"].as_u64().unwrap();

    let (status, on_air) = send(
        &fixture,
        "GET",
        "/api/graphics/1/on-air",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(on_air.as_array().unwrap().len(), 1);
    assert_eq!(on_air[0]["take_id"], take_id);
    assert_eq!(on_air[0]["name"], "Guest");
    assert_eq!(on_air[0]["leaving"], false);

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/graphics/1/take",
        Some(&user),
        json!({"template_id": 9999}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        &fixture,
        "POST",
        "/api/graphics/1/out",
        Some(&user),
        json!({"take_id": 9999}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, out) = send(
        &fixture,
        "POST",
        "/api/graphics/1/out",
        Some(&user),
        json!({"take_id": take_id}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(out["on_air"][0]["leaving"], true, "it plays its exit first");

    // An L-shape on air: a template that skips it is refused, one that
    // waits goes on air when it ends.
    let manager = fixture.state.controller.read().await.get(1).unwrap();
    manager.lshape_control.on();

    for (rule, name) in [("skip", "Skips"), ("wait", "Waits")] {
        let mut ruled = template.clone();
        ruled["name"] = json!(name);
        ruled["during_lshape"] = json!(rule);
        let (status, created) = send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            ruled,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{created}");
        let (status, taken) = send(
            &fixture,
            "POST",
            "/api/graphics/1/take",
            Some(&user),
            json!({"template_id": created["id"]}),
        )
        .await;

        if rule == "skip" {
            assert_eq!(status, StatusCode::CONFLICT, "{taken}");
        } else {
            assert_eq!(status, StatusCode::OK, "{taken}");
            assert_eq!(taken["state"], "waiting");
        }
    }

    let on_air_names = |fixture: &SecurityFixture| {
        let manager = futures_free_manager(fixture);
        noar::utils::graphics::graphics_on_air(&manager)
            .into_iter()
            .filter(|graphic| !graphic.leaving)
            .map(|graphic| graphic.name)
            .collect::<Vec<_>>()
    };
    assert!(
        on_air_names(&fixture).is_empty(),
        "nothing new while the L-shape is on"
    );

    manager.lshape_control.off();
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert_eq!(on_air_names(&fixture), ["Waits"]);

    let (status, out) = send(
        &fixture,
        "POST",
        "/api/graphics/1/out",
        Some(&user),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        out["on_air"]
            .as_array()
            .unwrap()
            .iter()
            .all(|graphic| graphic["leaving"] == true)
    );
}

fn futures_free_manager(fixture: &SecurityFixture) -> noar::player::controller::ChannelManager {
    fixture.state.controller.try_read().unwrap().get(1).unwrap()
}
