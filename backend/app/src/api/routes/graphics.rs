//! Graphics HTTP entry points. Keep wire contracts and channel authorization here;
//! application services own control actions, template preparation and rendering.

use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

#[path = "graphics/fonts/graphics.rs"]
mod fonts;
#[path = "graphics/sources/graphics.rs"]
mod sources;

pub use fonts::*;
pub use sources::*;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{
        handles,
        models::{GraphicsRole, GraphicsTemplate, Role},
    },
    graphics::{
        GraphicOnAirView, TemplateInput, commands::CommandOutcome, graphics_on_air,
        prepare_template,
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
    Json(request): Json<GraphicsTemplateRequest>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut template = prepare_template(&storage, id, 0, request.template)?;
    let role = request
        .assignment
        .as_ref()
        .and_then(|assignment| assignment.role);

    if role == Some(GraphicsRole::Lshape) {
        template.always_on = Some(false);
    }

    validate_assignment_target(&template, request.assignment.as_ref())?;
    crate::graphics::fonts::validate(&state.pool, &template).await?;
    crate::graphics::sources::validate_template(&state.pool, &template).await?;

    validate_channel_template(&manager, &template, role).await?;
    let bindings = handles::select_graphics_bindings(&state.pool, id).await?;
    let replaced = role.and_then(|role| {
        bindings
            .iter()
            .find(|binding| binding.role == role)
            .map(|binding| binding.template_id)
    });
    validate_permanent_template(&manager, &template, replaced).await?;
    ensure_unique_name(&state, &template).await?;

    template.id = if let Some(assignment) = &request.assignment {
        handles::insert_graphics_template_with_assignment(
            &state.pool,
            &template,
            assignment.role,
            assignment
                .control_layer_id
                .as_ref()
                .map(|target| target.as_deref()),
        )
        .await?
    } else {
        handles::insert_graphics_template(&state.pool, &template).await?
    };

    reload_graphics(&manager).await?;

    Ok(Json(
        handles::select_graphics_template(&state.pool, id, template.id)
            .await?
            .ok_or_else(|| template_not_found(template.id))?,
    ))
}

#[derive(Deserialize)]
pub struct GraphicsTemplateRequest {
    #[serde(flatten)]
    pub template: TemplateInput,
    /// Omitted keeps the existing role on update; role null saves an unassigned model.
    #[serde(default)]
    pub assignment: Option<TemplateAssignmentUpdate>,
}

#[derive(Deserialize)]
pub struct TemplateAssignmentUpdate {
    pub role: Option<GraphicsRole>,
    #[serde(default, deserialize_with = "assignment_target")]
    pub control_layer_id: Option<Option<String>>,
}

fn assignment_target<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

fn validate_assignment_target(
    template: &GraphicsTemplate,
    assignment: Option<&TemplateAssignmentUpdate>,
) -> Result<(), ServiceError> {
    if let Some(target) = assignment
        .and_then(|assignment| assignment.control_layer_id.as_ref())
        .and_then(|target| target.as_ref())
        && !template
            .layers
            .iter()
            .any(|layer| layer.id == *target && layer.kind == crate::graphics::LayerKind::Image)
    {
        return Err(ServiceError::BadRequest(
            "The control target must be an image layer in this template".to_string(),
        ));
    }
    Ok(())
}

