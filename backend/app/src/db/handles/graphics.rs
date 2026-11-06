use sqlx::sqlite::SqlitePool;

use crate::{
    db::models::{GraphicsChannelBinding, GraphicsRole, GraphicsTemplate},
    utils::errors::ProcessError,
};

/// Scene identities stay independent of editable library labels.
pub async fn select_graphics_scene_templates(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<(i32, crate::utils::config::GraphicsScene)>, ProcessError> {
    let rows: Vec<(i32, String)> = sqlx::query_as(
        "SELECT t.id, json_extract(t.settings, '$.scene')
         FROM graphics_scene_bindings b JOIN graphics_templates t
         ON t.id = b.template_id AND t.channel_id = b.channel_id
         WHERE b.channel_id = $1 ORDER BY b.position",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id, value)| {
            serde_json::from_str(&value)
                .map(|scene| (id, scene))
                .map_err(|error| ProcessError::Custom(error.to_string()))
        })
        .collect()
}

fn layers_json(template: &GraphicsTemplate) -> Result<String, ProcessError> {
    serde_json::to_string(&template.layers).map_err(|error| ProcessError::Custom(error.to_string()))
}

pub async fn select_graphics_templates(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<GraphicsTemplate>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, name, group_name, duration, during_lshape, layers,
         COALESCE((SELECT enabled FROM graphics_channel_bindings b WHERE b.template_id=graphics_templates.id AND b.channel_id=graphics_templates.channel_id AND b.role IN ('logo','ticker')),permanent) AS always_on,
         (SELECT control_layer_id FROM graphics_channel_bindings b WHERE b.template_id = graphics_templates.id AND b.channel_id = graphics_templates.channel_id) AS control_layer_id
         FROM graphics_templates WHERE channel_id = $1
         ORDER BY group_name COLLATE NOCASE, name COLLATE NOCASE, id",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?)
}

pub async fn select_graphics_template(
    pool: &SqlitePool,
    channel_id: i32,
    template_id: i32,
) -> Result<Option<GraphicsTemplate>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, name, group_name, duration, during_lshape, layers,
         COALESCE((SELECT enabled FROM graphics_channel_bindings b WHERE b.template_id=graphics_templates.id AND b.channel_id=graphics_templates.channel_id AND b.role IN ('logo','ticker')),permanent) AS always_on,
         (SELECT control_layer_id FROM graphics_channel_bindings b WHERE b.template_id = graphics_templates.id AND b.channel_id = graphics_templates.channel_id) AS control_layer_id
         FROM graphics_templates WHERE channel_id = $1 AND id = $2",
    )
    .bind(channel_id)
    .bind(template_id)
    .fetch_optional(pool)
    .await?)
}

/// Whether another template of the channel already has this name, ignoring
/// case.
pub async fn graphics_template_name_taken(
    pool: &SqlitePool,
    channel_id: i32,
    name: &str,
    except_id: i32,
) -> Result<bool, ProcessError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM graphics_templates
            WHERE channel_id = $1 AND name = $2 COLLATE NOCASE AND id <> $3)",
    )
    .bind(channel_id)
    .bind(name)
    .bind(except_id)
    .fetch_one(pool)
    .await?)
}

pub async fn insert_graphics_template(
    pool: &SqlitePool,
    template: &GraphicsTemplate,
) -> Result<i32, ProcessError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO graphics_templates (channel_id, name, group_name, duration, during_lshape,
            layers, permanent)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id",
    )
    .bind(template.channel_id)
    .bind(&template.name)
    .bind(&template.group)
    .bind(template.duration)
    .bind(template.during_lshape.as_str())
    .bind(layers_json(template)?)
    .bind(template.always_on)
    .fetch_one(pool)
    .await?)
}

