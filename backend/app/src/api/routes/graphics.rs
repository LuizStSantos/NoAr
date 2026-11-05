use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{
        handles,
        models::{GraphicsTemplate, Role},
    },
    player::controller::ChannelManager,
    utils::{
        errors::ServiceError,
        graphics::{
            GraphicOnAirView, TakeOutcome, TemplateInput, graphics_on_air, prepare_template,
            take_template,
        },
    },
};

/// Everyone on the channel may see the templates; only admins change them.
async fn authorized(
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
    authorized(&state, id, &user, &details, false).await?;

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
    authorized(&state, id, &user, &details, false).await?;

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
    let manager = authorized(&state, id, &user, &details, true).await?;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut template = prepare_template(&storage, id, 0, input)?;

    ensure_unique_name(&state, &template).await?;
    template.id = handles::insert_graphics_template(&state.pool, &template).await?;

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
    let manager = authorized(&state, id, &user, &details, true).await?;
    let storage = manager.config.read().await.channel.storage.clone();
    let template = prepare_template(&storage, id, template_id, input)?;

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
    crate::utils::graphics::load_channel_ticker(&manager).await?;

    Ok(Json(template))
}

/// **Remove a graphics template**
pub async fn delete_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<&'static str>, ServiceError> {
    authorized(&state, id, &user, &details, true).await?;

    if handles::graphics_template_is_bound(&state.pool, id, template_id).await? {
        return Err(ServiceError::Conflict(
            "This template is bound to channel graphics and cannot be deleted".to_string(),
        ));
    }

    if !handles::delete_graphics_template(&state.pool, id, template_id).await? {
        return Err(template_not_found(template_id));
    }

    Ok(Json("removed"))
}

#[derive(Debug, Deserialize)]
pub struct TakeRequest {
    pub template_id: i32,
    /// Values for the template's `{field}` names.
    #[serde(default)]
    pub fields: HashMap<String, String>,
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
    let manager = authorized(&state, id, &user, &details, false).await?;
    let template = handles::select_graphics_template(&state.pool, id, request.template_id)
        .await?
        .ok_or_else(|| template_not_found(request.template_id))?;

    if handles::graphics_template_is_bound(&state.pool, id, request.template_id).await? {
        return Err(ServiceError::Conflict(
            "Use the channel logo, L-shape or quick-scene controls for this assigned model"
                .to_string(),
        ));
    }

    Ok(Json(
        take_template(&manager, template, request.fields, request.duration).await?,
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
    let manager = authorized(&state, id, &user, &details, false).await?;

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
    let manager = authorized(&state, id, &user, &details, false).await?;

    Ok(Json(graphics_on_air(&manager)))
}

/// **Font families the graphics can use**
///
/// The families installed for the engine, for the text layers of templates.
pub async fn get_graphics_fonts(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<String>>, ServiceError> {
    authorized(&state, id, &user, &details, false).await?;

    Ok(Json(
        tokio::task::spawn_blocking(noar_engine::available_font_families)
            .await
            .map_err(|_| ServiceError::InternalServerError)?,
    ))
}

/// Assigned channel models and their shared runtime control state.
pub async fn get_channel_graphics(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::utils::graphics::ChannelGraphicView>>, ServiceError> {
    let manager = authorized(&state, id, &user, &details, false).await?;

    Ok(Json(
        crate::utils::graphics::channel_graphics(&manager).await?,
    ))
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
) -> Result<Json<Vec<crate::utils::graphics::ChannelGraphicView>>, ServiceError> {
    let manager = authorized(&state, id, &user, &details, false).await?;
    crate::utils::graphics::control_channel_graphic(&manager, template_id, request.on).await?;

    Ok(Json(
        crate::utils::graphics::channel_graphics(&manager).await?,
    ))
}
