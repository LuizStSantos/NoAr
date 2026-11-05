use sqlx::sqlite::SqlitePool;

use crate::{db::models::GraphicsTemplate, utils::errors::ProcessError};

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
