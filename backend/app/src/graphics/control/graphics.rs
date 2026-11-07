//! Apply channel-model control actions independently of HTTP and scheduling.
//!
//! HTTP adapters, saved scenes and timed events share these operations. Keep
//! placement validation, storage checks and action order here so each caller
//! reaches the same runtime state without depending on an API route.

use noar_engine::{LShapeCorner, LShapePlacement, LogoConfig};
use serde::{Deserialize, Serialize};

use crate::{
    file::norm_abs_path,
    player::controller::ChannelManager,
    utils::{
        config::{GraphicsScene, SceneLShape, SceneLogo, slug},
        errors::ServiceError,
        xmltv::library::has_image_extension,
    },
};

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

/// Where to put something: `position`, or the shares `x` and `y`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LogoPlacement<'a> {
    pub(crate) position: Option<&'a str>,
    pub(crate) x: Option<f64>,
    pub(crate) y: Option<f64>,
}

impl<'a> LogoPlacement<'a> {
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
fn resolve_logo_position(position: Option<&str>) -> Result<Option<String>, ServiceError> {
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

/// Where the picture goes for `place` and `scale`: `Ok(None)` puts it back
/// where the graphic places it.
fn resolve_lshape_placement(
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

pub(crate) async fn apply_lshape_action(
    manager: &ChannelManager,
    action: LShapeAction,
    graphic: Option<&str>,
    position: Option<&str>,
    scale: Option<f64>,
) -> Result<LShapeStatus, ServiceError> {
    let control = &manager.lshape_control;
    let graphic = graphic.map(str::trim).filter(|graphic| !graphic.is_empty());

    // Placed first, so `on` shrinks the picture straight to its new corner.
    if action == LShapeAction::Move || (action == LShapeAction::On && position.is_some()) {
        control
            .place(resolve_lshape_placement(position, scale)?)
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

    Ok(lshape_status(manager))
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

    tokio::task::spawn_blocking(move || control.replace_model_art(&label, &lshape, width, height))
        .await
        .map_err(|_| ServiceError::InternalServerError)?
        .map_err(|error| {
            ServiceError::BadRequest(format!(
                "{relative}: the L-shape graphic cannot be read ({})",
                error.root_cause()
            ))
        })
}

pub(crate) fn lshape_status(manager: &ChannelManager) -> LShapeStatus {
    let placement = manager.lshape_control.placement();

    LShapeStatus {
        on: manager.lshape_control.is_on(),
        ready: manager.lshape_control.is_ready(),
        replacement: manager.lshape_control.replacement(),
        position: placement.map(|placement| placement.corner.to_string()),
        scale: placement.and_then(|placement| placement.scale),
    }
}

#[derive(Debug, Serialize)]
pub struct SceneStatus {
    /// The scene's name as written.
    scene: String,
    logo: LogoStatus,
    lshape: LShapeStatus,
}

/// Apply the scene called `name`: the L-shape graphic and place first, then
/// the logo, then the L-shape in or out.
pub(crate) async fn apply_named_scene(
    manager: &ChannelManager,
    name: &str,
) -> Result<SceneStatus, ServiceError> {
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

    apply_scene_actions(manager, &scene).await?;

    Ok(SceneStatus {
        scene: scene.name,
        logo: logo_status(manager),
        lshape: lshape_status(manager),
    })
}

/// Put the changes of `scene` on air: the L-shape graphic and place first,
/// then the logo, then the L-shape in or out. A scene or a scheduled
/// graphics event.
pub(crate) async fn apply_scene_actions(
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
                .place(resolve_lshape_placement(Some(&corner), scale)?)
                .map_err(|error| ServiceError::BadRequest(format!("{error:#}")))?;
        }
    }

    match scene.logo {
        SceneLogo::Keep => {}
        SceneLogo::Hide => {
            apply_logo_action(manager, LogoAction::Hide, None, LogoPlacement::default())
                .await
                .map(drop)?;
        }
        SceneLogo::Show => {
            apply_logo_action(manager, LogoAction::Show, None, LogoPlacement::default())
                .await
                .map(drop)?;
        }
        SceneLogo::Replace => {
            apply_logo_action(
                manager,
                LogoAction::Replace,
                scene.logo_image.as_deref(),
                LogoPlacement::default(),
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
        let place = LogoPlacement {
            position: Some(position),
            x: None,
            y: None,
        };

        apply_logo_action(manager, LogoAction::Move, None, place)
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
pub(crate) async fn revert_scene_actions(
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
        apply_logo_action(manager, LogoAction::Show, None, LogoPlacement::default())
            .await
            .map(drop)?;
    }

    if scene.logo_position.is_some() {
        let place = LogoPlacement {
            position: Some("default"),
            x: None,
            y: None,
        };

        apply_logo_action(manager, LogoAction::Move, None, place)
            .await
            .map(drop)?;
    }

    Ok(())
}

pub(crate) async fn apply_logo_action(
    manager: &ChannelManager,
    action: LogoAction,
    logo: Option<&str>,
    place: LogoPlacement<'_>,
) -> Result<LogoStatus, ServiceError> {
    let control = &manager.logo_control;

    match action {
        LogoAction::Hide => control.hide(),
        LogoAction::Show => control.show(),
        LogoAction::Move => {
            let position = resolve_logo_position(place.resolve()?.as_deref())?;
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

    Ok(logo_status(manager))
}

pub(crate) fn logo_status(manager: &ChannelManager) -> LogoStatus {
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

    use super::{LogoPlacement, resolve_logo_position, resolve_lshape_placement};

    #[test]
    fn a_logo_goes_to_a_corner_an_expression_or_back() {
        assert_eq!(
            resolve_logo_position(Some("bottom-left"))
                .unwrap()
                .as_deref(),
            Some("12:H-h-12")
        );
        assert_eq!(
            resolve_logo_position(Some(" W/2-w/2:12 "))
                .unwrap()
                .as_deref(),
            Some("W/2-w/2:12")
        );
        assert_eq!(resolve_logo_position(Some("default")).unwrap(), None);
        assert!(resolve_logo_position(None).is_err());
    }

    #[test]
    fn a_logo_goes_anywhere_by_shares_or_by_name() {
        let shares = |x, y| {
            LogoPlacement {
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
            LogoPlacement {
                position: None,
                x: Some(10.0),
                y: None
            }
            .resolve()
            .is_err()
        );
        assert_eq!(
            resolve_logo_position(Some("center")).unwrap().as_deref(),
            Some("W/2-w/2:H/2-h/2")
        );
        assert_eq!(
            resolve_logo_position(Some("bottom")).unwrap().as_deref(),
            Some("W/2-w/2:H-h-12")
        );
    }

    #[test]
    fn the_lshape_picture_goes_to_a_corner_or_back() {
        assert_eq!(
            resolve_lshape_placement(Some("bottom-left"), Some(70.0)).unwrap(),
            Some(LShapePlacement {
                corner: LShapeCorner::BottomLeft,
                scale: Some(70.0)
            })
        );
        assert_eq!(
            resolve_lshape_placement(Some("default"), None).unwrap(),
            None
        );
        assert!(resolve_lshape_placement(Some("50%:100%"), None).is_err());
        assert!(resolve_lshape_placement(None, Some(60.0)).is_err());
    }
}