/// `false` when the channel has no such template.
pub async fn update_graphics_template(
    pool: &SqlitePool,
    template: &GraphicsTemplate,
) -> Result<bool, ProcessError> {
    let mut transaction = pool.begin().await?;
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM graphics_channel_bindings WHERE channel_id = $1 AND template_id = $2",
    )
    .bind(template.channel_id)
    .bind(template.id)
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(always_on) = template.always_on {
        sqlx::query("UPDATE graphics_channel_bindings SET enabled = $3 WHERE channel_id = $1 AND template_id = $2")
            .bind(template.channel_id).bind(template.id).bind(always_on)
            .execute(&mut *transaction).await?;
    }

    if matches!(role.as_deref(), Some("logo" | "lshape")) {
        let saved_target: Option<String> = sqlx::query_scalar(
            "SELECT control_layer_id FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2",
        ).bind(template.channel_id).bind(template.id).fetch_one(&mut *transaction).await?;
        let target = template
            .control_layer_id
            .as_deref()
            .or(saved_target.as_deref())
            .and_then(|id| {
                template.layers.iter().find(|layer| {
                    layer.id == id && layer.kind == crate::utils::graphics::LayerKind::Image
                })
            })
            .or_else(|| {
                template
                    .layers
                    .iter()
                    .find(|layer| layer.kind == crate::utils::graphics::LayerKind::Image)
            })
            .map(|layer| layer.id.as_str());
        sqlx::query("UPDATE graphics_channel_bindings SET control_layer_id = $3 WHERE channel_id = $1 AND template_id = $2")
            .bind(template.channel_id).bind(template.id).bind(target)
            .execute(&mut *transaction).await?;
    }

    let result = sqlx::query(
        "UPDATE graphics_templates SET name = $3, group_name = $4, duration = $5,
            during_lshape = $6, layers = $7, permanent = COALESCE($8, permanent), updated_at = CURRENT_TIMESTAMP
         WHERE channel_id = $1 AND id = $2",
    )
    .bind(template.channel_id)
    .bind(template.id)
    .bind(&template.name)
    .bind(&template.group)
    .bind(template.duration)
    .bind(template.during_lshape.as_str())
    .bind(layers_json(template)?)
    .bind(template.always_on)
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// `false` when the channel has no such template.
pub async fn delete_graphics_template(
    pool: &SqlitePool,
    channel_id: i32,
    template_id: i32,
) -> Result<bool, ProcessError> {
    let mut transaction = pool.begin().await?;

    for query in [
        "DELETE FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2",
        "DELETE FROM graphics_scene_bindings WHERE channel_id=$1 AND template_id=$2",
    ] {
        sqlx::query(query)
            .bind(channel_id)
            .bind(template_id)
            .execute(&mut *transaction)
            .await?;
    }

    let result = sqlx::query("DELETE FROM graphics_templates WHERE channel_id = $1 AND id = $2")
        .bind(channel_id)
        .bind(template_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok(result.rows_affected() > 0)
}

/// Whether deletion would break a channel role or a compatibility scene.
pub async fn graphics_template_is_bound(
    pool: &SqlitePool,
    channel: i32,
    template: i32,
) -> Result<bool, ProcessError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM graphics_channel_bindings WHERE channel_id = $1 AND template_id = $2 UNION ALL SELECT 1 FROM graphics_scene_bindings WHERE channel_id = $1 AND template_id = $2)")
        .bind(channel).bind(template).fetch_one(pool).await?)
}

/// Read bindings only when the schema has been installed by migration 00029.
/// Runtime callers must not use the old processing columns as a fallback.
pub async fn select_graphics_bindings(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<GraphicsChannelBinding>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT channel_id, role, template_id, enabled
         FROM graphics_channel_bindings WHERE channel_id = $1 ORDER BY role",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?)
}

/// `false` means the template does not belong to the channel. The guarded
/// insert and update are one statement, so no cross-channel binding is written.
pub async fn save_graphics_binding(
    pool: &SqlitePool,
    binding: &GraphicsChannelBinding,
) -> Result<bool, ProcessError> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO graphics_channel_bindings (channel_id, role, template_id, enabled)
         SELECT $1, $2, id, $4 FROM graphics_templates WHERE channel_id = $1 AND id = $3
         ON CONFLICT(channel_id, role) DO UPDATE
         SET template_id = excluded.template_id, enabled = excluded.enabled",
    )
    .bind(binding.channel_id)
    .bind(binding.role.as_str())
    .bind(binding.template_id)
    .bind(binding.enabled)
    .execute(&mut *transaction)
    .await?;

    if result.rows_affected() > 0
        && matches!(binding.role, GraphicsRole::Logo | GraphicsRole::Ticker)
    {
        sqlx::query("UPDATE graphics_templates SET permanent=$3 WHERE channel_id=$1 AND id=$2")
            .bind(binding.channel_id)
            .bind(binding.template_id)
            .bind(binding.enabled)
            .execute(&mut *transaction)
            .await?;
    }

    transaction.commit().await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_graphics_binding(
    pool: &SqlitePool,
    channel_id: i32,
    role: GraphicsRole,
) -> Result<bool, ProcessError> {
    let result =
        sqlx::query("DELETE FROM graphics_channel_bindings WHERE channel_id = $1 AND role = $2")
            .bind(channel_id)
            .bind(role.as_str())
            .execute(pool)
            .await?;

    Ok(result.rows_affected() > 0)
}

/// Convert all channels atomically. Complete bindings are the conversion marker:
/// restarting must never import the frozen compatibility columns over edited models.
pub async fn convert_channel_graphics(pool: &SqlitePool) -> Result<(), ProcessError> {
    let mut transaction = pool.begin().await?;
    let channels: Vec<i32> =
        sqlx::query_scalar("SELECT channel_id FROM config ORDER BY channel_id")
            .fetch_all(&mut *transaction)
            .await?;

    for channel in channels {
        convert_channel_graphics_on(&mut transaction, channel).await?;
    }

    transaction.commit().await?;
    Ok(())
}

