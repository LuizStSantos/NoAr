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
    let (status, _) = send(
        &fixture,
        "DELETE",
        &format!("/api/graphics/1/templates/{}", bound[0].template_id),
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

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
    assert_eq!(list.as_array().unwrap().len(), bound.len() + 1);
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
    assert_eq!(list.as_array().unwrap().len(), bound.len());
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

#[tokio::test]
#[serial]
async fn assigned_models_have_channel_controls_and_keep_hidden_tickers_hidden_on_save() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;
    let uri = "/api/graphics/1/channel";
    let (status, _) = send(&fixture, "GET", uri, None, json!(null)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = send(
        &fixture,
        "GET",
        "/api/graphics/2/channel",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let ticker_created = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        json!({"name":"Ticker fixture","duration":0,"always_on":false,"layers":[]}),
    )
    .await;
    assert_eq!(ticker_created.0, StatusCode::OK);
    let ticker_id = ticker_created.1["id"].as_i64().unwrap();
    let assigned = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/templates/{ticker_id}/assignment"),
        Some(&admin),
        json!({"role":"ticker"}),
    )
    .await;
    assert_eq!(assigned.0, StatusCode::OK);
    let (status, cards) = send(&fixture, "GET", uri, Some(&user), json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cards.as_array().unwrap().len(), 2);
    let ticker = cards
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["kind"] == "ticker")
        .unwrap();
    assert_eq!(ticker["ready"], false);
    let id = ticker["template"]["id"].as_i64().unwrap();
    let control = format!("/api/graphics/1/channel/{id}");
    let (status, _) = send(&fixture, "POST", &control, Some(&user), json!({"on":true})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let model = json!({"name":"A renamed permanent model", "duration":0, "layers":[
        {"id":"ticker", "kind":"text", "x":5, "y":85, "width":90, "height":10, "text":"Channel bulletin"}
    ]});
    let (status, _) = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&admin),
        model.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, cards) = send(&fixture, "POST", &control, Some(&user), json!({"on":true})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cards
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["kind"] == "ticker")
            .unwrap()["on"],
        true
    );
    let (status, _) = send(&fixture, "POST", &control, Some(&user), json!({"on":false})).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&admin),
        model,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, cards) = send(&fixture, "GET", uri, Some(&user), json!(null)).await;
    let ticker = cards
        .as_array()
        .unwrap()
        .iter()
        .find(|card| card["kind"] == "ticker")
        .unwrap();
    assert_eq!(
        ticker["on"], false,
        "saving a hidden permanent model must not put it back on air"
    );
    assert_eq!(ticker["template"]["layers"][0]["text"], "Channel bulletin");
    let manager = fixture.state.controller.read().await.get(1).unwrap();
    assert!(
        manager.graphics_control.on_air().is_empty(),
        "permanent actions never enter the GC queue"
    );
}

