//! Hides, shows or swaps the logo on air while the channel plays, for
//! example from the encoder of a live whose programme has a commercial
//! break. The change lasts until the next one or a server restart. The
//! L-shape is switched the same way, with the same key.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use noar_engine::{LShapeCorner, LShapePlacement, LogoConfig};
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
    utils::{
        config::{GraphicsScene, SceneLShape, SceneLogo, slug},
        errors::ServiceError,
        xmltv::library::has_image_extension,
    },
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

/// Where to put something: `position`, or the shares `x` and `y`.
#[derive(Debug, Clone, Copy, Default)]
struct Place<'a> {
    position: Option<&'a str>,
    x: Option<f64>,
    y: Option<f64>,
}

impl<'a> Place<'a> {
    /// `position`, or `x`/`y` as `X%:Y%`.
    fn resolve(self) -> Result<Option<String>, ServiceError> {
        match (self.x, self.y) {
            (None, None) => Ok(self
                .position
                .map(str::trim)
                .filter(|position| !position.is_empty())
                .map(str::to_string)),
            (Some(x), Some(y)) => {
                let share = |value: f64| {
                    (value.is_finite() && (0.0..=100.0).contains(&value))
                        .then(|| format!("{}%", (value * 10.0).round() / 10.0))
                        .ok_or_else(|| {
                            ServiceError::BadRequest(
                                "`x` and `y` go from 0 to 100 (percent)".to_string(),
                            )
                        })
                };

                Ok(Some(format!("{}:{}", share(x)?, share(y)?)))
            }
            _ => Err(ServiceError::BadRequest(
                "Give both `x` and `y`, or `position`".to_string(),
            )),
        }
    }
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

/// A place name as an `X:Y` expression; `default` is `None`.
fn logo_position(position: Option<&str>) -> Result<Option<String>, ServiceError> {
    let position = position.map(str::trim).unwrap_or_default();
    let m = CORNER_MARGIN;
    let (left, center, right) = (format!("{m}"), "W/2-w/2", format!("W-w-{m}"));
    let (top, middle, bottom) = (format!("{m}"), "H/2-h/2", format!("H-h-{m}"));

    Ok(match position {
        "" => {
            return Err(ServiceError::BadRequest(
                "Say where in `position` (a place, X%:Y%, X:Y or default), or give `x` and `y`"
                    .to_string(),
            ));
        }
        "default" => None,
        "top-left" => Some(format!("{left}:{top}")),
        "top" => Some(format!("{center}:{top}")),
        "top-right" => Some(format!("{right}:{top}")),
        "left" => Some(format!("{left}:{middle}")),
        "center" => Some(format!("{center}:{middle}")),
        "right" => Some(format!("{right}:{middle}")),
        "bottom-left" => Some(format!("{left}:{bottom}")),
        "bottom" => Some(format!("{center}:{bottom}")),
        "bottom-right" => Some(format!("{right}:{bottom}")),
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
        Place {
            position: request.position.as_deref(),
            x: request.x,
            y: request.y,
        },
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

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply(&manager, action, query.logo.as_deref(), query.place()).await
}

impl LogoKeyQuery {
    fn place(&self) -> Place<'_> {
        Place {
            position: self.position.as_deref(),
            x: self.x,
            y: self.y,
        }
    }
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
    /// Put the picture somewhere else in the L-shape, or back where the
    /// graphic places it.
    Move,
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

#[derive(Debug, Serialize)]
pub struct LShapeStatus {
    /// Switched on from outside; a stretch marked in a video also shows it.
    on: bool,
    /// Whether there is a graphic to show.
    ready: bool,
    /// Storage path of the graphic swapped in for the channel's.
    replacement: Option<String>,
    /// The corner the picture was moved to; `null` is the graphic's.
    position: Option<String>,
    /// The size it was given with the move, in percent of the screen.
    scale: Option<f64>,
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

    apply_lshape(
        &manager,
        request.action,
        request.graphic.as_deref(),
        request.position.as_deref(),
        request.scale,
    )
    .await
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

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_lshape(
        &manager,
        action,
        query.graphic.as_deref(),
        query.position.as_deref(),
        query.scale,
    )
    .await
}

/// Where the picture goes for `place` and `scale`: `Ok(None)` puts it back
/// where the graphic places it.
fn lshape_placement(
    position: Option<&str>,
    scale: Option<f64>,
) -> Result<Option<LShapePlacement>, ServiceError> {
    let position = position.map(str::trim).unwrap_or_default();

    match position {
        "default" => Ok(None),
        "" => Err(ServiceError::BadRequest(
            "Say which corner in `position` (top-left, top-right, bottom-left, bottom-right) or default"
                .to_string(),
        )),
        corner => {
            let corner = LShapeCorner::parse(corner).ok_or_else(|| {
                ServiceError::BadRequest(format!(
                    "Unknown L-shape corner '{corner}': use top-left, top-right, bottom-left, bottom-right or default"
                ))
            })?;

            Ok(Some(LShapePlacement { corner, scale }))
        }
    }
}

async fn apply_lshape(
    manager: &ChannelManager,
    action: LShapeAction,
    graphic: Option<&str>,
    position: Option<&str>,
    scale: Option<f64>,
) -> Result<Json<LShapeStatus>, ServiceError> {
    let control = &manager.lshape_control;
    let graphic = graphic.map(str::trim).filter(|graphic| !graphic.is_empty());

    // Placed first, so `on` shrinks the picture straight to its new corner.
    if action == LShapeAction::Move || (action == LShapeAction::On && position.is_some()) {
        control
            .place(lshape_placement(position, scale)?)
            .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
    }

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
        LShapeAction::Replace | LShapeAction::Move => {}
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
    let placement = manager.lshape_control.placement();

    LShapeStatus {
        on: manager.lshape_control.is_on(),
        ready: manager.lshape_control.is_ready(),
        replacement: manager.lshape_control.replacement(),
        position: placement.map(|placement| placement.corner.to_string()),
        scale: placement.and_then(|placement| placement.scale),
    }
}

#[derive(Debug, Deserialize)]
pub struct SceneRequest {
    /// The scene's name (case and punctuation do not matter).
    name: String,
}

#[derive(Debug, Serialize)]
pub struct SceneStatus {
    /// The scene's name as written.
    scene: String,
    logo: LogoStatus,
    lshape: LShapeStatus,
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
    let manager = authorized(&state, id, &user, &details).await?;

    apply_scene(&manager, &request.name).await
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

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing logo key".to_string(),
        ));
    }