pub async fn convert_channel_graphics_on(
    connection: &mut sqlx::SqliteConnection,
    channel: i32,
) -> Result<(), ProcessError> {
    use sqlx::Row;

    let imported: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM graphics_channel_state WHERE channel_id=$1)",
    )
    .bind(channel)
    .fetch_one(&mut *connection)
    .await?;

    if imported {
        return upgrade_channel_layers_on(connection, channel).await;
    }

    // This is the only reader of the frozen columns, and only for an unconverted channel.
    let row = sqlx::query(
        "SELECT p.* FROM config_processing p JOIN config c ON c.id = p.config_id
         WHERE c.channel_id = $1",
    )
    .bind(channel)
    .fetch_one(&mut *connection)
    .await?;
    let logo: String = row.try_get("logo")?;
    let lshape: String = row.try_get("lshape")?;
    let logo_settings = serde_json::json!({
        "scale": row.try_get::<String, _>("logo_scale")?,
        "position": row.try_get::<String, _>("logo_position")?,
        "transition_in": row.try_get::<String, _>("logo_transition_in")?,
        "transition_out": row.try_get::<String, _>("logo_transition_out")?,
        "transition_seconds": row.try_get::<Option<f64>, _>("logo_transition_seconds")?,
    });
    let lshape_settings = serde_json::json!({
        "scale": row.try_get::<f64, _>("lshape_scale")?,
        "corner": row.try_get::<String, _>("lshape_corner")?,
    });
    let scenes: String = row.try_get("graphics_scenes")?;
    let scenes: Vec<crate::utils::config::GraphicsScene> = serde_json::from_str(&scenes)
        .map_err(|error| ProcessError::Custom(format!("Channel {channel} scenes: {error}")))?;

    for (position, scene) in scenes.iter().enumerate() {
        let template = insert_converted_model(
            connection,
            channel,
            &scene.name,
            "Scenes",
            "[]",
            &serde_json::json!({"scene": scene}),
        )
        .await?;
        sqlx::query("INSERT INTO graphics_scene_bindings(channel_id, template_id, position) VALUES ($1, $2, $3)")
            .bind(channel).bind(template).bind(position as i64)
            .execute(&mut *connection).await?;
    }

    let opacity: f64 = row.try_get("logo_opacity")?;
    let logo_enabled: bool = row.try_get("add_logo")?;

    for (role, image, settings, enabled, opacity) in [
        ("logo", logo, logo_settings, logo_enabled, opacity),
        ("ticker", String::new(), serde_json::json!({}), false, 1.0),
        ("lshape", lshape, lshape_settings, false, 1.0),
    ] {
        let layers = if role == "ticker" {
            "[]".to_string()
        } else {
            native_model_layers(connection, channel, role, &image, opacity, &settings).await?
        };
        let template = insert_converted_model(
            connection,
            channel,
            role,
            "Channel graphics",
            &layers,
            &settings,
        )
        .await?;
        sqlx::query("INSERT INTO graphics_channel_bindings(channel_id, role, template_id, enabled) VALUES ($1, $2, $3, $4)")
            .bind(channel).bind(role).bind(template).bind(enabled)
            .execute(&mut *connection).await?;
    }

    upgrade_channel_layers_on(connection, channel).await?;
    sqlx::query("UPDATE graphics_templates SET permanent=(SELECT enabled FROM graphics_channel_bindings b WHERE b.template_id=graphics_templates.id AND b.role IN ('logo','ticker')) WHERE channel_id=$1")
        .bind(channel).execute(&mut *connection).await?;
    sqlx::query("INSERT INTO graphics_channel_state(channel_id) VALUES($1)")
        .bind(channel)
        .execute(&mut *connection)
        .await?;

    Ok(())
}

/// A new channel starts with one removable logo example, not empty role cards.
pub async fn initialize_channel_graphics_on(
    connection: &mut sqlx::SqliteConnection,
    channel: i32,
) -> Result<(), ProcessError> {
    convert_channel_graphics_on(connection, channel).await?;
    let unused: Vec<i32> = sqlx::query_scalar("SELECT template_id FROM graphics_channel_bindings WHERE channel_id=$1 AND role IN ('ticker','lshape')")
        .bind(channel).fetch_all(&mut *connection).await?;
    sqlx::query(
        "DELETE FROM graphics_channel_bindings WHERE channel_id=$1 AND role IN ('ticker','lshape')",
    )
    .bind(channel)
    .execute(&mut *connection)
    .await?;

    for id in unused {
        sqlx::query("DELETE FROM graphics_templates WHERE channel_id=$1 AND id=$2")
            .bind(channel)
            .bind(id)
            .execute(&mut *connection)
            .await?;
    }

    sqlx::query("UPDATE graphics_templates SET name='Logo',group_name='' WHERE channel_id=$1 AND id IN (SELECT template_id FROM graphics_channel_bindings WHERE channel_id=$1 AND role='logo')")
        .bind(channel).execute(&mut *connection).await?;

    Ok(())
}

fn image_layers(image: &str, opacity: f64) -> String {
    serde_json::json!([{
        "id": "image", "name": "Image", "kind": "image", "visible": true,
        "x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0,
        "image": image, "opacity": opacity,
        "animation_in": "none", "animation_out": "none"
    }])
    .to_string()
}