async fn validate_channel_template(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
    role: Option<GraphicsRole>,
) -> Result<(), ServiceError> {
    if let Some(role) = role {
        use crate::graphics::{LayerKind, graphic_spec_for_channel};

        let video = template
            .layers
            .iter()
            .any(|layer| layer.visible && layer.kind == LayerKind::Video);

        if (role == GraphicsRole::Lshape) != video {
            return Err(ServiceError::BadRequest(
                "Only the channel L-shape model must contain a visible video window".to_string(),
            ));
        }

        if crate::graphics::sources::has_unmapped_fields(template) {
            return Err(ServiceError::BadRequest("Channel models need literal content or fully mapped data fields; unmapped fields belong to temporary templates".to_string()));
        }

        let config = manager.config.read().await.clone();
        let spec = graphic_spec_for_channel(template, &HashMap::new(), &config)?;
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

    Ok(())
}

/// **Change a graphics template**
///
/// The whole template is sent again, as for adding one.
pub async fn update_graphics_template(
    State(state): State<AppState>,
    Path((id, template_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<GraphicsTemplateRequest>,
) -> Result<Json<GraphicsTemplate>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, true).await?;
    let _edit = manager.graphics_takes.begin_edit().await;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut template = prepare_template(&storage, id, template_id, request.template)?;
    let bindings = handles::select_graphics_bindings(&state.pool, id).await?;
    let previous_role = bindings
        .iter()
        .find(|binding| binding.template_id == template_id)
        .map(|binding| binding.role);
    let role = request
        .assignment
        .as_ref()
        .map_or(previous_role, |assignment| assignment.role);

    if request.assignment.is_some() && role == Some(GraphicsRole::Lshape) {
        template.always_on = Some(false);
    }

    validate_assignment_target(&template, request.assignment.as_ref())?;
    crate::graphics::fonts::validate(&state.pool, &template).await?;
    crate::graphics::sources::validate_template(&state.pool, &template).await?;

    validate_channel_template(&manager, &template, role).await?;

    let replaced = request
        .assignment
        .as_ref()
        .and_then(|assignment| assignment.role)
        .and_then(|role| {
            bindings
                .iter()
                .find(|binding| binding.role == role && binding.template_id != template_id)
        })
        .map(|binding| binding.template_id);
    validate_permanent_template(&manager, &template, replaced).await?;
    ensure_unique_name(&state, &template).await?;

    let updated = if let Some(assignment) = &request.assignment {
        handles::update_graphics_template_with_assignment(
            &state.pool,
            &template,
            assignment.role,
            assignment
                .control_layer_id
                .as_ref()
                .map(|target| target.as_deref()),
        )
        .await?
    } else {
        handles::update_graphics_template(&state.pool, &template).await?
    };

    if !updated {
        return Err(template_not_found(template_id));
    }

    reload_graphics(&manager).await?;

    if let Some(previous) = previous_role
        && Some(previous) != role
    {
        crate::graphics::clear_removed_channel_graphic(&manager, previous).await?;
    }

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
    /// The template by id; or use `template`.
    #[serde(default)]
    pub template_id: Option<i32>,
    /// The template by id or name (`comercial-l-shape`).
    #[serde(default)]
    pub template: Option<String>,
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
    /// One GC on air, as answered by a take.
    #[serde(default)]
    pub take_id: Option<u64>,
    /// A template by id: its GCs, or the logo, ticker or L-shape it is.
    #[serde(default)]
    pub template_id: Option<i32>,
    /// A template by id or name.
    #[serde(default)]
    pub template: Option<String>,
}

/// The template a request names, by `template_id` or `template`.
async fn requested_template(
    manager: &ChannelManager,
    template_id: Option<i32>,
    template: Option<&str>,
) -> Result<Option<GraphicsTemplate>, ServiceError> {
    match (template_id, template) {
        (Some(id), _) => crate::graphics::commands::find_template(manager, &id.to_string())
            .await
            .map(Some),
        (None, Some(reference)) => crate::graphics::commands::find_template(manager, reference)
            .await
            .map(Some),
        (None, None) => Ok(None),
    }
}

#[derive(Debug, Serialize)]
pub struct OutStatus {
    pub on_air: Vec<GraphicOnAirView>,
}

/// **Put a template on air**
///
/// Any template: a GC, an L-shape or an always-on logo or ticker, by
/// `template_id` or by `template` (id or name). `duration` is in seconds;
/// 0 keeps it on until it is taken off. Answers `{"state": "on_air",
/// "template_id", "take_id"}` (`take_id` only for a GC), or
/// `{"state": "waiting", "template_id"}` when a GC waits for the L-shape on
/// air to end.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/graphics/1/take -H 'Content-Type: application/json' \
/// -d '{"template": "comercial-l-shape", "duration": 50}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn take_graphic(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<TakeRequest>,
) -> Result<Json<CommandOutcome>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;
    let template = requested_template(&manager, request.template_id, request.template.as_deref())
        .await?
        .ok_or_else(|| ServiceError::BadRequest("Name the template".to_string()))?;

    Ok(Json(
        crate::graphics::commands::take(
            &manager,
            template,
            request.fields,
            request.records,
            request.duration,
        )
        .await?,
    ))
}

/// **Take graphics off air**
///
/// `template_id` or `template` takes that template off (its GCs, or the
/// logo, ticker or L-shape it is); `take_id` one GC; `{}` every GC, leaving
/// always-on templates as they are. Each plays its exit animation. Answers
/// what is still on air.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/graphics/1/out -H 'Content-Type: application/json' \
/// -d '{"template": "logo"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn take_graphic_out(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<OutRequest>,
) -> Result<Json<OutStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    if let Some(template) =
        requested_template(&manager, request.template_id, request.template.as_deref()).await?
    {
        crate::graphics::commands::off(&manager, &template).await?;

        return Ok(Json(OutStatus {
            on_air: graphics_on_air(&manager),
        }));
    }

    let _publication = manager.graphics_takes.lock_publication().await;

    match request.take_id {
        Some(take_id) => {
            if !manager.graphics_control.take_out(take_id) {
                return Err(ServiceError::NotFound(format!(
                    "Graphic {take_id} is not on air"
                )));
            }
        }
        None => {
            manager.graphics_takes.cancel_pending();
            manager.graphics_control.take_all_out();
        }
    }

    Ok(Json(OutStatus {
        on_air: graphics_on_air(&manager),
    }))
}

