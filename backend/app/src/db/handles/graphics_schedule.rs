use sqlx::sqlite::SqlitePool;

use crate::{db::models::GraphicsEvent, utils::errors::ProcessError};

fn actions_json(event: &GraphicsEvent) -> Result<String, ProcessError> {
    serde_json::to_string(&event.actions).map_err(|error| ProcessError::Custom(error.to_string()))
}

pub async fn select_graphics_events(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<GraphicsEvent>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, title, actions, start_date, start_time, end_time, repeat_days,
            enabled
         FROM graphics_events WHERE channel_id = $1
         ORDER BY start_date, start_time, id",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?)
}

pub async fn insert_graphics_event(
    pool: &SqlitePool,
    event: &GraphicsEvent,
) -> Result<i32, ProcessError> {
    Ok(sqlx::query_scalar(
        "INSERT INTO graphics_events (channel_id, title, actions, start_date, start_time,
            end_time, repeat_days, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id",
    )
    .bind(event.channel_id)
    .bind(&event.title)
    .bind(actions_json(event)?)
    .bind(&event.start_date)
    .bind(&event.start_time)
    .bind(&event.end_time)
    .bind(i64::from(event.repeat_days))
    .bind(event.enabled)
    .fetch_one(pool)
    .await?)
}

/// `false` when the channel has no such event.
pub async fn update_graphics_event(
    pool: &SqlitePool,
    event: &GraphicsEvent,
) -> Result<bool, ProcessError> {
    let result = sqlx::query(
        "UPDATE graphics_events SET title = $3, actions = $4, start_date = $5, start_time = $6,
            end_time = $7, repeat_days = $8, enabled = $9
         WHERE channel_id = $1 AND id = $2",
    )
    .bind(event.channel_id)
    .bind(event.id)
    .bind(&event.title)
    .bind(actions_json(event)?)
    .bind(&event.start_date)
    .bind(&event.start_time)
    .bind(&event.end_time)
    .bind(i64::from(event.repeat_days))
    .bind(event.enabled)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// `false` when the channel has no such event.
pub async fn delete_graphics_event(
    pool: &SqlitePool,
    channel_id: i32,
    event_id: i32,
) -> Result<bool, ProcessError> {
    let result = sqlx::query("DELETE FROM graphics_events WHERE channel_id = $1 AND id = $2")
        .bind(channel_id)
        .bind(event_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}
