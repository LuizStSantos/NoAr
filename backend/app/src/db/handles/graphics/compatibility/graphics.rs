//! Legacy configuration adapters backed exclusively by editable Graphics models.
//!
//! Reads project layers into the old DTO contract; saves update model layers and
//! bindings inside the caller's transaction. Frozen processing columns are only
//! read by the nested one-time conversion path, never as a runtime fallback.
//! Native-layer helpers are private here and shared with that child conversion.

use sqlx::sqlite::SqlitePool;

#[path = "conversion/graphics.rs"]
mod conversion;

use crate::utils::errors::ProcessError;

pub use conversion::{
    convert_channel_graphics, convert_channel_graphics_on, initialize_channel_graphics_on,
};

fn image_layers(image: &str, opacity: f64) -> String {
    serde_json::json!([{
        "id": "image", "name": "Image", "kind": "image", "visible": true,
        "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0,
        "image": image, "opacity": opacity,
        "animation_in": "none", "animation_out": "none"
    }])
    .to_string()
}

fn controlled_image<'a>(
    layers: &'a [crate::graphics::TemplateLayer],
    target: Option<&str>,
) -> Option<&'a crate::graphics::TemplateLayer> {
    layers.iter().find(|layer| {
        layer.kind == crate::graphics::LayerKind::Image && Some(layer.id.as_str()) == target
    })
}

fn layer_pixels(value: f64, total: u32) -> u32 {
    ((value / 100.0 * f64::from(total)).round().max(0.0) as u32) & !1
}

/// Legacy DTOs are adapters computed from layers, never a second saved layout.
fn project_layer_processing(
    role: &str,
    enabled: bool,
    layers: &[crate::graphics::TemplateLayer],
    target: Option<&str>,
    config: &mut crate::db::models::Configuration,
    dimensions: (u32, u32),
) {
    use crate::graphics::{Animation, LayerKind};

    let image = controlled_image(layers, target);
    let path = image
        .and_then(|layer| layer.image.clone())
        .unwrap_or_default();

    if role == "logo" {
        config.processing_add_logo = enabled;
        config.processing_logo = path;
        config.processing_logo_scale = "2:2".to_string();
        config.processing_logo_position = "0:0".to_string();
        config.processing_logo_opacity = 1.0;
        config.processing_logo_transition_in = "cut".to_string();
        config.processing_logo_transition_out = "cut".to_string();
        config.processing_logo_transition_seconds = None;

        if let Some(layer) = image {
            let width = layer_pixels(layer.width, dimensions.0).max(2);
            let height = layer_pixels(layer.height, dimensions.1).max(2);
            config.processing_logo_scale = format!("{width}:{height}");
            config.processing_logo_position = format!(
                "{}:{}",
                layer_pixels(layer.x, dimensions.0),
                layer_pixels(layer.y, dimensions.1)
            );
            config.processing_logo_opacity = layer.opacity;
            config.processing_logo_transition_in = if layer.animation_in == Animation::None {
                "cut"
            } else {
                "fade"
            }
            .to_string();
            config.processing_logo_transition_out = if layer.animation_out == Animation::None {
                "cut"
            } else {
                "fade"
            }
            .to_string();
            config.processing_logo_transition_seconds =
                if layer.animation_ms == 400 && layer.clip_animation_ms == Some(1000) {
                    None
                } else {
                    let milliseconds = layer.clip_animation_ms.unwrap_or(layer.animation_ms);

                    if milliseconds == 0 {
                        config.processing_logo_transition_in = "cut".to_string();
                        config.processing_logo_transition_out = "cut".to_string();
                        None
                    } else {
                        Some(f64::from(milliseconds) / 1000.0)
                    }
                };
        }
    } else if role == "lshape" {
        config.processing_lshape = path;

        if let Some(layer) = layers.iter().find(|layer| layer.kind == LayerKind::Video) {
            config.processing_lshape_scale = layer.width;
            let vertical = if layer.y > 0.0 { "bottom" } else { "top" };
            let horizontal = if layer.x > 0.0 { "right" } else { "left" };
            config.processing_lshape_corner = format!("{vertical}-{horizontal}");
        }
    }
}

