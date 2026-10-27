//! Hides, shows or swaps the logo on air while the channel plays, for
//! example from the encoder of a live whose programme has a commercial
//! break. The change lasts until the next one or a server restart. The
//! L-shape is switched the same way, with the same key.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use noar_engine::LogoConfig;
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::models::Role,
    file::norm_abs_path,
    player::controller::ChannelManager,
    utils::{errors::ServiceError, xmltv::library::has_image_extension},
};

/// Header an encoder can send the key in, instead of the `key` query.
const KEY_HEADER: &str = "x-logo-key";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogoAction {
    /// Fade the logo out.
    Hide,
    /// Fade the configured logo back in.
    Show,
    /// Show another image, from the storage, in place of the logo.
    Replace,
}

#[derive(Debug, Deserialize)]
pub struct LogoRequest {
    action: LogoAction,
    /// Storage path of the image, for `replace`.
    #[serde(default)]
    logo: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoKeyQuery {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    logo: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogoStatus {
    hidden: bool,
    /// Storage path of the image shown in place of the logo.
    replacement: Option<String>,
}

/// **What the logo on air is doing**
pub async fn get_logo_state(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let manager = authorized(&state, id, &user, &details).await?;

    Ok(Json(status(&manager)))
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
    let manager = authorized(&state, id, &user, &details).await?;

    apply(&manager, request.action, request.logo.as_deref()).await
}

/// **Hide, show or swap the logo on air, with the channel's logo key**
///
/// For encoders and scripts that cannot log in. The key is set in Settings ›
/// Logo; without one this answers 403.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/logo/1/hide?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/logo/1/replace?key=<KEY>&logo=00-assets/logo-ao-vivo.png'
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

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply(&manager, action, query.logo.as_deref()).await
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LShapeAction {
    /// Shrink the picture into the L-shape.
    On,
    /// Bring the picture back to the full screen.
    Off,
}

#[derive(Debug, Deserialize)]
pub struct LShapeRequest {
    action: LShapeAction,
}

#[derive(Debug, Serialize)]
pub struct LShapeStatus {
    /// Switched on from outside; a stretch marked in a video also shows it.
    on: bool,
    /// Whether the channel has an L-shape graphic.
    ready: bool,
}

/// **Whether the L-shape is on**
pub async fn get_lshape_state(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let manager = authorized(&state, id, &user, &details).await?;

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
    let manager = authorized(&state, id, &user, &details).await?;

    apply_lshape(&manager, request.action)
}

/// **Switch the L-shape on air, with the channel's logo key**
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/lshape/1/on?key=<KEY>'
/// curl -X POST 'http://127.0.0.1:8787/lshape/1/off?key=<KEY>'
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

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_lshape(&manager, action)
}

fn apply_lshape(
    manager: &ChannelManager,
    action: LShapeAction,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let control = &manager.lshape_control;

    match action {
        LShapeAction::On if !control.is_ready() => {
            return Err(ServiceError::Conflict(
                "No L-shape graphic: choose one in Settings › Logo".to_string(),
            ));
        }
        LShapeAction::On => control.on(),
        LShapeAction::Off => control.off(),
    }

    Ok(Json(lshape_status(manager)))
}

fn lshape_status(manager: &ChannelManager) -> LShapeStatus {
    LShapeStatus {
        on: manager.lshape_control.is_on(),
        ready: manager.lshape_control.is_ready(),
    }
}

async fn authorized(
    state: &AppState,
    id: i32,
    user: &AuthUser,
    details: &AuthDetails<Role>,
) -> Result<ChannelManager, ServiceError> {
    ensure_any_authority(
        details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let guard = state.controller.read().await;

    guard
        .get(id)
        .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))
}

/// An empty key turns the keyed route off. Compares every byte, so the time
/// taken tells nothing about how much of a guess was right.
pub(crate) fn key_matches(expected: &str, given: &str) -> bool {
    let expected = expected.trim().as_bytes();
    let given = given.trim().as_bytes();

    !expected.is_empty()
        && expected.len() == given.len()
        && expected
            .iter()
            .zip(given)
            .fold(0, |diff, (a, b)| diff | (a ^ b))
            == 0
}

async fn apply(
    manager: &ChannelManager,
    action: LogoAction,
    logo: Option<&str>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let control = &manager.logo_control;

    match action {
        LogoAction::Hide => control.hide(),
        LogoAction::Show => control.show(),
        LogoAction::Replace => {
            let config = manager.config.read().await.clone();
            let logo = logo
                .map(str::trim)
                .filter(|logo| !logo.is_empty())
                .ok_or_else(|| ServiceError::BadRequest("Name the image in `logo`".to_string()))?;
            let (path, _, relative) = norm_abs_path(&config.channel.storage, logo)?;

            if !path.is_file() || !has_image_extension(&path) {
                return Err(ServiceError::BadRequest(format!(
                    "{relative} is not an image in the storage"
                )));
            }

            let processing = &config.processing;
            let logo_config = LogoConfig {
                path: path.to_string_lossy().to_string(),
                scale: (!processing.logo_scale.trim().is_empty())
                    .then(|| processing.logo_scale.clone()),
                opacity: processing.logo_opacity,
                position: processing.logo_position.clone(),
            };

            control
                .replace(
                    &relative,
                    &logo_config,
                    config.output.width,
                    config.output.height,
                )
                .map_err(|error| ServiceError::BadRequest(format!("{relative}: {error:#}")))?;
        }
    }

    Ok(Json(status(manager)))
}

fn status(manager: &ChannelManager) -> LogoStatus {
    let state = manager.logo_control.state();

    LogoStatus {
        hidden: state.hidden,
        replacement: state.replacement,
    }
}

#[cfg(test)]
mod tests {
    use super::key_matches;

    #[test]
    fn the_logo_key_must_be_set_and_match() {
        assert!(key_matches("abc123", "abc123"));
        assert!(key_matches(" abc123 ", "abc123"));
        assert!(!key_matches("abc123", "abc124"));
        assert!(!key_matches("abc123", "abc12"));
        assert!(!key_matches("", ""));
    }
}