#[tokio::test]
#[serial]
async fn an_admin_can_delete_an_assigned_model_without_restart_resurrection() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let bindings = handles::select_graphics_bindings(&fixture.state.pool, 1)
        .await
        .unwrap();
    assert_eq!(bindings.len(), 1);
    let id = bindings[0].template_id;
    let result = send(
        &fixture,
        "DELETE",
        &format!("/api/graphics/1/templates/{id}"),
        Some(&admin),
        json!(null),
    )
    .await;
    assert_eq!(result.0, StatusCode::OK);
    handles::convert_channel_graphics(&fixture.state.pool)
        .await
        .unwrap();
    assert!(
        handles::select_graphics_template(&fixture.state.pool, 1, id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        handles::select_graphics_bindings(&fixture.state.pool, 1)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/api/playout/config/1",
            Some(&admin),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
#[serial]
async fn permanent_capacity_and_bad_artwork_do_not_leave_partial_saved_models() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;

    let initial = handles::select_graphics_templates(&fixture.state.pool, 1)
        .await
        .unwrap()
        .iter()
        .filter(|template| template.always_on == Some(true))
        .count();

    for number in 0..(16 - initial) {
        let model = json!({"name":format!("Permanent {number}"),"always_on":true,"layers":[{"id":"box","kind":"box","x":5,"y":5,"width":10,"height":10,"fill":"#ffffff"}]});
        let result = send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            model,
        )
        .await;
        assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    }

    let before = handles::select_graphics_templates(&fixture.state.pool, 1)
        .await
        .unwrap();
    let rejected = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        json!({"name":"Seventeenth","always_on":true,"layers":[]}),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::CONFLICT);
    assert_eq!(
        handles::select_graphics_templates(&fixture.state.pool, 1)
            .await
            .unwrap()
            .len(),
        before.len()
    );
    let inserted = handles::insert_graphics_template(
        &fixture.state.pool,
        &noar::db::models::GraphicsTemplate {
            channel_id: 1,
            name: "Concurrent capacity".to_string(),
            always_on: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert!(
        inserted.is_err(),
        "the database must enforce the limit for concurrent writers too"
    );
    let removed = before
        .iter()
        .find(|model| model.name == "Permanent 0")
        .unwrap()
        .id;
    assert_eq!(
        send(
            &fixture,
            "DELETE",
            &format!("/api/graphics/1/templates/{removed}"),
            Some(&admin),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
    let before_invalid = handles::select_graphics_templates(&fixture.state.pool, 1)
        .await
        .unwrap()
        .len();
    let invalid=send(&fixture,"POST","/api/graphics/1/templates",Some(&admin),json!({"name":"Missing artwork","always_on":true,"layers":[{"id":"missing","kind":"image","image":"missing-font-artwork.png","x":0,"y":0,"width":10,"height":10}]})).await;
    assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        handles::select_graphics_templates(&fixture.state.pool, 1)
            .await
            .unwrap()
            .len(),
        before_invalid
    );
}

#[tokio::test]
#[serial]
async fn compatibility_assignment_preserves_an_explicit_image_target_and_authored_group() {
    let fixture = security_fixture().await;
    let admin = access_token(&fixture.state, 2).await;
    let created = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        json!({"name":"Programme identity","group":"My programme","layers":[
            {"id":"first","kind":"image","image":"logo.png","x":5,"y":5,"width":10,"height":10},
            {"id":"second","kind":"image","image":"logo.png","x":75,"y":5,"width":10,"height":10}
        ]}),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK);
    let id = created.1["id"].as_i64().unwrap();
    let route = format!("/api/graphics/1/templates/{id}/assignment");
    let assigned = send(
        &fixture,
        "PUT",
        &route,
        Some(&admin),
        json!({"role":"logo","control_layer_id":"second"}),
    )
    .await;
    assert_eq!(assigned.0, StatusCode::OK, "{}", assigned.1);
    assert_eq!(assigned.1["control_layer_id"], "second");
    assert_eq!(assigned.1["name"], "Programme identity");
    assert_eq!(assigned.1["group"], "My programme");
    let repeated = send(
        &fixture,
        "PUT",
        &route,
        Some(&admin),
        json!({"role":"logo"}),
    )
    .await;
    assert_eq!(repeated.1["control_layer_id"], "second");
    let invalid = send(
        &fixture,
        "PUT",
        &route,
        Some(&admin),
        json!({"role":"logo","control_layer_id":"foreign"}),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        send(&fixture, "PUT", &route, Some(&admin), json!({"role":null}))
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        !handles::select_graphics_bindings(&fixture.state.pool, 1)
            .await
            .unwrap()
            .iter()
            .any(|binding| binding.template_id == id as i32)
    );
}

#[tokio::test]
#[serial]
async fn clock_layers_and_shape_styles_validate_and_do_not_create_default_models() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;
    assert_eq!(
        send(&fixture, "GET", "/api/graphics/1/clock", None, json!(null))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/api/graphics/2/clock",
            Some(&user),
            json!(null)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let clock = send(
        &fixture,
        "GET",
        "/api/graphics/1/clock",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(clock.0, StatusCode::OK);
    assert!(
        (clock.1["unix_ms"].as_i64().unwrap() - chrono::Utc::now().timestamp_millis()).abs() < 5000
    );
    assert_eq!(
        handles::select_graphics_templates(&fixture.state.pool, 1)
            .await
            .unwrap()
            .len(),
        1
    );
    let body = json!({"name":"Studio clock","always_on":true,"layers":[
        {"id":"clock","kind":"clock","x":70,"y":5,"width":25,"height":10,"clock_format":"HH:mm:ss","clock_timezone":"UTC"},
        {"id":"ellipse","kind":"box","shape":"ellipse","x":5,"y":5,"width":10,"height":20,"fill":"#ffffff","stroke":"#123456","stroke_width":0.5}
    ]});
    let result = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        body.clone(),
    )
    .await;
    assert_eq!(result.0, StatusCode::OK, "{}", result.1);
    assert_eq!(result.1["layers"][0]["kind"], "clock");
    assert_eq!(result.1["layers"][1]["shape"], "ellipse");
    assert!(result.1["fields"].as_array().unwrap().is_empty());
    let mut invalid = body;
    invalid["name"] = json!("Bad clock");
    invalid["layers"][0]["clock_timezone"] = json!("Unknown/Zone");
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            invalid.clone()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    invalid["layers"][0]["clock_timezone"] = json!("UTC");
    invalid["layers"][0]["clock_format"] = json!("%arbitrary");
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            invalid
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
#[serial]
async fn uploaded_fonts_are_immutable_channel_assets_and_validate_template_ownership() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests_assets/fonts/graphics.ttf"),
    )
    .unwrap();

    async fn upload(
        fixture: &SecurityFixture,
        token: &str,
        bytes: Vec<u8>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method("POST")
            .uri("/api/graphics/1/font-assets?filename=Review.ttf")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/octet-stream")
            .body(Body::from(bytes))
            .unwrap();
        let response = fixture.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();

        (status, serde_json::from_slice(&body).unwrap())
    }

    assert_eq!(
        upload(&fixture, &user, bytes.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        upload(&fixture, &admin, b"not a font".to_vec()).await.0,
        StatusCode::BAD_REQUEST
    );
    let (status, first) = upload(&fixture, &admin, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, second) = upload(&fixture, &admin, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(first["id"], second["id"]);
    assert_ne!(first["family"], second["family"]);
    assert_eq!(first["filename"], second["filename"]);
    assert!(first.get("data").is_none());
    let id = first["id"].as_str().unwrap();
    assert_eq!(
        handles::select_graphics_font_data(&fixture.state.pool, 1, id)
            .await
            .unwrap(),
        Some(bytes.clone())
    );
    assert!(
        handles::select_graphics_font_data(&fixture.state.pool, 2, id)
            .await
            .unwrap()
            .is_none()
    );
    noar::utils::graphics::fonts::load(&fixture.state.pool, 1)
        .await
        .unwrap();
    assert_eq!(
        handles::select_graphics_font_data(&fixture.state.pool, 1, id)
            .await
            .unwrap(),
        Some(bytes)
    );
    let mut template = lower_third("Custom typeface");
    template["layers"][1]["font_family"] = json!("NoArFont_other_channel");
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            template.clone()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    template["layers"][1]["font_family"] = first["family"].clone();
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            template
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &fixture,
            "GET",
            &format!("/api/graphics/2/font-assets/{id}"),
            Some(&user),
            json!(null)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
#[serial]
async fn data_sources_enforce_channel_access_keep_last_good_rows_and_protect_references() {
    let fixture = security_fixture().await;
    let user = access_token(&fixture.state, 1).await;
    let admin = access_token(&fixture.state, 2).await;
    let input = json!({"name":"Quotes", "kind":"csv", "document":"symbol,price\nUSD,5.10\nEUR,6.20", "interval":5});
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/sources",
            Some(&user),
            input.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, source) = send(
        &fixture,
        "POST",
        "/api/graphics/1/sources",
        Some(&admin),
        input.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{source}");
    assert_eq!(source["rows"][1]["price"], "6.20");
    assert!(source.get("document").is_none());
    let id = source["id"].as_i64().unwrap();
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/api/graphics/1/sources",
            Some(&user),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        send(
            &fixture,
            "GET",
            "/api/graphics/2/sources",
            Some(&user),
            json!(null)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let private_url = "https://example.invalid/feed?token=synthetic-redaction-fixture";
    let external = noar::utils::graphics::sources::SourceInput {
        name: "Public provider".into(),
        kind: "json".into(),
        url: private_url.into(),
        interval: 30,
        ..Default::default()
    };
    let external_id = handles::save_graphics_source(&fixture.state.pool, 1, None, &external)
        .await
        .unwrap();
    let (_, operator_sources) = send(
        &fixture,
        "GET",
        "/api/graphics/1/sources",
        Some(&user),
        json!(null),
    )
    .await;
    assert_eq!(
        operator_sources
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["id"] == external_id)
            .unwrap()["url"],
        ""
    );
    let (_, admin_sources) = send(
        &fixture,
        "GET",
        "/api/graphics/1/sources",
        Some(&admin),
        json!(null),
    )
    .await;
    assert_eq!(
        admin_sources
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["id"] == external_id)
            .unwrap()["url"],
        private_url
    );
    let unavailable = json!({"name":"No initial data", "always_on":true, "layers":[{"id":"quote","x":5,"y":75,"width":50,"height":10,"text":"{price}","data_fields":{"price":{"source_id":external_id,"column":"price","row":0}}}]});
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            unavailable
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let mut template = json!({"name":"Quotes on air","always_on":true,"duration":0,"layers":[{"id":"quote","x":5,"y":75,"width":50,"height":10,"text":"{symbol} {price}","data_fields":{"symbol":{"source_id":id,"column":"symbol","row":0},"price":{"source_id":id,"column":"price","row":0}}}]});
    template["layers"][0]["data_fields"]["price"]["source_id"] = json!(999);
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/templates",
            Some(&admin),
            template.clone()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    template["layers"][0]["data_fields"]["price"]["source_id"] = json!(id);
    let (status, model) = send(
        &fixture,
        "POST",
        "/api/graphics/1/templates",
        Some(&admin),
        template,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{model}");
    let mut changed = input;
    changed["document"] = json!("symbol,other\nUSD,not-price");
    changed["revision"] = source["revision"].clone();
    let (status, failed) = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/sources/{id}"),
        Some(&admin),
        changed.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert!(failed["error"].is_string());
    assert_eq!(failed["rows"], source["rows"]);
    assert_eq!(failed["last_success"], source["last_success"]);
    assert_eq!(
        send(
            &fixture,
            "PUT",
            &format!("/api/graphics/1/sources/{id}"),
            Some(&admin),
            changed
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &fixture,
            "DELETE",
            &format!("/api/graphics/1/sources/{id}"),
            Some(&admin),
            json!(null)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &fixture,
            "POST",
            "/api/graphics/1/sources",
            Some(&admin),
            json!({"name":"Private","kind":"json","url":"http://127.0.0.1/","interval":5})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut literal = model.clone();
    literal["layers"][0]["text"] = json!("Literal quote");
    let (status, detached) = send(
        &fixture,
        "PUT",
        &format!("/api/graphics/1/templates/{}", model["id"]),
        Some(&admin),
        literal,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detached}");
    assert_eq!(detached["fields"], json!([]));
    assert!(detached["layers"][0].get("data_fields").is_none());
    assert_eq!(
        send(
            &fixture,
            "DELETE",
            &format!("/api/graphics/1/sources/{id}"),
            Some(&admin),
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
}
