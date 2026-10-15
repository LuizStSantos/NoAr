use sqlx::sqlite::SqlitePool;

use crate::{
    db::models::{ScheduleEvent, ScheduleRun},
    utils::errors::ProcessError,
};

pub async fn select_schedule_events(
    pool: &SqlitePool,
    channel_id: i32,
) -> Result<Vec<ScheduleEvent>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, kind, title, sources, duration, start_date, start_time, end_time,
            repeat_days, entry_mode, after_mode, show_in_guide, retry_seconds, enabled
         FROM schedule_events WHERE channel_id = $1
         ORDER BY start_date, start_time, id",
    )
    .bind(channel_id)
    .fetch_all(pool)
    .await?)
}

pub async fn select_schedule_event(
    pool: &SqlitePool,
    channel_id: i32,
    event_id: i32,
) -> Result<Option<ScheduleEvent>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, channel_id, kind, title, sources, duration, start_date, start_time, end_time,
            repeat_days, entry_mode, after_mode, show_in_guide, retry_seconds, enabled
         FROM schedule_events WHERE channel_id = $1 AND id = $2",
    )
    .bind(channel_id)
    .bind(event_id)
    .fetch_optional(pool)
    .await?)
}

/// Insert a new event (its `id` is ignored) and return its id.
pub async fn insert_schedule_event(
    pool: &SqlitePool,
    event: &ScheduleEvent,
) -> Result<i32, ProcessError> {
    let id = sqlx::query_scalar(
        "INSERT INTO schedule_events (channel_id, kind, title, sources, duration, start_date,
            start_time, end_time, repeat_days, entry_mode, after_mode, show_in_guide,
            retry_seconds, enabled)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         RETURNING id",
    )
    .bind(event.channel_id)
    .bind(event.kind.as_str())
    .bind(&event.title)
    .bind(sources_json(event)?)
    .bind(event.duration)
    .bind(&event.start_date)
    .bind(&event.start_time)
    .bind(&event.end_time)
    .bind(i64::from(event.repeat_days))
    .bind(event.entry_mode.as_str())
    .bind(event.after_mode.as_str())
    .bind(event.show_in_guide)
    .bind(i64::from(event.retry_seconds))
    .bind(event.enabled)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// Replace an event; returns `false` when it does not exist on the channel.
pub async fn update_schedule_event(
    pool: &SqlitePool,
    event: &ScheduleEvent,
) -> Result<bool, ProcessError> {
    let result = sqlx::query(
        "UPDATE schedule_events SET kind = $3, title = $4, sources = $5, duration = $6,
            start_date = $7, start_time = $8, end_time = $9, repeat_days = $10,
            entry_mode = $11, after_mode = $12, show_in_guide = $13, retry_seconds = $14,
            enabled = $15
         WHERE channel_id = $1 AND id = $2",
    )
    .bind(event.channel_id)
    .bind(event.id)
    .bind(event.kind.as_str())
    .bind(&event.title)
    .bind(sources_json(event)?)
    .bind(event.duration)
    .bind(&event.start_date)
    .bind(&event.start_time)
    .bind(&event.end_time)
    .bind(i64::from(event.repeat_days))
    .bind(event.entry_mode.as_str())
    .bind(event.after_mode.as_str())
    .bind(event.show_in_guide)
    .bind(i64::from(event.retry_seconds))
    .bind(event.enabled)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

/// Delete an event; returns `false` when it does not exist on the channel.
pub async fn delete_schedule_event(
    pool: &SqlitePool,
    channel_id: i32,
    event_id: i32,
) -> Result<bool, ProcessError> {
    let mut transaction = pool.begin().await?;

    sqlx::query("DELETE FROM schedule_runs WHERE event_id = $1")
        .bind(event_id)
        .execute(&mut *transaction)
        .await?;

    let result = sqlx::query("DELETE FROM schedule_events WHERE channel_id = $1 AND id = $2")
        .bind(channel_id)
        .bind(event_id)
        .execute(&mut *transaction)
        .await?;

    transaction.commit().await?;

    Ok(result.rows_affected() == 1)
}

/// Runs of the channel's events for occurrences between two dates (inclusive).
pub async fn select_schedule_runs(
    pool: &SqlitePool,
    channel_id: i32,
    from_date: &str,
    to_date: &str,
) -> Result<Vec<ScheduleRun>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT run.event_id, run.occurrence, run.status, run.started_at, run.finished_at
         FROM schedule_runs run
         JOIN schedule_events event ON event.id = run.event_id
         WHERE event.channel_id = $1 AND run.occurrence BETWEEN $2 AND $3",
    )
    .bind(channel_id)
    .bind(from_date)
    .bind(to_date)
    .fetch_all(pool)
    .await?)
}

