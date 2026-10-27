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
    /// Put the logo somewhere else, or back where the channel places it.
    Move,
}

#[derive(Debug, Deserialize)]
pub struct LogoRequest {
    action: LogoAction,
    /// Storage path of the image, for `replace`.
    #[serde(default)]
    logo: Option<String>,
    /// Where to, for `move`: a corner (`top-left`, `top-right`,
    /// `bottom-left`, `bottom-right`), an `X:Y` expression, or `default`.
    #[serde(default)]
    position: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoKeyQuery {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    position: Option<String>,
    /// Storage path of an L-shape graphic, for `replace` (and `on`).
    #[serde(default)]
    graphic: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LogoStatus {
    hidden: bool,
    /// Storage path of the image shown in place of the logo.
    replacement: Option<String>,
    /// Where the logo was moved to (`X:Y`); `null` is the channel's place.
    position: Option<String>,
}

/// The margin of a logo moved to a corner, in pixels.
const CORNER_MARGIN: u32 = 12;

/// A corner name as an `X:Y` expression; `default` (or nothing) is `None`.
fn logo_position(position: Option<&str>) -> Result<Option<String>, ServiceError> {
    let position = position.map(str::trim).unwrap_or_default();
    let m = CORNER_MARGIN;

    Ok(match position {
        "" => {
            return Err(ServiceError::BadRequest(
                "Say where in `position`: a corner, X:Y or default".to_string(),
            ));
        }
        "default" => None,
        "top-left" => Some(format!("{m}:{m}")),
        "top-right" => Some(format!("W-w-{m}:{m}")),
        "bottom-left" => Some(format!("{m}:H-h-{m}")),
        "bottom-right" => Some(format!("W-w-{m}:H-h-{m}")),
        expression => Some(expression.to_string()),
    })
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

    apply(
        &manager,
        request.action,
        request.logo.as_deref(),
        request.position.as_deref(),
    )
    .await
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

    apply(
        &manager,
        action,
        query.logo.as_deref(),
        query.position.as_deref(),
    )
    .await
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LShapeAction {
    /// Shrink the picture into the L-shape; with `graphic`, that one.
    On,
    /// Bring the picture back to the full screen.
    Off,
    /// Use another graphic from now on, on or off, until `reset`.
    Replace,
    /// Back to the channel's graphic.
    Reset,
}

#[derive(Debug, Deserialize)]
pub struct LShapeRequest {
    action: LShapeAction,
    /// Storage path of a graphic, for `replace` and optionally `on`.
    #[serde(default)]
    graphic: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LShapeStatus {
    /// Switched on from outside; a stretch marked in a video also shows it.
    on: bool,
    /// Whether there is a graphic to show.
    ready: bool,
    /// Storage path of the graphic swapped in for the channel's.
    replacement: Option<String>,
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

    apply_lshape(&manager, request.action, request.graphic.as_deref()).await
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

    apply_lshape(&manager, action, query.graphic.as_deref()).await
}

async fn apply_lshape(
    manager: &ChannelManager,
    action: LShapeAction,
    graphic: Option<&str>,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let control = &manager.lshape_control;
    let graphic = graphic.map(str::trim).filter(|graphic| !graphic.is_empty());

    if let Some(graphic) =
        graphic.filter(|_| matches!(action, LShapeAction::On | LShapeAction::Replace))
    {
        swap_lshape(manager, graphic).await?;
    } else if action == LShapeAction::Replace {
        return Err(ServiceError::BadRequest(
            "Name the graphic in `graphic`".to_string(),
        ));
    }

    match action {
        LShapeAction::On if !control.is_ready() => {
            return Err(ServiceError::Conflict(
                "No L-shape graphic: choose one in Settings › Logo".to_string(),
            ));
        }
        LShapeAction::On => control.on(),
        LShapeAction::Off => control.off(),
        LShapeAction::Replace => {}
        LShapeAction::Reset => control.reset(),
    }

    Ok(Json(lshape_status(manager)))
}

/// Load the storage image at `graphic` in place of the channel's L-shape graphic.
async fn swap_lshape(manager: &ChannelManager, graphic: &str) -> Result<(), ServiceError> {
    let config = manager.config.read().await.clone();
    let (path, _, relative) = norm_abs_path(&config.channel.storage, graphic)?;

    if !path.is_file() || !has_image_extension(&path) {
        return Err(ServiceError::BadRequest(format!(
            "{relative} is not an image in the storage"
        )));
    }

    let mut processing = config.processing.clone();
    processing.lshape = relative.clone();
    let lshape = processing
        .lshape_config(&config.channel.storage)
        .map_err(ServiceError::BadRequest)?
        .ok_or_else(|| ServiceError::BadRequest("Name the graphic in `graphic`".to_string()))?;
    let control = manager.lshape_control.clone();
    let (width, height) = (config.output.width, config.output.height);
    let label = relative.clone();

    tokio::task::spawn_blocking(move || control.replace(&label, &lshape, width, height))
        .await
        .map_err(|_| ServiceError::InternalServerError)?
        .map_err(|error| {
            ServiceError::BadRequest(format!(
                "{relative}: the L-shape graphic cannot be read ({})",
                error.root_cause()
            ))
        })
}

fn lshape_status(manager: &ChannelManager) -> LShapeStatus {
    LShapeStatus {
        on: manager.lshape_control.is_on(),
        ready: manager.lshape_control.is_ready(),
        replacement: manager.lshape_control.replacement(),
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
    position: Option<&str>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let control = &manager.logo_control;

    match action {
        LogoAction::Hide => control.hide(),
        LogoAction::Show => control.show(),
        LogoAction::Move => {
            let position = logo_position(position)?;
            let output = manager.config.read().await.output.clone();

            control
                .move_to(position.as_deref(), output.width, output.height)
                .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
        }
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
        position: state.position,
    }
}

#[cfg(test)]
mod tests {
    use super::{key_matches, logo_position};

    #[test]
    fn a_logo_goes_to_a_corner_an_expression_or_back() {
        assert_eq!(
            logo_position(Some("bottom-left")).unwrap().as_deref(),
            Some("12:H-h-12")
        );
        assert_eq!(
            logo_position(Some(" W/2-w/2:12 ")).unwrap().as_deref(),
            Some("W/2-w/2:12")
        );
        assert_eq!(logo_position(Some("default")).unwrap(), None);
        assert!(logo_position(None).is_err());
    }

    #[test]
    fn the_logo_key_must_be_set_and_match() {
        assert!(key_matches("abc123", "abc123"));
        assert!(key_matches(" abc123 ", "abc123"));
        assert!(!key_matches("abc123", "abc124"));
        assert!(!key_matches("abc123", "abc12"));
        assert!(!key_matches("", ""));
    }
}