async fn native_model_layers(
    connection: &mut sqlx::SqliteConnection,
    channel: i32,
    role: &str,
    image: &str,
    opacity: f64,
    settings: &serde_json::Value,
) -> Result<String, ProcessError> {
    let mut layers: serde_json::Value = serde_json::from_str(&image_layers(image, opacity))
        .map_err(|error| ProcessError::Custom(error.to_string()))?;

    if role == "logo" && !image.is_empty() {
        let output: Option<(String, u32, u32)> = sqlx::query_as("SELECT ch.storage, o.width, o.height FROM channels ch JOIN config c ON c.channel_id = ch.id JOIN config_output o ON o.config_id = c.id AND o.active = 1 WHERE ch.id = $1")
            .bind(channel).fetch_optional(&mut *connection).await?;

        if let Some((storage, width, height)) = output {
            let path = crate::file::norm_abs_path(std::path::Path::new(&storage), image)
                .ok()
                .map(|(path, _, _)| path)
                .filter(|path| path.is_file())
                .or_else(|| {
                    // First-run conversion precedes setup/storage creation. Only
                    // the packaged default may use its original asset here;
                    // custom images must never borrow another image's geometry.
                    (image == "00-assets/logo.png")
                        .then(|| crate::file::default_asset_path("logo.png"))
                        .filter(|path| path.is_file())
                });

            if let Some(path) = path {
                let config = noar_engine::LogoConfig {
                    path: path.to_string_lossy().to_string(),
                    scale: settings["scale"]
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    position: settings["position"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    opacity,
                };

                match noar_engine::logo_layer_geometry(&config, width, height) {
                    Ok((x, y, width, height)) => {
                        layers[0]["x"] = x.into();
                        layers[0]["y"] = y.into();
                        layers[0]["width"] = width.into();
                        layers[0]["height"] = height.into();
                    }
                    Err(error) => {
                        // An unreadable legacy asset must not prevent all channels starting.
                        log::warn!("Channel {channel} logo canvas layout unavailable: {error:#}");
                    }
                }
            }
        }
    }

    if role == "lshape" {
        let scale = settings["scale"].as_f64().unwrap_or(75.0);
        let corner = settings["corner"].as_str().unwrap_or("top-right");
        let (output_width, output_height): (u32, u32) = sqlx::query_as("SELECT o.width, o.height FROM config_output o JOIN config c ON c.id = o.config_id WHERE c.channel_id = $1 AND o.active = 1")
            .bind(channel).fetch_optional(&mut *connection).await?.unwrap_or((1280, 720));
        // Match the native window's even-pixel rounding before expressing it as
        // percentages. Rounding (100 - scale) independently can shift a corner by 2 px.
        let window_width = ((f64::from(output_width) * scale / 100.0).round() as u32 & !1).max(2);
        let window_height = ((f64::from(output_height) * scale / 100.0).round() as u32 & !1).max(2);
        let x = if corner.ends_with("right") {
            output_width.saturating_sub(window_width) & !1
        } else {
            0
        };
        let y = if corner.starts_with("bottom") {
            output_height.saturating_sub(window_height) & !1
        } else {
            0
        };
        let percent = |pixels: u32, total: u32| f64::from(pixels) * 100.0 / f64::from(total.max(1));
        layers.as_array_mut().unwrap().push(serde_json::json!({
            "id": "picture", "name": "Picture", "kind": "video", "visible": true,
            "x": percent(x, output_width), "y": percent(y, output_height),
            "width": percent(window_width, output_width), "height": percent(window_height, output_height),
            "animation_in": "none", "animation_out": "none"
        }));
    }

    Ok(layers.to_string())
}

async fn insert_converted_model(
    connection: &mut sqlx::SqliteConnection,
    channel: i32,
    name: &str,
    group: &str,
    layers: &str,
    settings: &serde_json::Value,
) -> Result<i32, ProcessError> {
    let mut candidate = name.to_string();
    let mut suffix = 2;

    while sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM graphics_templates WHERE channel_id = $1 AND name = $2 COLLATE NOCASE)")
        .bind(channel).bind(&candidate).fetch_one(&mut *connection).await? {
        candidate = format!("{name} ({suffix})");
        suffix += 1;
    }

    Ok(sqlx::query_scalar("INSERT INTO graphics_templates(channel_id, name, group_name, duration, layers, settings) VALUES ($1, $2, $3, 0, $4, $5) RETURNING id")
        .bind(channel).bind(candidate).bind(group).bind(layers).bind(settings.to_string())
        .fetch_one(&mut *connection).await?)
}