/// Header an encoder or a Stream Deck can send the graphics key in.
const GRAPHICS_KEY_HEADER: &str = "x-graphics-key";

async fn keyed_manager(
    state: &AppState,
    id: i32,
    query: &HashMap<String, String>,
    headers: &axum::http::HeaderMap,
) -> Result<ChannelManager, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let expected = manager.config.read().await.processing.logo_key.clone();
    let given = headers
        .get(GRAPHICS_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(query.get("key").map(String::as_str))
        .unwrap_or_default();

    if !crate::api::auth::control_key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing graphics key".to_string(),
        ));
    }

    Ok(manager)
}

/// **Put a template on air, with the channel's graphics key**
///
/// For encoders and controllers that only send a URL. `duration` (seconds)
/// is optional; every other query parameter fills the template field of the
/// same name.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/graphics/1/take/comercial-l-shape?key=<KEY>&duration=50'
/// curl -X POST 'http://127.0.0.1:8787/graphics/1/take/entrevistado?key=<KEY>&nome=Maria'
/// ```
pub async fn take_graphic_with_key(
    State(state): State<AppState>,
    Path((id, reference)): Path<(i32, String)>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<CommandOutcome>, ServiceError> {
    let manager = keyed_manager(&state, id, &query, &headers).await?;
    let template = crate::graphics::commands::find_template(&manager, &reference).await?;
    let duration = query
        .get("duration")
        .map(|value| {
            value.trim().replace(',', ".").parse::<f64>().map_err(|_| {
                ServiceError::BadRequest("duration is a number of seconds".to_string())
            })
        })
        .transpose()?;
    let fields = query
        .into_iter()
        .filter(|(name, _)| name != "key" && name != "duration")
        .collect();

    Ok(Json(
        crate::graphics::commands::take(&manager, template, fields, HashMap::new(), duration)
            .await?,
    ))
}

/// **Take a template off air, with the channel's graphics key**
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/graphics/1/out/logo?key=<KEY>'
/// ```
pub async fn take_graphic_out_with_key(
    State(state): State<AppState>,
    Path((id, reference)): Path<(i32, String)>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<CommandOutcome>, ServiceError> {
    let manager = keyed_manager(&state, id, &query, &headers).await?;
    let template = crate::graphics::commands::find_template(&manager, &reference).await?;

    Ok(Json(
        crate::graphics::commands::off(&manager, &template).await?,
    ))
}

/// **Everything on air, with the channel's graphics key**
///
/// The channel's logo, ticker, L-shape and always-on graphics that are on,
/// then the GCs; `template` is the name to use in the other commands.
///
/// ```BASH
/// curl 'http://127.0.0.1:8787/graphics/1/on-air?key=<KEY>'
/// ```
pub async fn graphics_on_air_with_key(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<crate::graphics::commands::OnAirGraphic>>, ServiceError> {
    let manager = keyed_manager(&state, id, &query, &headers).await?;

    Ok(Json(crate::graphics::commands::on_air(&manager).await?))
}

/// **Take everything off air, with the channel's graphics key**
///
/// GCs and the channel's logo, ticker, L-shape and always-on graphics.
/// `except` lists templates (name or id, separated by commas) that stay on.
/// Answers with what is still on air.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/graphics/1/out?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/graphics/1/out?key=<KEY>&except=logo,ticker'
/// ```
pub async fn all_graphics_out_with_key(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<crate::graphics::commands::OnAirGraphic>>, ServiceError> {
    let manager = keyed_manager(&state, id, &query, &headers).await?;
    let mut keep = Vec::new();

    for reference in query
        .get("except")
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        keep.push(crate::graphics::commands::find_template(&manager, reference).await?);
    }

    crate::graphics::commands::all_off(&manager, &keep).await?;

    Ok(Json(crate::graphics::commands::on_air(&manager).await?))
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
    let _publication = manager.graphics_takes.lock_publication().await;
    crate::graphics::control_channel_graphic(&manager, template_id, request.on).await?;

    Ok(Json(crate::graphics::channel_graphics(&manager).await?))
}

async fn validate_permanent_template(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
    replaced: Option<i32>,
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
        .filter(|other| {
            other.id != template.id && Some(other.id) != replaced && other.always_on == Some(true)
        })
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
