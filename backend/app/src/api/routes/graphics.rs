//! Graphics HTTP entry points. Keep wire contracts and channel authorization here;
//! application services own control actions, template preparation and rendering.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

#[path = "graphics/control/graphics.rs"]
mod control;
#[path = "graphics/fonts/graphics.rs"]
mod fonts;
#[path = "graphics/sources/graphics.rs"]
mod sources;

pub use control::*;
pub use fonts::*;
pub use sources::*;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{
        handles,
        models::{GraphicsTemplate, Role},
    },
    graphics::{
        GraphicOnAirView, TakeOutcome, TemplateInput, graphics_on_air, prepare_template,
        take_template,
    },
    player::controller::ChannelManager,
    utils::errors::ServiceError,
};

/// Everyone on the channel may see the templates; only admins change them.
async fn authorize_channel_access(
    state: &AppState,
    id: i32,
    user: &AuthUser,
    details: &AuthDetails<Role>,
    edits: bool,
) -> Result<ChannelManager, ServiceError> {
    if edits {
        ensure_any_authority(details, &[&Role::GlobalAdmin, &Role::ChannelAdmin])?;
    } else {
        ensure_any_authority(
            details,
            &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
        )?;
    }

    user.ensure_channel_or_admin(id)?;

    let guard = state.controller.read().await;

    guard
        .get(id)
        .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))
}

fn template_not_found(template_id: i32) -> ServiceError {
    ServiceError::NotFound(format!("Graphics template {template_id} not found"))
}

async fn ensure_unique_name(
    state: &AppState,
    template: &GraphicsTemplate,
) -> Result<(), ServiceError> {
    if handles::graphics_template_name_taken(
        &state.pool,
        template.channel_id,
        &template.name,
        template.id,
    )
    .await?
    {
        return Err(ServiceError::Conflict(format!(
            "There is already a template called '{}'",
            template.name
        )));
    }

    Ok(())
}

/// **Graphics templates of a channel**
///
/// Each template comes with its layers and the `{field}` names to fill in.
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/api/graphics/1/templates' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_graphics_templates(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<GraphicsTemplate>>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(
        handles::select_graphics_templates(&state.pool, id).await?,
    ))
}

/// **One graphics template**
pub async fn get_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    authorize_channel_access(&state, id, &user, &details, false).await?;

    handles::select_graphics_template(&state.pool, id, template_id)
        .await?
        .map(Json)
        .ok_or_else(|| template_not_found(template_id))
}

/// **Add a graphics template**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/graphics/1/templates -H 'Content-Type: application/json' \
/// -d '{"name": "Lower third", "group": "Lower thirds", "duration": 8, "layers": [
///       {"id": "bar", "kind": "box", "x": 5, "y": 75, "width": 45, "height": 12, "fill": "#f39223"},
///       {"id": "name", "kind": "text", "x": 7, "y": 76, "width": 41, "height": 6, "text": "{name}"},
///       {"id": "role", "kind": "text", "x": 7, "y": 82, "width": 41, "height": 4, "text": "{role}"}]}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn add_graphics_template(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<TemplateInput>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut template = prepare_template(&storage, id, 0, input)?;
    crate::graphics::fonts::validate(&state.pool, &template).await?;
    crate::graphics::sources::validate_template(&state.pool, &template).await?;

    validate_permanent_template(&manager, &template).await?;

    ensure_unique_name(&state, &template).await?;
    template.id = handles::insert_graphics_template(&state.pool, &template).await?;
    crate::graphics::load_permanent_templates(&manager).await?;

    Ok(Json(template))
}

