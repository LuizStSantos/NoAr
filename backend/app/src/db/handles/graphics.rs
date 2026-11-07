//! Graphics persistence facade: templates and channel/scene bindings.
//!
//! `compatibility` translates legacy DTOs to/from the same layer models and
//! owns startup conversion; `resources` stores fonts and feed revisions. Public
//! handle names stay stable so API, config loading and scheduling share one
//! repository boundary without leaking SQL into those callers.

use sqlx::sqlite::SqlitePool;

#[path = "graphics/compatibility/graphics.rs"]
mod compatibility;
#[path = "graphics/resources/graphics.rs"]
mod resources;

use crate::{
    db::models::{GraphicsChannelBinding, GraphicsRole, GraphicsTemplate},
    utils::errors::ProcessError,
};

pub use compatibility::{
    convert_channel_graphics, convert_channel_graphics_on, initialize_channel_graphics_on,
    project_channel_graphics, save_channel_graphics_on,
};
pub use resources::{
    begin_graphics_source_refresh, delete_graphics_source, finish_graphics_source_refresh,
    insert_graphics_font, save_graphics_source, select_graphics_font_data, select_graphics_fonts,
    select_graphics_source, select_graphics_sources,
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
                template
                    .layers
                    .iter()
                    .find(|layer| layer.id == id && layer.kind == crate::graphics::LayerKind::Image)
            })
            .or_else(|| {
                template
                    .layers
                    .iter()
                    .find(|layer| layer.kind == crate::graphics::LayerKind::Image)
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

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;

    async fn install_fixture(pool: &SqlitePool) {
        sqlx::migrate!("../../migrations").run(pool).await.unwrap();
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
}
