//! Bound channel-model views and visibility controls.
//!
//! These operations share runtime handles with clip marks and external commands;
//! template validation and temporary-take ownership live in the parent domain.

use std::collections::HashMap;

use serde::Serialize;

use crate::{
    db::models::GraphicsTemplate, player::controller::ChannelManager, utils::errors::ServiceError,
};

use super::{
    LayerKind, fill_fields, graphic_spec_for_channel, load_permanent_templates, sources,
    templates::layer_image,
};

/// Whether a saved model has visible artwork or text, not just a picture window.
pub(crate) fn graphic_has_content(
    template: &GraphicsTemplate,
    config: &crate::utils::config::PlayoutConfig,
) -> bool {
    let Ok(values) = sources::resolved_values(template, &HashMap::new(), &HashMap::new(), config)
    else {
        return false;
    };

    template.layers.iter().any(|layer| {
        layer.visible
            && match layer.kind {
                LayerKind::Image => layer_image(
                    layer.image.as_deref().unwrap_or_default(),
                    &values,
                    &config.channel.storage,
                )
                .is_ok_and(|path| path.is_some_and(|path| path.is_file())),
                LayerKind::Text => !fill_fields(&layer.text, &values).trim().is_empty(),
                LayerKind::Clock => true,
                LayerKind::Box => layer.fill.is_some() || layer.style.stroke.is_some(),
                LayerKind::Video => false,
            }
    })
}

/// A channel model and its current control state, separate from temporary GCs.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelGraphicView {
    pub kind: String,
    pub template: GraphicsTemplate,
    pub on: bool,
    pub ready: bool,
    pub scene_name: Option<String>,
}

pub async fn channel_graphics(
    manager: &ChannelManager,
) -> Result<Vec<ChannelGraphicView>, ServiceError> {
    use crate::db::{handles, models::GraphicsRole};
    use crate::utils::config::{SceneLShape, SceneLogo};

    let templates = handles::select_graphics_templates(&manager.db_pool, manager.id).await?;
    let bindings = handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let scenes = handles::select_graphics_scene_templates(&manager.db_pool, manager.id).await?;
    let config = manager.config.read().await.clone();
    let mut result = Vec::new();

    for binding in &bindings {
        let template = templates
            .iter()
            .find(|template| template.id == binding.template_id)
            .ok_or_else(|| ServiceError::Conflict("Missing channel graphics model".to_string()))?
            .clone();
        let ready = graphic_has_content(&template, &config);
        let on = match binding.role {
            GraphicsRole::Logo => manager.logo_control.is_visible(),
            GraphicsRole::Ticker => {
                manager
                    .graphics_control
                    .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
                    == Some(true)
            }
            GraphicsRole::Lshape => manager.lshape_control.is_on(),
        };
        result.push(ChannelGraphicView {
            kind: binding.role.as_str().to_string(),
            template,
            on,
            ready,
            scene_name: None,
        });
    }

    for template in templates.iter().filter(|template| {
        template.always_on == Some(true)
            && !bindings
                .iter()
                .any(|binding| binding.template_id == template.id)
    }) {
        result.push(ChannelGraphicView {
            kind: "permanent".to_string(),
            ready: graphic_has_content(template, &config),
            on: manager
                .graphics_control
                .persistent_visible(template.id as u64)
                .unwrap_or(false),
            template: template.clone(),
            scene_name: None,
        });
    }

    for (id, scene) in scenes {
        let mut template = templates
            .iter()
            .find(|template| template.id == id)
            .ok_or_else(|| ServiceError::Conflict("Missing scene model".to_string()))?
            .clone();
        let picture = scene.lshape == SceneLShape::On
            || (scene.lshape == SceneLShape::Keep && manager.lshape_control.is_on());
        template.layers = Vec::new();

        if picture && let Some(base) = result.iter().find(|view| view.kind == "lshape") {
            template.layers = base.template.layers.clone();

            if let Some(image) = scene.lshape_graphic.as_ref()
                && let Some(layer) = template
                    .layers
                    .iter_mut()
                    .find(|layer| layer.kind == LayerKind::Image)
            {
                layer.image = Some(image.clone());
            }
        }

        if !picture
            && scene.logo != SceneLogo::Hide
            && let Some(base) = result.iter().find(|view| view.kind == "logo")
            && (base.on || matches!(scene.logo, SceneLogo::Show | SceneLogo::Replace))
        {
            let mut layers = base.template.layers.clone();

            if scene.logo == SceneLogo::Replace
                && let Some(layer) = layers
                    .iter_mut()
                    .find(|layer| layer.kind == LayerKind::Image)
            {
                layer.image = scene.logo_image.clone();
            }

            template.layers.extend(layers);
        }

        result.push(ChannelGraphicView {
            kind: "scene".to_string(),
            template,
            on: false,
            ready: true,
            scene_name: Some(scene.name),
        });
    }

    Ok(result)
}