/// **Change a graphics template**
///
/// The whole template is sent again, as for adding one.
pub async fn update_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<TemplateInput>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let storage = manager.config.read().await.channel.storage.clone();
    let template = prepare_template(&storage, id, template_id, input)?;
    crate::graphics::fonts::validate(&state.pool, &template).await?;
    crate::graphics::sources::validate_template(&state.pool, &template).await?;

    if let Some(binding) = handles::select_graphics_bindings(&state.pool, id)
        .await?
        .into_iter()
        .find(|binding| binding.template_id == template_id)
    {
        use crate::{
            db::models::GraphicsRole,
            graphics::{LayerKind, graphic_spec_for_channel},
        };

        let video = template
            .layers
            .iter()
            .any(|layer| layer.visible && layer.kind == LayerKind::Video);

        if (binding.role == GraphicsRole::Lshape) != video {
            return Err(ServiceError::BadRequest(
                "Only the channel L-shape model must contain a visible video window".to_string(),
            ));
        }

        if crate::graphics::sources::has_unmapped_fields(&template) {
            return Err(ServiceError::BadRequest("Channel models need literal content or fully mapped data fields; unmapped fields belong to temporary templates".to_string()));
        }

        let config = manager.config.read().await.clone();
        let spec = graphic_spec_for_channel(&template, &HashMap::new(), &config)?;
        let output = manager.config.read().await.output.clone();
        tokio::task::spawn_blocking(move || {
            noar_engine::render_graphic(&spec, output.width, output.height)
        })
        .await
        .map_err(|_| ServiceError::InternalServerError)?
        .map_err(|error| {
            ServiceError::BadRequest(format!("The model cannot be rendered: {error:#}"))
        })?;
    }

    validate_permanent_template(&manager, &template).await?;
    ensure_unique_name(&state, &template).await?;

    if !handles::update_graphics_template(&state.pool, &template).await? {
        return Err(template_not_found(template_id));
    }

    let config = crate::utils::config::get_config(&state.pool, id).await?;
    let running = manager.running_config();
    let (width, height) = running
        .as_deref()
        .map_or((config.output.width, config.output.height), |running| {
            (running.output.width, running.output.height)
        });
    crate::player::output::load_logo(&manager, &config, width, height);
    crate::player::output::load_lshape(&manager, &config).await;
    manager.update_config(config).await;
    crate::graphics::load_channel_ticker(&manager).await?;

    Ok(Json(
        handles::select_graphics_template(&state.pool, id, template_id)
            .await?
            .ok_or_else(|| template_not_found(template_id))?,
    ))
}

/// **Remove a graphics template**
pub async fn delete_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<&'static str>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let removed_role = handles::select_graphics_bindings(&state.pool, id)
        .await?
        .into_iter()
        .find(|binding| binding.template_id == template_id)
        .map(|binding| binding.role);
    let active = crate::graphics::graphics_on_air(&manager);

    if !handles::delete_graphics_template(&state.pool, id, template_id).await? {
        return Err(template_not_found(template_id));
    }

    for take in active
        .into_iter()
        .filter(|take| take.template_id == template_id)
    {
        manager.graphics_control.take_out(take.take_id);
    }

    reload_graphics(&manager).await?;

    if let Some(role) = removed_role {
        crate::graphics::clear_removed_channel_graphic(&manager, role).await?;
    }

    Ok(Json("removed"))
}

#[derive(Debug, Deserialize)]
pub struct TakeRequest {
    pub template_id: i32,
    /// Values for the template's `{field}` names.
    #[serde(default)]
    pub fields: HashMap<String, String>,
    #[serde(default)]
    pub records: HashMap<i32, usize>,
    /// Seconds on air, in place of the template's; 0 stays until taken out.
    #[serde(default)]
    pub duration: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct OutRequest {
    /// The graphic to take out; none takes every graphic out.
    #[serde(default)]
    pub take_id: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OutStatus {
    pub on_air: Vec<GraphicOnAirView>,
}

/// **Put a template on air**
///
/// Answers `{"state": "on_air", "take_id"}`, or `{"state": "waiting"}` when
/// the template waits for the L-shape on air to end. 409 when the template
/// does not go on air during an L-shape and one is on.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/graphics/1/take -H 'Content-Type: application/json' \
/// -d '{"template_id": 3, "fields": {"name": "Maria da Silva", "role": "Farmer"}}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn take_graphic(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<TakeRequest>,
) -> Result<Json<TakeOutcome>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;
    let template = handles::select_graphics_template(&state.pool, id, request.template_id)
        .await?
        .ok_or_else(|| template_not_found(request.template_id))?;