/// Forget the runs of an event's occurrences from `from_date` (`YYYY-MM-DD`)
/// on, except one on air, so a moved event plays again at its new time.
/// Returns how many runs were forgotten.
pub async fn reset_schedule_runs_from(
    pool: &SqlitePool,
    event_id: i32,
    from_date: &str,
) -> Result<u64, ProcessError> {
    let result = sqlx::query(
        "DELETE FROM schedule_runs
         WHERE event_id = $1 AND occurrence >= $2 AND status != 'running'",
    )
    .bind(event_id)
    .bind(from_date)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Claim an occurrence before it goes on air. Returns `false` when it was
/// already claimed, so each occurrence plays at most once.
pub async fn claim_schedule_run(
    pool: &SqlitePool,
    event_id: i32,
    occurrence: &str,
    status: &str,
) -> Result<bool, ProcessError> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO schedule_runs (event_id, occurrence, status, started_at)
         VALUES ($1, $2, $3, CURRENT_TIMESTAMP)",
    )
    .bind(event_id)
    .bind(occurrence)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() == 1)
}

pub async fn finish_schedule_run(
    pool: &SqlitePool,
    event_id: i32,
    occurrence: &str,
    status: &str,
) -> Result<(), ProcessError> {
    sqlx::query(
        "UPDATE schedule_runs SET status = $3, finished_at = CURRENT_TIMESTAMP
         WHERE event_id = $1 AND occurrence = $2",
    )
    .bind(event_id)
    .bind(occurrence)
    .bind(status)
    .execute(pool)
    .await?;

    Ok(())
}

fn sources_json(event: &ScheduleEvent) -> Result<String, ProcessError> {
    serde_json::to_string(&event.sources).map_err(|e| ProcessError::Custom(e.to_string()))
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::db::{
        handles::db_migrate,
        models::{AfterMode, EntryMode, ScheduleKind},
    };

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db_migrate(&pool).await.unwrap();

        pool
    }

    fn event() -> ScheduleEvent {
        ScheduleEvent {
            channel_id: 1,
            kind: ScheduleKind::Live,
            title: "Match".to_string(),
            sources: vec!["Lives/Match/signal.live".to_string()],
            start_date: "2026-10-14".to_string(),
            start_time: "16:00:00".to_string(),
            end_time: Some("17:00:00".to_string()),
            repeat_days: 0b0011_1110,
            entry_mode: EntryMode::Interrupt,
            after_mode: AfterMode::Clock,
            show_in_guide: true,
            retry_seconds: 90,
            enabled: true,
            ..ScheduleEvent::default()
        }
    }

    #[tokio::test]
    async fn events_round_trip_and_stay_on_their_channel() {
        let pool = pool().await;
        let mut stored = event();

        stored.id = insert_schedule_event(&pool, &stored).await.unwrap();

        assert_eq!(
            select_schedule_events(&pool, 1).await.unwrap(),
            [stored.clone()]
        );
        assert!(select_schedule_events(&pool, 2).await.unwrap().is_empty());

        stored.title = "Final".to_string();
        stored.sources = vec!["a.mp4".to_string(), "b.mp4".to_string()];
        stored.kind = ScheduleKind::Media;
        stored.end_time = None;

        assert!(update_schedule_event(&pool, &stored).await.unwrap());
        assert_eq!(
            select_schedule_event(&pool, 1, stored.id).await.unwrap(),
            Some(stored.clone())
        );

        let mut other_channel = stored.clone();
        other_channel.channel_id = 2;

        assert!(!update_schedule_event(&pool, &other_channel).await.unwrap());
        assert!(!delete_schedule_event(&pool, 2, stored.id).await.unwrap());
        assert!(delete_schedule_event(&pool, 1, stored.id).await.unwrap());
        assert!(select_schedule_events(&pool, 1).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_occurrence_can_be_claimed_once() {
        let pool = pool().await;
        let id = insert_schedule_event(&pool, &event()).await.unwrap();

        assert!(
            claim_schedule_run(&pool, id, "2026-10-14", "running")
                .await
                .unwrap()
        );
        assert!(
            !claim_schedule_run(&pool, id, "2026-10-14", "running")
                .await
                .unwrap()
        );

        finish_schedule_run(&pool, id, "2026-10-14", "done")
            .await
            .unwrap();

        let runs = select_schedule_runs(&pool, 1, "2026-10-01", "2026-10-31")
            .await
            .unwrap();

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "done");
        assert!(runs[0].finished_at.is_some());
    }

    #[tokio::test]
    async fn resetting_runs_keeps_past_days_and_the_one_on_air() {
        let pool = pool().await;
        let id = insert_schedule_event(&pool, &event()).await.unwrap();

        for (date, status) in [
            ("2026-10-13", "done"),
            ("2026-10-14", "failed"),
            ("2026-10-15", "running"),
            ("2026-10-16", "done"),
        ] {
            assert!(claim_schedule_run(&pool, id, date, status).await.unwrap());
        }

        assert_eq!(
            reset_schedule_runs_from(&pool, id, "2026-10-14")
                .await
                .unwrap(),
            2
        );

        let runs = select_schedule_runs(&pool, 1, "2026-10-01", "2026-10-31")
            .await
            .unwrap();
        let mut left: Vec<_> = runs.iter().map(|run| run.occurrence.as_str()).collect();
        left.sort_unstable();

        assert_eq!(left, ["2026-10-13", "2026-10-15"]);
        assert!(
            claim_schedule_run(&pool, id, "2026-10-14", "running")
                .await
                .unwrap()
        );
    }
}
