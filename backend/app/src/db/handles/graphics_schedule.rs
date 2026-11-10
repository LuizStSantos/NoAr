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
            enabled, template
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
            end_time, repeat_days, enabled, template)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
    .bind(
        event
            .template
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ProcessError::Custom(error.to_string()))?,
    )
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
            end_time = $7, repeat_days = $8, enabled = $9, template = $10
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
    .bind(
        event
            .template
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| ProcessError::Custom(error.to_string()))?,
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn template_schedule_migration_preserves_existing_action_bytes_and_is_repeatable() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrations = sqlx::migrate!("../../migrations");
        sqlx::migrate::Migrator::with_migrations(
            migrations
                .iter()
                .filter(|migration| migration.version < 34)
                .cloned()
                .collect::<Vec<_>>(),
        )
        .run(&pool)
        .await
        .unwrap();
        let actions = r#"{"name":"Existing","logo":"hide","lshape":"on"}"#;
        sqlx::query("INSERT INTO graphics_events (channel_id,title,actions,start_date,start_time,repeat_days,enabled) VALUES (1,'Existing',$1,'2026-11-08','12:00:00',0,1)").bind(actions).execute(&pool).await.unwrap();
        migrations.run(&pool).await.unwrap();
        migrations.run(&pool).await.unwrap();
        let saved: String = sqlx::query_scalar("SELECT actions FROM graphics_events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(saved, actions);
        let events = select_graphics_events(&pool, 1).await.unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].template.is_none());
        assert!(
            !serde_json::to_value(&events[0])
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("template")
        );
    }
}