/// Upgrade 00029 definitions once, never importing frozen processing columns.
async fn upgrade_channel_layers_on(
    connection: &mut sqlx::SqliteConnection,
    channel: i32,
) -> Result<(), ProcessError> {
    let rows: Vec<(i32, String, String, String)> = sqlx::query_as(
        "SELECT t.id,b.role,t.layers,t.settings FROM graphics_channel_bindings b
         JOIN graphics_templates t ON t.id=b.template_id AND t.channel_id=b.channel_id
         WHERE b.channel_id=$1 AND b.layers_version=0",
    )
    .bind(channel)
    .fetch_all(&mut *connection)
    .await?;

    for (id, role, saved, settings) in rows {
        use crate::utils::graphics::{Animation, LayerKind, TemplateLayer};

        let mut layers: Vec<TemplateLayer> = serde_json::from_str(&saved)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let settings: serde_json::Value = serde_json::from_str(&settings)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let target = layers
            .iter()
            .position(|layer| layer.kind == LayerKind::Image);

        if role == "logo"
            && let Some(index) = target
        {
            let image = layers[index].image.clone().unwrap_or_default();
            let native = native_model_layers(
                connection,
                channel,
                &role,
                &image,
                layers[index].opacity,
                &settings,
            )
            .await?;
            let native: Vec<TemplateLayer> = serde_json::from_str(&native)
                .map_err(|error| ProcessError::Custom(error.to_string()))?;
            let layer = &mut layers[index];
            layer.x = native[0].x;
            layer.y = native[0].y;
            layer.width = native[0].width;
            layer.height = native[0].height;
            layer.animation_in = if settings["transition_in"] == "cut" {
                Animation::None
            } else {
                Animation::Fade
            };
            layer.animation_out = if settings["transition_out"] == "cut" {
                Animation::None
            } else {
                Animation::Fade
            };
            let seconds = settings["transition_seconds"].as_f64();
            layer.animation_ms = (seconds.unwrap_or(0.4) * 1000.0).round() as u32;
            layer.clip_animation_ms = Some((seconds.unwrap_or(1.0) * 1000.0).round() as u32);
        }

        // 00029 already represented the actual (possibly freely edited)
        // L-shape window in a video layer. Preserve that geometry and all art.
        let target_id = target.map(|index| layers[index].id.clone());
        let value = serde_json::to_string(&layers)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        sqlx::query("UPDATE graphics_templates SET layers=$2, settings='{}' WHERE id=$1")
            .bind(id)
            .bind(value)
            .execute(&mut *connection)
            .await?;
        sqlx::query("UPDATE graphics_channel_bindings SET layers_version=1,control_layer_id=$3 WHERE channel_id=$1 AND template_id=$2")
            .bind(channel).bind(id).bind(target_id).execute(&mut *connection).await?;
    }

    Ok(())
}

fn controlled_image<'a>(
    layers: &'a [crate::utils::graphics::TemplateLayer],
    target: Option<&str>,
) -> Option<&'a crate::utils::graphics::TemplateLayer> {
    layers.iter().find(|layer| {
        layer.kind == crate::utils::graphics::LayerKind::Image && Some(layer.id.as_str()) == target
    })
}

fn layer_pixels(value: f64, total: u32) -> u32 {
    ((value / 100.0 * f64::from(total)).round().max(0.0) as u32) & !1
}