    apply_scene(&manager, &name).await
}

/// Apply the scene called `name`: the L-shape graphic and place first, then
/// the logo, then the L-shape in or out.
async fn apply_scene(
    manager: &ChannelManager,
    name: &str,
) -> Result<Json<SceneStatus>, ServiceError> {
    let wanted = slug(name);
    let scene = manager
        .config
        .read()
        .await
        .processing
        .graphics_scenes
        .iter()
        .find(|scene| scene.slug() == wanted)
        .cloned()
        .ok_or_else(|| ServiceError::NotFound(format!("No scene called '{}'", name.trim())))?;

    apply_graphics(manager, &scene).await?;

    Ok(Json(SceneStatus {
        scene: scene.name,
        logo: status(manager),
        lshape: lshape_status(manager),
    }))
}

/// Put the changes of `scene` on air: the L-shape graphic and place first,
/// then the logo, then the L-shape in or out. A scene or a scheduled
/// graphics event.
pub(crate) async fn apply_graphics(
    manager: &ChannelManager,
    scene: &GraphicsScene,
) -> Result<(), ServiceError> {
    let control = &manager.lshape_control;

    if let Some(graphic) = scene
        .lshape_graphic
        .as_deref()
        .filter(|graphic| !graphic.is_empty())
    {
        swap_lshape(manager, graphic).await?;
    }

    match (scene.lshape_corner.as_deref(), scene.lshape_scale) {
        (Some("default"), _) => {
            control
                .place(None)
                .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
        }
        (None, None) => {}
        (corner, scale) => {
            let corner = match corner {
                Some(corner) => corner.to_string(),
                None => match control.placement() {
                    Some(placement) => placement.corner.to_string(),
                    None => manager.config.read().await.processing.lshape_corner.clone(),
                },
            };

            control
                .place(lshape_placement(Some(&corner), scale)?)
                .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
        }
    }

    match scene.logo {
        SceneLogo::Keep => {}
        SceneLogo::Hide => {
            apply(manager, LogoAction::Hide, None, Place::default())
                .await
                .map(drop)?;
        }
        SceneLogo::Show => {
            apply(manager, LogoAction::Show, None, Place::default())
                .await
                .map(drop)?;
        }
        SceneLogo::Replace => {
            apply(
                manager,
                LogoAction::Replace,
                scene.logo_image.as_deref(),
                Place::default(),
            )
            .await
            .map(drop)?;
        }
    }

    if let Some(position) = scene
        .logo_position
        .as_deref()
        .filter(|position| !position.trim().is_empty())
    {
        let place = Place {
            position: Some(position),
            x: None,
            y: None,
        };

        apply(manager, LogoAction::Move, None, place)
            .await
            .map(drop)?;
    }

    match scene.lshape {
        SceneLShape::Keep => {}
        SceneLShape::On if !control.is_ready() => {
            return Err(ServiceError::Conflict(
                "No L-shape graphic: choose one in Grafismo › L-shape".to_string(),
            ));
        }
        SceneLShape::On => control.on(),
        SceneLShape::Off => control.off(),
    }

    Ok(())
}