/// All controls share the handles used by clip marks and legacy HTTP routes.
pub async fn control_channel_graphic(
    manager: &ChannelManager,
    template_id: i32,
    on: bool,
) -> Result<(), ServiceError> {
    use crate::db::{handles, models::GraphicsRole};

    let bindings = handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;

    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.template_id == template_id)
    {
        let views = channel_graphics(manager).await?;
        let view = views
            .iter()
            .find(|view| view.template.id == template_id)
            .ok_or_else(|| ServiceError::NotFound("Channel model not found".to_string()))?;

        if on && !view.ready {
            return Err(ServiceError::Conflict(
                "Configure this channel model before putting it on air".to_string(),
            ));
        }

        if on && !binding.enabled && binding.role != GraphicsRole::Lshape {
            let mut enabled = binding.clone();
            enabled.enabled = true;
            handles::save_graphics_binding(&manager.db_pool, &enabled).await?;
            let config = crate::utils::config::get_config(&manager.db_pool, manager.id).await?;
            let running = manager.running_config();
            let (width, height) = running
                .as_deref()
                .map_or((config.output.width, config.output.height), |running| {
                    (running.output.width, running.output.height)
                });
            crate::player::output::load_logo(manager, &config, width, height);
            manager.update_config(config).await;
        }

        match binding.role {
            GraphicsRole::Logo => {
                if on {
                    let config = manager.config.read().await.clone();
                    let running = manager.running_config();
                    let (width, height) = running
                        .as_deref()
                        .map_or((config.output.width, config.output.height), |running| {
                            (running.output.width, running.output.height)
                        });
                    crate::player::output::load_logo(manager, &config, width, height);
                    manager.logo_control.show();
                } else {
                    manager.logo_control.hide();
                }
            }
            GraphicsRole::Ticker => {
                if manager
                    .graphics_control
                    .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
                    .is_none()
                {
                    load_channel_ticker(manager).await?;
                }

                manager
                    .graphics_control
                    .set_always_on_visible(noar_engine::AlwaysOnSlot::Ticker, on);
            }
            GraphicsRole::Lshape => {
                if on && !manager.lshape_control.is_ready() {
                    let config = manager.config.read().await.clone();
                    crate::player::output::load_lshape(manager, &config).await;
                }

                if on && !manager.lshape_control.is_ready() {
                    return Err(ServiceError::Conflict(
                        "The L-shape artwork cannot be loaded".to_string(),
                    ));
                }

                if on {
                    manager.lshape_control.on();
                } else {
                    manager.lshape_control.off();
                }
            }
        }

        return Ok(());
    }

    if let Some(template) =
        handles::select_graphics_template(&manager.db_pool, manager.id, template_id).await?
        && template.always_on == Some(true)
    {
        if manager
            .graphics_control
            .persistent_visible(template_id as u64)
            .is_none()
        {
            load_permanent_templates(manager).await?;
        }

        manager
            .graphics_control
            .set_persistent_visible(template_id as u64, on);
        return Ok(());
    }

    let scenes = handles::select_graphics_scene_templates(&manager.db_pool, manager.id).await?;
    let (_, scene) = scenes
        .iter()
        .find(|(id, _)| *id == template_id)
        .ok_or_else(|| ServiceError::NotFound("Channel model not found".to_string()))?;
    if !on {
        return Err(ServiceError::BadRequest(
            "A scene is applied with on:true; use channel controls to change its result"
                .to_string(),
        ));
    }

    super::control::apply_scene_actions(manager, scene).await?;

    Ok(())
}