    if template.always_on == Some(true)
        || handles::graphics_template_is_bound(&state.pool, id, request.template_id).await?
    {
        return Err(ServiceError::Conflict(
            "Use the channel logo, L-shape or quick-scene controls for this assigned model"
                .to_string(),
        ));
    }

    Ok(Json(
        take_template(
            &manager,
            template,
            request.fields,
            request.records,
            request.duration,
        )
        .await?,
    ))
}

/// **Take graphics out of the air**
///
/// With `take_id`, that graphic; without it, every graphic. Each plays its
/// exit animation. Answers what is still on air.
pub async fn take_graphic_out(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<OutRequest>,
) -> Result<Json<OutStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    match request.take_id {
        Some(take_id) => {
            if !manager.graphics_control.take_out(take_id) {
                return Err(ServiceError::NotFound(format!(
                    "Graphic {take_id} is not on air"
                )));
            }
        }
        None => manager.graphics_control.take_all_out(),
    }

    Ok(Json(OutStatus {
        on_air: graphics_on_air(&manager),
    }))
}

/// **Graphics on air**
pub async fn get_graphics_on_air(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<GraphicOnAirView>>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(graphics_on_air(&manager)))
}

/// Assigned channel models and their shared runtime control state.
pub async fn get_channel_graphics(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::graphics::ChannelGraphicView>>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(crate::graphics::channel_graphics(&manager).await?))
}

#[derive(Debug, Deserialize)]
pub struct ChannelGraphicRequest {
    pub on: bool,
}

pub async fn control_channel_graphics(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<ChannelGraphicRequest>,
) -> Result<Json<Vec<crate::graphics::ChannelGraphicView>>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    crate::graphics::control_channel_graphic(&manager, template_id, request.on).await?;

    Ok(Json(crate::graphics::channel_graphics(&manager).await?))
}

async fn validate_permanent_template(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
) -> Result<(), ServiceError> {
    if template.always_on != Some(true) {
        return Ok(());
    }

    if template
        .layers
        .iter()
        .any(|layer| layer.kind == crate::graphics::LayerKind::Video)
    {
        return Err(ServiceError::BadRequest(
            "A permanent template cannot contain a video window".to_string(),
        ));
    }

    if crate::graphics::sources::has_unmapped_fields(template) {
        return Err(ServiceError::BadRequest(
            "Map permanent template fields to a data source or use literal content".to_string(),
        ));
    }

    let existing = handles::select_graphics_templates(&manager.db_pool, manager.id).await?;

    if existing
        .iter()
        .filter(|other| other.id != template.id && other.always_on == Some(true))
        .count()
        >= noar_engine::MAX_GRAPHICS_ON_AIR
    {
        return Err(ServiceError::Conflict(
            "At most 16 permanent templates per channel".to_string(),
        ));
    }

    let config = manager.config.read().await.clone();
    let spec = crate::graphics::graphic_spec_for_channel(template, &HashMap::new(), &config)?;
    tokio::task::spawn_blocking(move || {
        noar_engine::render_graphic(&spec, config.output.width, config.output.height)
    })
    .await
    .map_err(|_| ServiceError::InternalServerError)?
    .map_err(|error| {
        ServiceError::BadRequest(format!("Permanent template cannot be rendered: {error:#}"))
    })?;

    Ok(())
}

async fn reload_graphics(manager: &ChannelManager) -> Result<(), ServiceError> {
    let config = crate::utils::config::get_config(&manager.db_pool, manager.id).await?;
    let running = manager.running_config();
    let (width, height) = running
        .as_deref()
        .map_or((config.output.width, config.output.height), |running| {
            (running.output.width, running.output.height)
        });
    crate::player::output::load_logo(manager, &config, width, height);
    crate::player::output::load_lshape(manager, &config).await;
    manager.update_config(config).await;
    crate::graphics::load_channel_ticker(manager).await
}

#[derive(Deserialize)]
pub struct GraphicsAssignmentRequest {
    pub role: Option<crate::db::models::GraphicsRole>,
    pub control_layer_id: Option<String>,
}