/// Project the compatibility API from models. No frozen graphics column is read.
pub async fn project_channel_graphics(
    pool: &SqlitePool,
    config: &mut crate::db::models::Configuration,
) -> Result<(), ProcessError> {
    let dimensions: (u32, u32) =
        sqlx::query_as("SELECT width,height FROM config_output WHERE id=$1")
            .bind(config.output_id)
            .fetch_one(pool)
            .await?;
    let rows: Vec<(String, bool, String, Option<String>)> = sqlx::query_as(
        "SELECT b.role,b.enabled,t.layers,b.control_layer_id FROM graphics_channel_bindings b
         JOIN graphics_templates t ON t.id=b.template_id AND t.channel_id=b.channel_id WHERE b.channel_id=$1",
    ).bind(config.channel_id).fetch_all(pool).await?;

    for (role, enabled, layers, target) in rows {
        let layers: Vec<crate::graphics::TemplateLayer> = serde_json::from_str(&layers)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        project_layer_processing(
            &role,
            enabled,
            &layers,
            target.as_deref(),
            config,
            dimensions,
        );
    }

    let scenes: Vec<String> = sqlx::query_scalar("SELECT json_extract(t.settings, '$.scene') FROM graphics_scene_bindings b JOIN graphics_templates t ON t.id = b.template_id AND t.channel_id = b.channel_id WHERE b.channel_id = $1 ORDER BY b.position")
        .bind(config.channel_id).fetch_all(pool).await?;
    config.processing_graphics_scenes = format!("[{}]", scenes.join(","));
    Ok(())
}

/// Compatibility settings pages update bound models inside their existing transaction.
pub async fn save_channel_graphics_on(
    connection: &mut sqlx::SqliteConnection,
    config_id: i32,
    processing: &crate::utils::config::Processing,
) -> Result<(), ProcessError> {
    let dimensions = sqlx::query_as(
        "SELECT o.width,o.height FROM config_output o WHERE o.config_id=$1 AND o.active=1",
    )
    .bind(config_id)
    .fetch_one(&mut *connection)
    .await?;

    save_channel_graphics_with_projection_on(connection, config_id, processing, dimensions).await
}

