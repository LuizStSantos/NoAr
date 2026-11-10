//! Transactional startup import of legacy graphics and 00029 layer upgrades.
//!
//! Conversion markers make restarts idempotent; existing model IDs and edited
//! layers are retained. Schema installation remains the normal sqlx migrator's
//! responsibility, and the migration tests below use that real runner.

use sqlx::sqlite::SqlitePool;

use crate::utils::errors::ProcessError;

use super::{insert_converted_model, native_model_layers};

/// Convert all channels atomically. A durable marker tracks the completed import,
/// independently of later binding deletion or reassignment.
pub async fn convert_channel_graphics(pool: &SqlitePool) -> Result<(), ProcessError> {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
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

    let existing_bindings: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM graphics_channel_bindings WHERE channel_id=$1
         UNION ALL SELECT 1 FROM graphics_scene_bindings WHERE channel_id=$1)",
    )
    .bind(channel)
    .fetch_one(&mut *connection)
    .await?;

    if existing_bindings {
        // Migration 31 could omit the marker after a pre-31 binding deletion.
        // Any surviving role or scene proves prior import; deleted roles stay deleted.
        upgrade_channel_layers_on(connection, channel).await?;
        sqlx::query("INSERT INTO graphics_channel_state(channel_id) VALUES($1)")
            .bind(channel)
            .execute(&mut *connection)
            .await?;

        return Ok(());
    }

    // With neither a marker nor any binding, a virgin channel and a pre-31
    // channel stripped of every binding are indistinguishable. Unrelated custom
    // templates do not establish prior import, so retain the normal import path.
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
        use crate::graphics::{Animation, LayerKind, TemplateLayer};

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

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use crate::db::{
        handles::graphics::{
            insert_graphics_template, select_graphics_bindings, select_graphics_template,
            select_graphics_templates, update_graphics_template,
        },
        models::{GraphicsRole, GraphicsTemplate},
    };

    use super::super::{layer_pixels, save_channel_graphics_on};
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
        crate::db::handles::db_migrate(&pool).await.unwrap();
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
        assert_eq!(image.animation_in, crate::graphics::Animation::Fade);
        assert_eq!(image.animation_out, crate::graphics::Animation::None);
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
        let projected = crate::db::handles::select_configuration(&pool, 1)
            .await
            .unwrap();
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
        let projected = crate::db::handles::select_configuration(&pool, 1)
            .await
            .unwrap();
        assert_eq!(projected.processing_logo_transition_seconds, None);
        assert_eq!(projected.processing_logo_transition_in, "cut");
    }

    #[tokio::test]
    async fn partial_pre31_import_adopts_surviving_roles_and_scenes_without_frozen_reads() {
        for version in [29, 30] {
            for retained in 0..=2 {
                let pool = fixture_29().await;

                if version == 30 {
                    let migrations = sqlx::migrate!("../../migrations");
                    sqlx::migrate::Migrator::with_migrations(
                        migrations
                            .iter()
                            .filter(|migration| migration.version <= 30)
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                    .run(&pool)
                    .await
                    .unwrap();
                }

                // Even with no role bindings, a surviving scene proves prior import.
                let scene: i32 = sqlx::query_scalar(r#"INSERT INTO graphics_templates(channel_id,name,group_name,layers,settings) VALUES(1,'Kept scene','Scenes','[]','{"scene":{"name":"Kept scene"}}') RETURNING id"#)
                    .fetch_one(&pool).await.unwrap();
                sqlx::query("INSERT INTO graphics_scene_bindings(channel_id,template_id,position) VALUES(1,$1,0)")
                    .bind(scene).execute(&pool).await.unwrap();
                sqlx::query("DELETE FROM graphics_channel_bindings WHERE role IN (SELECT role FROM graphics_channel_bindings ORDER BY role DESC LIMIT $1)")
                    .bind(3 - retained).execute(&pool).await.unwrap();
                sqlx::query("DELETE FROM graphics_templates WHERE id NOT IN (SELECT template_id FROM graphics_channel_bindings UNION ALL SELECT template_id FROM graphics_scene_bindings)")
                    .execute(&pool).await.unwrap();
                sqlx::query("UPDATE graphics_templates SET layers=json_set(layers,'$[1].x',10.1,'$[1].height',50.3) WHERE name='lshape'; UPDATE config_processing SET logo='frozen-deleted.png',graphics_scenes='invalid frozen scenes'")
                    .execute(&pool).await.unwrap();
                let bindings: Vec<(String, i32)> = sqlx::query_as(
                    "SELECT role,template_id FROM graphics_channel_bindings ORDER BY role",
                )
                .fetch_all(&pool)
                .await
                .unwrap();
                let ids: Vec<i32> =
                    sqlx::query_scalar("SELECT id FROM graphics_templates ORDER BY id")
                        .fetch_all(&pool)
                        .await
                        .unwrap();
                crate::db::handles::db_migrate(&pool).await.unwrap();
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM graphics_channel_state WHERE channel_id=1"
                    )
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                    1
                );
                assert_eq!(
                    sqlx::query_as::<_, (String, i32)>(
                        "SELECT role,template_id FROM graphics_channel_bindings ORDER BY role"
                    )
                    .fetch_all(&pool)
                    .await
                    .unwrap(),
                    bindings
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i64>(
                        "SELECT count(*) FROM graphics_channel_bindings WHERE layers_version=1"
                    )
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                    i64::from(retained)
                );
                let lshape: Option<String> =
                    sqlx::query_scalar("SELECT layers FROM graphics_templates WHERE name='lshape'")
                        .fetch_optional(&pool)
                        .await
                        .unwrap();

                if let Some(layers) = lshape {
                    let layers: serde_json::Value = serde_json::from_str(&layers).unwrap();
                    assert_eq!(layers[1]["x"], 10.1);
                    assert_eq!(layers[1]["height"], 50.3);
                }

                // After adoption, authored edits must survive repeated startup conversion.
                sqlx::query("UPDATE graphics_templates SET layers=json_set(layers,'$[0].x',5.1) WHERE name='logo'")
                    .execute(&pool).await.unwrap();
                let saved = select_graphics_templates(&pool, 1).await.unwrap();
                convert_channel_graphics(&pool).await.unwrap();
                convert_channel_graphics(&pool).await.unwrap();
                assert_eq!(select_graphics_templates(&pool, 1).await.unwrap(), saved);
                assert_eq!(
                    sqlx::query_scalar::<_, i32>("SELECT id FROM graphics_templates ORDER BY id")
                        .fetch_all(&pool)
                        .await
                        .unwrap(),
                    ids
                );
                assert_eq!(
                    sqlx::query_scalar::<_, i32>(
                        "SELECT template_id FROM graphics_scene_bindings WHERE channel_id=1"
                    )
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                    scene
                );
            }
        }
    }

    #[tokio::test]
    async fn partial_pre31_roles_alone_prove_import() {
        let pool = fixture_29().await;
        sqlx::query("DELETE FROM graphics_channel_bindings WHERE role!='logo'; UPDATE config_processing SET graphics_scenes='invalid frozen scenes'")
            .execute(&pool).await.unwrap();
        let id: i32 = sqlx::query_scalar("SELECT template_id FROM graphics_channel_bindings")
            .fetch_one(&pool)
            .await
            .unwrap();
        crate::db::handles::db_migrate(&pool).await.unwrap();
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(
            sqlx::query_as::<_, (i32, i32)>(
                "SELECT template_id,layers_version FROM graphics_channel_bindings"
            )
            .fetch_all(&pool)
            .await
            .unwrap(),
            vec![(id, 1)]
        );
    }

    #[tokio::test]
    async fn unrelated_custom_template_does_not_suppress_virgin_import() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        install_fixture(&pool).await;
        let id = template(&pool, "Custom model").await;
        convert_channel_graphics(&pool).await.unwrap();
        assert_eq!(select_graphics_bindings(&pool, 1).await.unwrap().len(), 3);
        assert!(
            select_graphics_template(&pool, 1, id)
                .await
                .unwrap()
                .is_some()
        );
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
            .find(|layer| layer.kind == crate::graphics::LayerKind::Video)
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
        crate::db::handles::db_migrate(&pool).await.unwrap();
        let mut projected = crate::db::handles::select_configuration(&pool, 1)
            .await
            .unwrap();
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
        crate::db::handles::db_migrate(&pool).await.unwrap();
        assert_eq!(
            select_graphics_templates(&pool, 1).await.unwrap().len(),
            models.len()
        );
        projected = crate::db::handles::select_configuration(&pool, 1)
            .await
            .unwrap();
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
            crate::db::handles::select_configuration(&pool, 1)
                .await
                .unwrap()
                .processing_logo,
            "saved-from-old-page.png"
        );
        let saved = select_graphics_template(&pool, 1, logo.template_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.layers[0].kind, crate::graphics::LayerKind::Box);
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
