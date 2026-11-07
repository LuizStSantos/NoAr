//! Compatibility HTTP adapters for logo, picture-window and scene controls.
//!
//! Route names and wire DTOs remain stable. All commands are executed by the
//! same graphics application service used by the scheduler and model library.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use protect_axum::authorities::AuthDetails;
use serde::Deserialize;

use crate::{
    api::{auth::control_key_matches, routes::AuthUser, state::AppState},
    db::models::Role,
    graphics::control::{
        LShapeAction, LShapeStatus, LogoAction, LogoPlacement, LogoStatus, SceneStatus,
        apply_logo_action, apply_lshape_action, apply_named_scene, logo_status, lshape_status,
    },
    utils::errors::ServiceError,
};

use super::authorize_channel_access;

/// Header an encoder can send the key in, instead of the `key` query.
const KEY_HEADER: &str = "x-logo-key";

#[derive(Debug, Deserialize)]
pub struct LogoRequest {
    action: LogoAction,
    /// Storage path of the image, for `replace`.
    #[serde(default)]
    logo: Option<String>,
    /// Where to, for `move`: a place on the screen (`top-left`, `top`,
    /// `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom`,
    /// `bottom-right`), `X%:Y%`, an `X:Y` expression, or `default`.
    #[serde(default)]
    position: Option<String>,
    /// Instead of `position`: how far right and down, in percent of the
    /// room around the logo (0 is the left or top edge, 100 the other one).
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoKeyQuery {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    /// Storage path of an L-shape graphic, for `replace` (and `on`).
    #[serde(default)]
    graphic: Option<String>,
    /// Size of the picture in the L-shape, in percent of the screen.
    #[serde(default)]
    scale: Option<f64>,
}

/// **What the logo on air is doing**
pub async fn get_logo_state(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(logo_status(&manager)))
}

/// **Hide, show or swap the logo on air**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/logo -H 'Content-Type: application/json' \
/// -d '{"action": "replace", "logo": "00-assets/logo-ao-vivo.png"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn control_logo(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<LogoRequest>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    apply_logo_action(
        &manager,
        request.action,
        request.logo.as_deref(),
        LogoPlacement {
            position: request.position.as_deref(),
            x: request.x,
            y: request.y,
        },
    )
    .await
    .map(Json)
}

/// **Hide, show or swap the logo on air, with the channel's logo key**
///
/// For encoders and scripts that cannot log in. The key is set in Settings ›
/// Logo; without one this answers 403.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/logo/1/hide?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/logo/1/replace?key=<KEY>&logo=00-assets/logo-ao-vivo.png'
/// curl -X POST 'http://127.0.0.1:8787/logo/1/move?key=<KEY>&x=90&y=5'
/// ```
pub async fn control_logo_with_key(
    State(state): State<AppState>,
    Path((id, action)): Path<(i32, LogoAction)>,
    Query(query): Query<LogoKeyQuery>,
    headers: HeaderMap,
) -> Result<Json<LogoStatus>, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let expected = manager.config.read().await.processing.logo_key.clone();
    let given = headers
        .get(KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(query.key.as_deref())
        .unwrap_or_default();

    if !control_key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_logo_action(&manager, action, query.logo.as_deref(), query.place())
        .await
        .map(Json)
}

impl LogoKeyQuery {
    fn place(&self) -> LogoPlacement<'_> {
        LogoPlacement {
            position: self.position.as_deref(),
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LShapeRequest {
    action: LShapeAction,
    /// Storage path of a graphic, for `replace` and optionally `on`.
    #[serde(default)]
    graphic: Option<String>,
    /// The corner the picture goes to, for `move` and optionally `on`
    /// (`top-left`, `top-right`, `bottom-left`, `bottom-right`), or
    /// `default`.
    #[serde(default)]
    position: Option<String>,
    /// Size of the picture, in percent of the screen (30 to 95).
    #[serde(default)]
    scale: Option<f64>,
}

/// **Whether the L-shape is on**
pub async fn get_lshape_state(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    Ok(Json(lshape_status(&manager)))
}

/// **Switch the L-shape on air**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/lshape -H 'Content-Type: application/json' \
/// -d '{"action": "on"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn control_lshape(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<LShapeRequest>,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    apply_lshape_action(
        &manager,
        request.action,
        request.graphic.as_deref(),
        request.position.as_deref(),
        request.scale,
    )
    .await
    .map(Json)
}

/// **Switch the L-shape on air, with the channel's logo key**
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/lshape/1/on?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/lshape/1/off?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/lshape/1/move?key=<KEY>&position=bottom-left&scale=70'
/// ```
pub async fn control_lshape_with_key(
    State(state): State<AppState>,
    Path((id, action)): Path<(i32, LShapeAction)>,
    Query(query): Query<LogoKeyQuery>,
    headers: HeaderMap,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let expected = manager.config.read().await.processing.logo_key.clone();
    let given = headers
        .get(KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(query.key.as_deref())
        .unwrap_or_default();

    if !control_key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_lshape_action(
        &manager,
        action,
        query.graphic.as_deref(),
        query.position.as_deref(),
        query.scale,
    )
    .await
    .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct SceneRequest {
    /// The scene's name (case and punctuation do not matter).
    name: String,
}

/// **Apply a quick graphics scene**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/scene -H 'Content-Type: application/json' \
/// -d '{"name": "Intervalo comercial"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn control_scene(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<SceneRequest>,
) -> Result<Json<SceneStatus>, ServiceError> {
    let manager = authorize_channel_access(&state, id, &user, &details, false).await?;

    apply_named_scene(&manager, &request.name).await.map(Json)
}

/// **Apply a quick graphics scene, with the channel's graphics key**
///
/// The scene is named in the address, lower case with dashes.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/scene/1/intervalo-comercial?key=<KEY>'
/// ```
pub async fn control_scene_with_key(
    State(state): State<AppState>,
    Path((id, name)): Path<(i32, String)>,
    Query(query): Query<LogoKeyQuery>,
    headers: HeaderMap,
) -> Result<Json<SceneStatus>, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let expected = manager.config.read().await.processing.logo_key.clone();
    let given = headers
        .get(KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(query.key.as_deref())
        .unwrap_or_default();

    if !control_key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_named_scene(&manager, &name).await.map(Json)
}
