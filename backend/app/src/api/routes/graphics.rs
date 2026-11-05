use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;

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
        graphics_template::{TemplateInput, prepare_template},
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

    if !handles::delete_graphics_template(&state.pool, id, template_id).await? {
        return Err(template_not_found(template_id));
    }

    Ok(Json("removed"))
}