/// Legacy DTOs are adapters computed from layers, never a second saved layout.
fn project_layer_processing(
    role: &str,
    enabled: bool,
    layers: &[crate::utils::graphics::TemplateLayer],
    target: Option<&str>,
    config: &mut crate::db::models::Configuration,
    dimensions: (u32, u32),
) {
    use crate::utils::graphics::{Animation, LayerKind};

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
        let layers: Vec<crate::utils::graphics::TemplateLayer> = serde_json::from_str(&layers)
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
        let mut layers: Vec<crate::utils::graphics::TemplateLayer> =
            serde_json::from_str(&saved)
                .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let target_id: Option<String> = sqlx::query_scalar("SELECT control_layer_id FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2")
            .bind(channel).bind(template_id).fetch_one(&mut *connection).await?;
        let native =
            native_model_layers(connection, channel, role, path, opacity, &settings).await?;
        let native: Vec<crate::utils::graphics::TemplateLayer> = serde_json::from_str(&native)
            .map_err(|error| ProcessError::Custom(error.to_string()))?;
        let target = layers
            .iter_mut()
            .find(|layer| Some(layer.id.as_str()) == target_id.as_deref());

        if let Some(target) = target {
            let geometry = &native[0];
            target.image = Some(path.to_string());
            if role == "logo" {
                target.opacity = opacity;
                target.x = geometry.x;
                target.y = geometry.y;
                target.width = geometry.width;
                target.height = geometry.height;
            }
        } else if !path.is_empty() {
            let mut image = native[0].clone();
            image.id = uuid::Uuid::new_v4().to_string();
            layers.push(image);
        }

        if role == "logo"
            && let Some(target) = layers.iter_mut().find(|layer| {
                target_id.as_deref().map_or(
                    layer.kind == crate::utils::graphics::LayerKind::Image,
                    |id| layer.id == id,
                )
            })
        {
            use crate::utils::{config::LogoTransition, graphics::Animation};

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
                .find(|layer| layer.kind == crate::utils::graphics::LayerKind::Video)
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
                .find(|layer| layer.kind == crate::utils::graphics::LayerKind::Image)
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

/// Assign a compatibility role without changing a template's identity or group.
pub async fn assign_graphics_template(
    pool: &SqlitePool,
    channel: i32,
    template: i32,
    role: Option<GraphicsRole>,
    target: Option<&str>,
) -> Result<(), ProcessError> {
    let mut transaction = pool.begin().await?;
    let enabled: bool = sqlx::query_scalar("SELECT COALESCE((SELECT enabled FROM graphics_channel_bindings b WHERE b.channel_id=$1 AND b.template_id=$2 AND b.role IN ('logo','ticker')),permanent,0) FROM graphics_templates WHERE channel_id=$1 AND id=$2")
        .bind(channel).bind(template).fetch_one(&mut *transaction).await?;
    let previous_target: Option<String> = sqlx::query_scalar("SELECT control_layer_id FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2")
        .bind(channel).bind(template).fetch_optional(&mut *transaction).await?.flatten();
    sqlx::query("DELETE FROM graphics_channel_bindings WHERE channel_id=$1 AND template_id=$2")
        .bind(channel)
        .bind(template)
        .execute(&mut *transaction)
        .await?;

    if let Some(role) = role {
        // Replacing a role retains its old composition as an ordinary template,
        // rather than drawing it again as a second permanent logo/ticker.
        sqlx::query("UPDATE graphics_templates SET permanent=0 WHERE channel_id=$1 AND id IN (SELECT template_id FROM graphics_channel_bindings WHERE channel_id=$1 AND role=$2 AND template_id!=$3)")
            .bind(channel).bind(role.as_str()).bind(template).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO graphics_channel_bindings(channel_id,role,template_id,enabled,layers_version,control_layer_id) VALUES($1,$2,$3,$4,1,COALESCE($5,(SELECT json_extract(value,'$.id') FROM graphics_templates t,json_each(t.layers) WHERE t.id=$3 AND json_extract(value,'$.kind')='image' LIMIT 1))) ON CONFLICT(channel_id,role) DO UPDATE SET template_id=excluded.template_id,enabled=excluded.enabled,layers_version=1,control_layer_id=excluded.control_layer_id")
            .bind(channel).bind(role.as_str()).bind(template).bind(enabled)
            .bind(target.or(previous_target.as_deref())).execute(&mut *transaction).await?;
    }

    transaction.commit().await?;

    Ok(())
}

pub async fn select_graphics_fonts(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<crate::db::models::GraphicsFont>, ProcessError> {
    Ok(sqlx::query_as("SELECT id, name, filename, family, size FROM graphics_fonts WHERE channel_id = $1 ORDER BY name, id")
        .bind(channel_id).fetch_all(pool).await?)
}

pub async fn select_graphics_font_data(
    pool: &SqlitePool,
    channel_id: i32,
    id: &str,
) -> Result<Option<Vec<u8>>, ProcessError> {
    Ok(
        sqlx::query_scalar("SELECT data FROM graphics_fonts WHERE channel_id = $1 AND id = $2")
            .bind(channel_id)
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn insert_graphics_font(
    pool: &SqlitePool,
    channel_id: i32,
    font: &crate::db::models::GraphicsFont,
    data: &[u8],
) -> Result<(), ProcessError> {
    sqlx::query("INSERT INTO graphics_fonts (id, channel_id, name, filename, family, size, data) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(&font.id).bind(channel_id).bind(&font.name).bind(&font.filename).bind(&font.family).bind(font.size).bind(data).execute(pool).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;

    async fn install_fixture(pool: &SqlitePool) {
        sqlx::migrate!("../../migrations").run(pool).await.unwrap();
    }

    #[tokio::test]
    async fn first_run_places_the_packaged_logo_before_storage_assets_exist() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        let missing_storage =
            std::env::temp_dir().join(format!("noar-uncreated-{}", uuid::Uuid::new_v4()));
        assert!(!missing_storage.exists());
        sqlx::query("UPDATE channels SET storage=$1 WHERE id=1")
            .bind(missing_storage.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE config_output SET width=640,height=360 WHERE active=1")
            .execute(&pool)
            .await
            .unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        let binding = select_graphics_bindings(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|binding| binding.role == GraphicsRole::Logo)
            .unwrap();
        let mut logo = select_graphics_template(&pool, 1, binding.template_id)
            .await
            .unwrap()
            .unwrap();
        let image = &logo.layers[0];
        assert_eq!(
            (layer_pixels(image.x, 640), layer_pixels(image.y, 360)),
            (586, 12)
        );
        assert_eq!(
            (
                layer_pixels(image.width, 640),
                layer_pixels(image.height, 360)
            ),
            (42, 42)
        );

        // An operator may deliberately make the same stock image full-frame.
        // Subsequent starts must preserve that edit instead of guessing intent.
        logo.layers[0].x = 0.0;
        logo.layers[0].y = 0.0;
        logo.layers[0].width = 100.0;
        logo.layers[0].height = 100.0;
        update_graphics_template(&pool, &logo).await.unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(
            select_graphics_template(&pool, 1, logo.id)
                .await
                .unwrap()
                .unwrap(),
            logo
        );
    }

    async fn fixture_29() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrations = sqlx::migrate!("../../migrations");
        sqlx::migrate::Migrator::with_migrations(
            migrations
                .iter()
                .filter(|migration| migration.version <= 29)
                .cloned()
                .collect::<Vec<_>>(),
        )
        .run(&pool)
        .await
        .unwrap();
        let storage =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests_assets/storage");
        sqlx::query("UPDATE channels SET storage=$1 WHERE id=1")
            .bind(storage.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE config_output SET width=640,height=360 WHERE active=1")
            .execute(&pool)
            .await
            .unwrap();
        let mut connection = pool.acquire().await.unwrap();

        for (role, enabled) in [("logo", true), ("ticker", false), ("lshape", false)] {
            let settings = if role == "logo" {
                serde_json::json!({"scale":"80:48","position":"12:12","transition_in":"fade","transition_out":"cut","transition_seconds":1.0})
            } else {
                serde_json::json!({"scale":70.0,"corner":"top-right"})
            };
            let layers = if role == "ticker" {
                "[]".to_string()
            } else {
                native_model_layers(&mut connection, 1, role, "logo.png", 0.6, &settings)
                    .await
                    .unwrap()
            };
            let id = insert_converted_model(
                &mut connection,
                1,
                role,
                "Channel graphics",
                &layers,
                &settings,
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO graphics_channel_bindings(channel_id,role,template_id,enabled) VALUES(1,$1,$2,$3)")
                .bind(role).bind(id).bind(enabled).execute(&mut *connection).await.unwrap();
        }

        drop(connection);
        pool
    }

    #[tokio::test]
    async fn layer_upgrade_converts_29_in_place_preserves_edits_and_never_reads_settings_again() {
        let pool = fixture_29().await;
        let ids: Vec<(String, i32, bool)> = sqlx::query_as(
            "SELECT role,template_id,enabled FROM graphics_channel_bindings ORDER BY role",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let frozen: String = sqlx::query_scalar("SELECT json_object('logo',logo,'lshape',lshape,'scenes',graphics_scenes) FROM config_processing LIMIT 1").fetch_one(&pool).await.unwrap();
        sqlx::query("UPDATE graphics_templates SET layers=json_set(layers,'$[1].x',10.0,'$[1].y',10.0,'$[1].width',70.0,'$[1].height',50.0) WHERE name='lshape'")
            .execute(&pool).await.unwrap();
        super::super::db_migrate(&pool).await.unwrap();
        let templates = select_graphics_templates(&pool, 1).await.unwrap();
        let logo = templates
            .iter()
            .find(|template| template.name == "logo")
            .unwrap();
        let image = &logo.layers[0];
        assert_eq!(
            (image.x, image.y, image.width, image.height),
            (1.875, 100.0 * 12.0 / 360.0, 12.5, 100.0 * 48.0 / 360.0)
        );
        assert_eq!(image.animation_in, crate::utils::graphics::Animation::Fade);
        assert_eq!(image.animation_out, crate::utils::graphics::Animation::None);
        assert_eq!(
            (image.animation_ms, image.clip_animation_ms),
            (1000, Some(1000))
        );
        assert_eq!(logo.control_layer_id.as_deref(), Some("image"));
        assert_eq!(logo.always_on, Some(true));
        let picture = &templates
            .iter()
            .find(|template| template.name == "lshape")
            .unwrap()
            .layers[1];
        assert_eq!(
            (picture.x, picture.y, picture.width, picture.height),
            (10.0, 10.0, 70.0, 50.0)
        );
        sqlx::query("UPDATE graphics_templates SET settings='{\"position\":\"invalid\",\"scale\":\"invalid\"}' WHERE name='logo'").execute(&pool).await.unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(
            select_graphics_templates(&pool, 1).await.unwrap(),
            templates
        );
        let projected = super::super::select_configuration(&pool, 1).await.unwrap();
        assert_eq!(projected.processing_logo_position, "12:12");
        assert_eq!(projected.processing_logo_scale, "80:48");
        assert_eq!(
            sqlx::query_as::<_, (String, i32, bool)>(
                "SELECT role,template_id,enabled FROM graphics_channel_bindings ORDER BY role"
            )
            .fetch_all(&pool)
            .await
            .unwrap(),
            ids
        );
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT json_object('logo',logo,'lshape',lshape,'scenes',graphics_scenes) FROM config_processing LIMIT 1").fetch_one(&pool).await.unwrap(),frozen);

        // Older template clients omit binding metadata. Reordering layers must
        // not redirect external image replacements to a different image.
        let mut edited = logo.clone();
        let mut second_image = edited.layers[0].clone();
        second_image.id = "secondary-image".to_string();
        edited.layers.insert(0, second_image);
        edited.control_layer_id = None;
        edited.layers[1].animation_ms = 0;
        edited.layers[1].clip_animation_ms = None;
        update_graphics_template(&pool, &edited).await.unwrap();
        let saved = select_graphics_template(&pool, 1, edited.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.control_layer_id.as_deref(), Some("image"));
        let projected = super::super::select_configuration(&pool, 1).await.unwrap();
        assert_eq!(projected.processing_logo_transition_seconds, None);
        assert_eq!(projected.processing_logo_transition_in, "cut");
    }

    #[tokio::test]
    async fn layer_upgrade_rolls_back_every_definition_on_invalid_29_layers() {
        let pool = fixture_29().await;
        sqlx::query("UPDATE graphics_templates SET layers='{}' WHERE name='ticker'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        assert!(convert_channel_graphics(&pool).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM graphics_channel_bindings WHERE layers_version=1"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        sqlx::query("UPDATE graphics_templates SET layers='[]' WHERE name='ticker'")
            .execute(&pool)
            .await
            .unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM graphics_channel_bindings WHERE layers_version=1"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            3
        );
    }

    async fn template(pool: &SqlitePool, name: &str) -> i32 {
        insert_graphics_template(
            pool,
            &GraphicsTemplate {
                channel_id: 1,
                name: name.to_string(),
                ..GraphicsTemplate::default()
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn conversion_keeps_native_even_pixel_rounding_for_fractional_lshape_scales() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        sqlx::query("UPDATE config_output SET width = 1280, height = 720; UPDATE config_processing SET lshape_scale = 73.5, lshape_corner = 'top-right'")
            .execute(&pool).await.unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        let binding = select_graphics_bindings(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|binding| binding.role == GraphicsRole::Lshape)
            .unwrap();
        let template = select_graphics_template(&pool, 1, binding.template_id)
            .await
            .unwrap()
            .unwrap();
        let picture = template
            .layers
            .iter()
            .find(|layer| layer.kind == crate::utils::graphics::LayerKind::Video)
            .unwrap();
        assert_eq!(picture.x * 1280.0 / 100.0, 340.0);
        assert_eq!(picture.width * 1280.0 / 100.0, 940.0);
        assert_eq!(picture.height * 720.0 / 100.0, 528.0);
    }

    #[tokio::test]
    async fn conversion_keeps_existing_names_and_never_reads_frozen_data_again() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        let original = template(&pool, "logo").await;
        convert_channel_graphics(&pool).await.unwrap();
        let binding = select_graphics_bindings(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|binding| binding.role == GraphicsRole::Logo)
            .unwrap();
        assert_ne!(binding.template_id, original);
        assert_eq!(
            select_graphics_template(&pool, 1, binding.template_id)
                .await
                .unwrap()
                .unwrap()
                .name,
            "logo (2)"
        );
        assert_eq!(
            select_graphics_template(&pool, 1, original)
                .await
                .unwrap()
                .unwrap()
                .name,
            "logo"
        );
        let saved = select_graphics_templates(&pool, 1).await.unwrap();
        // Simulate a stale external writer: converted channels must not parse this column.
        sqlx::query("UPDATE config_processing SET graphics_scenes = 'invalid old data'")
            .execute(&pool)
            .await
            .unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(select_graphics_templates(&pool, 1).await.unwrap(), saved);
    }

    #[tokio::test]
    async fn conversion_preserves_frozen_columns_and_edited_models_across_restart() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrations = sqlx::migrate!("../../migrations");
        sqlx::migrate::Migrator::with_migrations(
            migrations
                .iter()
                .filter(|migration| migration.version < 29)
                .cloned()
                .collect(),
        )
        .run(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE config_processing SET add_logo = 1, logo = 'old.png', logo_scale = '64:-1', logo_position = 'W-w-12:12', logo_opacity = 0.7, lshape = 'art.png', lshape_scale = 70, graphics_scenes = '[{\"name\":\"Break\",\"logo\":\"hide\",\"lshape\":\"on\"}]'")
            .execute(&pool).await.unwrap();
        let frozen: String = sqlx::query_scalar("SELECT json_object('logo', logo, 'enabled', add_logo, 'scale', logo_scale, 'opacity', logo_opacity, 'position', logo_position, 'art', lshape, 'scenes', graphics_scenes) FROM config_processing LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        super::super::db_migrate(&pool).await.unwrap();
        let mut projected = super::super::select_configuration(&pool, 1).await.unwrap();
        assert_eq!(projected.processing_logo, "old.png");
        assert_eq!(projected.processing_logo_scale, "1280:720");
        assert_eq!(projected.processing_logo_opacity, 0.7);
        assert_eq!(projected.processing_lshape, "art.png");
        assert_eq!(projected.processing_lshape_scale, 70.0);
        let models = select_graphics_templates(&pool, 1).await.unwrap();
        assert_eq!(models.len(), 4);
        let logo = select_graphics_bindings(&pool, 1)
            .await
            .unwrap()
            .into_iter()
            .find(|binding| binding.role == GraphicsRole::Logo)
            .unwrap();
        sqlx::query("UPDATE graphics_templates SET layers = json_set(layers, '$[0].image', 'edited.png') WHERE id = $1")
            .bind(logo.template_id).execute(&pool).await.unwrap();
        super::super::db_migrate(&pool).await.unwrap();
        assert_eq!(
            select_graphics_templates(&pool, 1).await.unwrap().len(),
            models.len()
        );
        projected = super::super::select_configuration(&pool, 1).await.unwrap();
        assert_eq!(projected.processing_logo, "edited.png");
        // A Composer reorder must not make the compatibility page edit a box.
        let mut edited_layers: serde_json::Value = serde_json::from_str(
            &sqlx::query_scalar::<_, String>("SELECT layers FROM graphics_templates WHERE id = $1")
                .bind(logo.template_id)
                .fetch_one(&pool)
                .await
                .unwrap(),
        )
        .unwrap();
        edited_layers.as_array_mut().unwrap().insert(
            0,
            serde_json::json!({
                "id": "box", "kind": "box", "x": 5, "y": 5, "width": 10, "height": 10,
                "fill": "#ffffff"
            }),
        );
        sqlx::query("UPDATE graphics_templates SET layers = $2 WHERE id = $1")
            .bind(logo.template_id)
            .bind(edited_layers.to_string())
            .execute(&pool)
            .await
            .unwrap();
        // The old page sends the same Processing DTO, but it now writes only models.
        let processing = crate::utils::config::Processing {
            logo: "saved-from-old-page.png".to_string(),
            add_logo: true,
            logo_scale: "80:-1".to_string(),
            logo_position: "12:12".to_string(),
            logo_opacity: 1.0,
            ..Default::default()
        };
        let mut transaction = pool.begin().await.unwrap();
        save_channel_graphics_on(&mut transaction, projected.id, &processing)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(
            super::super::select_configuration(&pool, 1)
                .await
                .unwrap()
                .processing_logo,
            "saved-from-old-page.png"
        );
        let saved = select_graphics_template(&pool, 1, logo.template_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.layers[0].kind, crate::utils::graphics::LayerKind::Box);
        assert!(saved.layers[0].image.is_none());
        assert_eq!(
            saved.layers[1].image.as_deref(),
            Some("saved-from-old-page.png")
        );
        let after: String = sqlx::query_scalar("SELECT json_object('logo', logo, 'enabled', add_logo, 'scale', logo_scale, 'opacity', logo_opacity, 'position', logo_position, 'art', lshape, 'scenes', graphics_scenes) FROM config_processing LIMIT 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(after, frozen);
    }

    #[tokio::test]
    async fn conversion_rolls_back_all_channels_when_one_has_invalid_scenes() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        sqlx::query("INSERT INTO channels(id, name, preview_url) VALUES (2, 'Other channel', 'http://localhost/other.m3u8'); INSERT INTO config(id, channel_id) VALUES (2, 2); INSERT INTO config_processing(config_id, graphics_scenes) VALUES (2, 'invalid')")
            .execute(&pool).await.unwrap();
        assert!(convert_channel_graphics(&pool).await.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graphics_templates")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::query("UPDATE config_processing SET graphics_scenes = '[]' WHERE config_id = 2")
            .execute(&pool)
            .await
            .unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(select_graphics_bindings(&pool, 1).await.unwrap().len(), 3);
        assert_eq!(select_graphics_bindings(&pool, 2).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn permanent_bindings_survive_reopen_without_changing_the_legacy_logo() {
        let path = std::env::temp_dir().join(format!("noar-graphics-{}.db", uuid::Uuid::new_v4()));
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options.clone())
            .await
            .unwrap();
        install_fixture(&pool).await;

        sqlx::query("UPDATE config_processing SET logo = 'legacy-logo.png', add_logo = 1")
            .execute(&pool)
            .await
            .unwrap();
        let logo = template(&pool, "Permanent logo").await;
        let ticker = template(&pool, "Permanent ticker").await;
        let binding = GraphicsChannelBinding {
            channel_id: 1,
            role: GraphicsRole::Logo,
            template_id: logo,
            enabled: true,
        };
        let text = GraphicsChannelBinding {
            role: GraphicsRole::Ticker,
            template_id: ticker,
            enabled: false,
            ..binding.clone()
        };

        assert!(save_graphics_binding(&pool, &binding).await.unwrap());
        assert!(save_graphics_binding(&pool, &binding).await.unwrap());
        assert!(save_graphics_binding(&pool, &text).await.unwrap());
        pool.close().await;

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .unwrap();
        assert_eq!(
            select_graphics_bindings(&pool, 1).await.unwrap(),
            vec![binding.clone(), text]
        );
        let legacy: (String, bool) =
            sqlx::query_as("SELECT logo, add_logo FROM config_processing LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(legacy, ("legacy-logo.png".to_string(), true));
        assert!(
            delete_graphics_binding(&pool, 1, GraphicsRole::Logo)
                .await
                .unwrap()
        );
        assert!(
            !delete_graphics_binding(&pool, 1, GraphicsRole::Logo)
                .await
                .unwrap()
        );
        assert_eq!(select_graphics_bindings(&pool, 1).await.unwrap().len(), 1);
        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn a_binding_cannot_read_or_replace_another_channels_template() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        let id = template(&pool, "Only channel one").await;
        let binding = GraphicsChannelBinding {
            channel_id: 1,
            role: GraphicsRole::Logo,
            template_id: id,
            enabled: true,
        };
        assert!(save_graphics_binding(&pool, &binding).await.unwrap());

        let wrong_channel = GraphicsChannelBinding {
            channel_id: 2,
            ..binding.clone()
        };
        assert!(!save_graphics_binding(&pool, &wrong_channel).await.unwrap());
        assert!(select_graphics_bindings(&pool, 2).await.unwrap().is_empty());
        let missing = GraphicsChannelBinding {
            template_id: id + 1000,
            ..binding.clone()
        };
        assert!(!save_graphics_binding(&pool, &missing).await.unwrap());
        assert_eq!(
            select_graphics_bindings(&pool, 1).await.unwrap(),
            vec![binding]
        );
        assert!(!delete_graphics_template(&pool, 2, id).await.unwrap());
        assert!(delete_graphics_template(&pool, 1, id).await.unwrap());
        assert!(select_graphics_bindings(&pool, 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn migration_installs_bindings_before_conversion() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'graphics_channel_bindings')",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graphics_templates")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
