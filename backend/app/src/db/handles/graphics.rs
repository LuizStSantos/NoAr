use sqlx::sqlite::SqlitePool;

use crate::{
    db::models::{GraphicsChannelBinding, GraphicsRole, GraphicsTemplate},
    utils::errors::ProcessError,
};

fn layers_json(template: &GraphicsTemplate) -> Result<String, ProcessError> {
    serde_json::to_string(&template.layers).map_err(|error| ProcessError::Custom(error.to_string()))
}

pub async fn select_graphics_templates(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<GraphicsTemplate>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, name, group_name, duration, during_lshape, layers
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
        "SELECT id, channel_id, name, group_name, duration, during_lshape, layers
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
            layers)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id",
    )
    .bind(template.channel_id)
    .bind(&template.name)
    .bind(&template.group)
    .bind(template.duration)
    .bind(template.during_lshape.as_str())
    .bind(layers_json(template)?)
    .fetch_one(pool)
    .await?)
}

/// `false` when the channel has no such template.
pub async fn update_graphics_template(
    pool: &SqlitePool,
    template: &GraphicsTemplate,
) -> Result<bool, ProcessError> {
    let result = sqlx::query(
        "UPDATE graphics_templates SET name = $3, group_name = $4, duration = $5,
            during_lshape = $6, layers = $7, updated_at = CURRENT_TIMESTAMP
         WHERE channel_id = $1 AND id = $2",
    )
    .bind(template.channel_id)
    .bind(template.id)
    .bind(&template.name)
    .bind(&template.group)
    .bind(template.duration)
    .bind(template.during_lshape.as_str())
    .bind(layers_json(template)?)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// `false` when the channel has no such template.
pub async fn delete_graphics_template(
    pool: &SqlitePool,
    channel_id: i32,
    template_id: i32,
) -> Result<bool, ProcessError> {
    let result = sqlx::query("DELETE FROM graphics_templates WHERE channel_id = $1 AND id = $2")
        .bind(channel_id)
        .bind(template_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
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
    .execute(pool)
    .await?;

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

#[cfg(test)]
mod tests {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;

    // Stage a deliberately installs the proposed schema only in a fixture.
    // Production startup continues to run the unchanged migrations through 00028.
    async fn install_fixture(pool: &SqlitePool) {
        sqlx::migrate!("../../migrations").run(pool).await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE graphics_channel_bindings (
                channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
                role TEXT NOT NULL CHECK(role IN ('logo', 'ticker', 'lshape')),
                template_id INTEGER NOT NULL REFERENCES graphics_templates(id) ON DELETE RESTRICT,
                enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
                PRIMARY KEY(channel_id, role)
             );",
        )
        .execute(pool)
        .await
        .unwrap();
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
        assert!(
            delete_graphics_template(&pool, 1, id).await.is_err(),
            "a bound template cannot be removed silently"
        );
    }

    #[tokio::test]
    async fn ordinary_startup_has_no_bindings_table_or_data_conversion_in_stage_a() {
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
        assert!(!exists);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graphics_templates")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