/// Compare against the output used to project the incoming DTO before an output update.
pub async fn save_channel_graphics_with_projection_on(
    connection: &mut sqlx::SqliteConnection,
    config_id: i32,
    processing: &crate::utils::config::Processing,
    projection_dimensions: (u32, u32),
) -> Result<(), ProcessError> {
    let channel: i32 = sqlx::query_scalar("SELECT channel_id FROM config WHERE id = $1")
        .bind(config_id)
        .fetch_one(&mut *connection)
        .await?;
    let logo_settings = serde_json::json!({
        "scale": processing.logo_scale,
        "position": processing.logo_position,
        "transition_in": processing.logo_transition_in,
        "transition_out": processing.logo_transition_out,
        "transition_seconds": processing.logo_transition_seconds,
    });
    let lshape_settings =
        serde_json::json!({"scale": processing.lshape_scale, "corner": processing.lshape_corner});

    for (role, path, opacity, settings) in [
        (
            "logo",
            &processing.logo,
            processing.logo_opacity,
            logo_settings,
        ),
        ("lshape", &processing.lshape, 1.0, lshape_settings),
    ] {
        let template_id: Option<i32> = sqlx::query_scalar(
            "SELECT template_id FROM graphics_channel_bindings WHERE channel_id = $1 AND role = $2",
        )
        .bind(channel)
        .bind(role)
        .fetch_optional(&mut *connection)
        .await?;
        let template_id = match template_id {
            Some(id) => id,
            None if path.is_empty() => continue,
            None => {
                let layers =
                    native_model_layers(connection, channel, role, path, opacity, &settings)
                        .await?;
                let id = insert_converted_model(
                    connection,
                    channel,
                    role,
                    "",
                    &layers,
                    &serde_json::json!({}),
                )
                .await?;
                sqlx::query("INSERT INTO graphics_channel_bindings(channel_id,role,template_id,enabled,layers_version,control_layer_id) VALUES($1,$2,$3,$4,1,'image')")
                    .bind(channel).bind(role).bind(id).bind(role == "logo" && processing.add_logo)
                    .execute(&mut *connection).await?;
                id
            }
        };
        let saved: String = sqlx::query_scalar(
            "SELECT layers FROM graphics_templates WHERE id = $1 AND channel_id = $2",
        )
        .bind(template_id)
        .bind(channel)
        .fetch_one(&mut *connection)
        .await?;
        let mut layers: Vec<crate::graphics::TemplateLayer> = serde_json::from_str(&saved)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let target_id: Option<String> = sqlx::query_scalar("SELECT control_layer_id FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2")
            .bind(channel).bind(template_id).fetch_one(&mut *connection).await?;
        let native =
            native_model_layers(connection, channel, role, path, opacity, &settings).await?;
        let native: Vec<crate::graphics::TemplateLayer> = serde_json::from_str(&native)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let target = layers
            .iter_mut()
            .find(|layer| Some(layer.id.as_str()) == target_id.as_deref());

        if let Some(target) = target {
            let geometry = &native[0];
            target.image = Some(path.to_string());

            if role == "logo" {
                target.opacity = opacity;
                let projected_position = format!(
                    "{}:{}",
                    layer_pixels(target.x, projection_dimensions.0),
                    layer_pixels(target.y, projection_dimensions.1)
                );
                let projected_scale = format!(
                    "{}:{}",
                    layer_pixels(target.width, projection_dimensions.0).max(2),
                    layer_pixels(target.height, projection_dimensions.1).max(2)
                );

                // The compatibility DTO rounds authored percentages to even pixels.
                // Preserve each unchanged pair, including on an output-size save.
                if processing.logo_position != projected_position {
                    target.x = geometry.x;
                    target.y = geometry.y;
                }

                if processing.logo_scale != projected_scale {
                    target.width = geometry.width;
                    target.height = geometry.height;
                }
            }
        } else if !path.is_empty() {
            let mut image = native[0].clone();
            image.id = uuid::Uuid::new_v4().to_string();
            layers.push(image);
        }

        if role == "logo"
            && let Some(target) = layers.iter_mut().find(|layer| {
                target_id
                    .as_deref()
                    .map_or(layer.kind == crate::graphics::LayerKind::Image, |id| {
                        layer.id == id
                    })
            })
        {
            use crate::graphics::Animation;
            use crate::utils::config::LogoTransition;

            let legacy_in = if target.animation_in == Animation::None {
                LogoTransition::Cut
            } else {
                LogoTransition::Fade
            };
            let legacy_out = if target.animation_out == Animation::None {
                LogoTransition::Cut
            } else {
                LogoTransition::Fade
            };

            if processing.logo_transition_in != legacy_in {
                target.animation_in = if processing.logo_transition_in == LogoTransition::Cut {
                    Animation::None
                } else {
                    Animation::Fade
                };
            }

            if processing.logo_transition_out != legacy_out {
                target.animation_out = if processing.logo_transition_out == LogoTransition::Cut {
                    Animation::None
                } else {
                    Animation::Fade
                };
            }

            let old_clip = target.clip_animation_ms.unwrap_or(target.animation_ms);
            let clip = processing
                .logo_transition_seconds
                .map_or(1000, |seconds| (seconds * 1000.0).round() as u32);

            if old_clip != clip {
                target.animation_ms = processing
                    .logo_transition_seconds
                    .map_or(400, |seconds| (seconds * 1000.0).round() as u32);
                target.clip_animation_ms = Some(clip);
            }
        }

        if role == "lshape" {
            if let Some(picture) = layers
                .iter_mut()
                .find(|layer| layer.kind == crate::graphics::LayerKind::Video)
            {
                let vertical = if picture.y > 0.0 { "bottom" } else { "top" };
                let horizontal = if picture.x > 0.0 { "right" } else { "left" };

                if (picture.width - processing.lshape_scale).abs() > 0.000001
                    || format!("{vertical}-{horizontal}") != processing.lshape_corner
                {
                    *picture = native[1].clone();
                }
            } else {
                layers.push(native[1].clone());
            }
        }

        if target_id.is_none() {
            let target = layers
                .iter()
                .find(|layer| layer.kind == crate::graphics::LayerKind::Image)
                .map(|layer| layer.id.as_str());
            sqlx::query("UPDATE graphics_channel_bindings SET control_layer_id=$3 WHERE channel_id=$1 AND template_id=$2")
                .bind(channel).bind(template_id).bind(target).execute(&mut *connection).await?;
        }

        let layers = serde_json::to_string(&layers)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let updated = sqlx::query("UPDATE graphics_templates SET layers = $3, updated_at = CURRENT_TIMESTAMP WHERE channel_id = $1 AND id = $2")
            .bind(channel).bind(template_id).bind(layers)
            .execute(&mut *connection).await?;

        if updated.rows_affected() != 1 {
            return Err(ProcessError::Custom(format!(
                "Missing {role} binding for channel {channel}"
            )));
        }
    }

    sqlx::query(
        "UPDATE graphics_channel_bindings SET enabled = $2 WHERE channel_id = $1 AND role = 'logo'",
    )
    .bind(channel)
    .bind(processing.add_logo)
    .execute(&mut *connection)
    .await?;

    sqlx::query("UPDATE graphics_templates SET permanent=$2 WHERE id IN (SELECT template_id FROM graphics_channel_bindings WHERE channel_id=$1 AND role='logo')")
        .bind(channel).bind(processing.add_logo).execute(&mut *connection).await?;

    // Reconcile by slug so reordering or an unchanged settings save keeps model IDs.
    let existing: Vec<(i32, String)> = sqlx::query_as("SELECT t.id, json_extract(t.settings, '$.scene') FROM graphics_scene_bindings b JOIN graphics_templates t ON t.id = b.template_id WHERE b.channel_id = $1")
        .bind(channel).fetch_all(&mut *connection).await?;
    sqlx::query("DELETE FROM graphics_scene_bindings WHERE channel_id = $1")
        .bind(channel)
        .execute(&mut *connection)
        .await?;
    let mut used = std::collections::HashSet::new();

    for (position, scene) in processing.graphics_scenes.iter().enumerate() {
        let found = existing.iter().find(|(_, json)| {
            serde_json::from_str::<crate::utils::config::GraphicsScene>(json)
                .is_ok_and(|saved| saved.slug() == scene.slug())
        });
        let settings = serde_json::json!({"scene": scene});
        let template = if let Some((id, _)) = found {
            sqlx::query("UPDATE graphics_templates SET settings = $2, updated_at = CURRENT_TIMESTAMP WHERE id = $1")
                .bind(id).bind(settings.to_string()).execute(&mut *connection).await?;
            *id
        } else {
            insert_converted_model(connection, channel, &scene.name, "Scenes", "[]", &settings)
                .await?
        };
        used.insert(template);
        sqlx::query("INSERT INTO graphics_scene_bindings(channel_id, template_id, position) VALUES ($1, $2, $3)")
            .bind(channel).bind(template).bind(position as i64).execute(&mut *connection).await?;
    }

    for (template, _) in existing {
        if !used.contains(&template) {
            sqlx::query("DELETE FROM graphics_templates WHERE id = $1 AND channel_id = $2")
                .bind(template)
                .bind(channel)
                .execute(&mut *connection)
                .await?;
        }
    }

    Ok(())
}