/// Removing a channel binding also removes its transient external/clip artwork.
/// Ordinary saves and replacement assignments keep those shared controls intact.
pub async fn clear_removed_channel_graphic(
    manager: &ChannelManager,
    role: crate::db::models::GraphicsRole,
) -> Result<(), ServiceError> {
    let config = manager.config.read().await;
    let running = manager.running_config();
    let (width, height) = running
        .as_deref()
        .map_or((config.output.width, config.output.height), |running| {
            (running.output.width, running.output.height)
        });

    match role {
        crate::db::models::GraphicsRole::Logo => {
            manager.logo_control.show();
            manager
                .logo_control
                .move_to(None, width, height)
                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
            manager
                .logo_control
                .set_clip_image(None, width, height)
                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
            manager
                .logo_control
                .set_clip_position(None, width, height)
                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
        }
        crate::db::models::GraphicsRole::Lshape => {
            manager.lshape_control.off();
            manager.lshape_control.reset();
            manager
                .lshape_control
                .place(None)
                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
            manager
                .lshape_control
                .set_clip_graphic(None, width, height)
                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
        }
        crate::db::models::GraphicsRole::Ticker => {}
    }

    Ok(())
}

/// Restore the bound permanent text model. Released 2.2.1 has no persisted
/// ticker settings, so conversion creates an empty, disabled binding.
pub async fn load_channel_ticker(manager: &ChannelManager) -> Result<(), ServiceError> {
    let bindings =
        crate::db::handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let Some(binding) = bindings
        .iter()
        .find(|binding| binding.role == crate::db::models::GraphicsRole::Ticker)
    else {
        manager
            .graphics_control
            .clear_always_on(noar_engine::AlwaysOnSlot::Ticker);
        return load_permanent_templates(manager).await;
    };

    if !binding.enabled {
        manager
            .graphics_control
            .clear_always_on(noar_engine::AlwaysOnSlot::Ticker);
        return load_permanent_templates(manager).await;
    }

    let template = crate::db::handles::select_graphics_template(
        &manager.db_pool,
        manager.id,
        binding.template_id,
    )
    .await?
    .ok_or_else(|| ServiceError::Conflict("Missing channel ticker model".to_string()))?;
    let config = manager.config.read().await;
    let has_content = graphic_has_content(&template, &config);
    let spec = if has_content {
        graphic_spec_for_channel(&template, &HashMap::new(), &config)?
    } else {
        // Preserve an explicit off state when text is cleared and later restored.
        noar_engine::GraphicSpec {
            layers: Vec::new(),
            picture: None,
        }
    };
    let running = manager.running_config();
    let (width, height) = running
        .as_deref()
        .map_or((config.output.width, config.output.height), |running| {
            (running.output.width, running.output.height)
        });
    drop(config);
    let graphic =
        tokio::task::spawn_blocking(move || noar_engine::render_graphic(&spec, width, height))
            .await
            .map_err(|_| ServiceError::InternalServerError)?
            .map_err(|error| {
                ServiceError::Conflict(format!("Channel ticker cannot be rendered: {error:#}"))
            })?;
    let visible = has_content
        && manager
            .graphics_control
            .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
            .unwrap_or(true);
    manager
        .graphics_control
        .set_always_on(noar_engine::AlwaysOnSlot::Ticker, graphic, visible)
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
    load_permanent_templates(manager).await
}