/// Undo what `scene` put on air, when a scheduled graphics event ends: the
/// L-shape it brought in goes out, its graphic and place go back to the
/// channel's, and the logo it hid, swapped or moved comes back as the
/// channel has it.
pub(crate) async fn revert_graphics(
    manager: &ChannelManager,
    scene: &GraphicsScene,
) -> Result<(), ServiceError> {
    let control = &manager.lshape_control;

    if scene.lshape == SceneLShape::On {
        control.off();
    }
    if scene.lshape_graphic.is_some() {
        control.reset();
    }
    if scene.lshape_corner.is_some() || scene.lshape_scale.is_some() {
        control
            .place(None)
            .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
    }

    if matches!(scene.logo, SceneLogo::Hide | SceneLogo::Replace) {
        apply(manager, LogoAction::Show, None, Place::default())
            .await
            .map(drop)?;
    }

    if scene.logo_position.is_some() {
        let place = Place {
            position: Some("default"),
            x: None,
            y: None,
        };

        apply(manager, LogoAction::Move, None, place)
            .await
            .map(drop)?;
    }

    Ok(())
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
    place: Place<'_>,
) -> Result<Json<LogoStatus>, ServiceError> {
    let control = &manager.logo_control;

    match action {
        LogoAction::Hide => control.hide(),
        LogoAction::Show => control.show(),
        LogoAction::Move => {
            let position = logo_position(place.resolve()?.as_deref())?;
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
    use noar_engine::{LShapeCorner, LShapePlacement};

    use super::{Place, key_matches, logo_position, lshape_placement};

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
    fn a_logo_goes_anywhere_by_shares_or_by_name() {
        let shares = |x, y| {
            Place {
                position: None,
                x: Some(x),
                y: Some(y),
            }
            .resolve()
        };

        assert_eq!(shares(90.0, 5.0).unwrap().as_deref(), Some("90%:5%"));
        assert_eq!(shares(33.33, 0.0).unwrap().as_deref(), Some("33.3%:0%"));
        assert!(shares(101.0, 0.0).is_err());
        assert!(shares(f64::NAN, 0.0).is_err());
        assert!(
            Place {
                position: None,
                x: Some(10.0),
                y: None
            }
            .resolve()
            .is_err()
        );
        assert_eq!(
            logo_position(Some("center")).unwrap().as_deref(),
            Some("W/2-w/2:H/2-h/2")
        );
        assert_eq!(
            logo_position(Some("bottom")).unwrap().as_deref(),
            Some("W/2-w/2:H-h-12")
        );
    }

    #[test]
    fn the_lshape_picture_goes_to_a_corner_or_back() {
        assert_eq!(
            lshape_placement(Some("bottom-left"), Some(70.0)).unwrap(),
            Some(LShapePlacement {
                corner: LShapeCorner::BottomLeft,
                scale: Some(70.0)
            })
        );
        assert_eq!(lshape_placement(Some("default"), None).unwrap(), None);
        assert!(lshape_placement(Some("50%:100%"), None).is_err());
        assert!(lshape_placement(None, Some(60.0)).is_err());
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