/// Assign an optional compatibility control independently of names and groups.
pub async fn assign_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<GraphicsAssignmentRequest>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let previous_role = handles::select_graphics_bindings(&state.pool, id)
        .await?
        .into_iter()
        .find(|binding| binding.template_id == template_id)
        .map(|binding| binding.role);
    let template = handles::select_graphics_template(&state.pool, id, template_id)
        .await?
        .ok_or_else(|| template_not_found(template_id))?;
    let video = template
        .layers
        .iter()
        .any(|layer| layer.visible && layer.kind == crate::graphics::LayerKind::Video);

    if let Some(role) = request.role
        && ((role == crate::db::models::GraphicsRole::Lshape) != video
            || crate::graphics::sources::has_unmapped_fields(&template))
    {
        return Err(ServiceError::BadRequest("Assign literal content or fully mapped data fields; only L-shape can contain a video window".to_string()));
    }

    if let Some(target) = &request.control_layer_id
        && !template
            .layers
            .iter()
            .any(|layer| layer.id == *target && layer.kind == crate::graphics::LayerKind::Image)
    {
        return Err(ServiceError::BadRequest(
            "The control target must be an image layer in this template".to_string(),
        ));
    }

    handles::assign_graphics_template(
        &state.pool,
        id,
        template_id,
        request.role,
        request.control_layer_id.as_deref(),
    )
    .await?;
    reload_graphics(&manager).await?;

    if let Some(role) = previous_role
        && Some(role) != request.role
    {
        crate::graphics::clear_removed_channel_graphic(&manager, role).await?;
    }

    Ok(Json(
        handles::select_graphics_template(&state.pool, id, template_id)
            .await?
            .ok_or_else(|| template_not_found(template_id))?,
    ))
}

/// Show live use and retained scheduled action snapshots before deletion.
pub async fn graphics_template_usage(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<serde_json::Value>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;
    let template = handles::select_graphics_template(&state.pool, id, template_id)
        .await?
        .ok_or_else(|| template_not_found(template_id))?;
    let views = crate::graphics::channel_graphics(&manager).await?;
    let assigned = views.iter().find(|view| view.template.id == template_id);
    let events = handles::select_graphics_events(&state.pool, id).await?;
    let events: Vec<_> = events
        .iter()
        .filter(|event| {
            event
                .template
                .as_ref()
                .is_some_and(|selection| selection.template_id == template_id)
                || assigned.is_some_and(|view| match view.kind.as_str() {
                    "logo" => {
                        event.actions.logo != crate::utils::config::SceneLogo::Keep
                            || event.actions.logo_position.is_some()
                    }
                    "lshape" => {
                        event.actions.lshape != crate::utils::config::SceneLShape::Keep
                            || event.actions.lshape_graphic.is_some()
                    }
                    "scene" => event.actions.name == view.scene_name.as_deref().unwrap_or_default(),
                    _ => false,
                })
        })
        .map(|event| serde_json::json!({"id":event.id,"title":event.title,"enabled":event.enabled}))
        .collect();
    let active = crate::graphics::graphics_on_air(&manager)
        .iter()
        .filter(|take| take.template_id == template_id)
        .count();

    Ok(Json(serde_json::json!({
        "assignment":assigned.map(|view|&view.kind),
        "on":assigned.is_some_and(|view|view.on),
        "always_on":template.always_on==Some(true),
        "temporary_instances":active,
        "events":events,
        "scheduled_actions_are_snapshots":true
    })))
}

pub async fn graphics_clock(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<serde_json::Value>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;
    let timezone = manager.config.read().await.channel.timezone;
    let now = chrono::Utc::now();
    let local = timezone.map_or_else(
        || chrono::Local::now().fixed_offset(),
        |zone| now.with_timezone(&zone).fixed_offset(),
    );
    let offset = local.offset().local_minus_utc();

    Ok(Json(
        serde_json::json!({"unix_ms":chrono::Utc::now().timestamp_millis(),"timezone":timezone.map(|zone|zone.to_string()),"offset_seconds":offset}),
    ))
}
