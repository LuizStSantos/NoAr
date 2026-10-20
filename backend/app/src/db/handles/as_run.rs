use sqlx::{Row, sqlite::SqlitePool};

use crate::{
    db::models::{AsRunEntry, NewAsRunEntry},
    utils::errors::ProcessError,
};

/// Why rows still open are closed when a new item starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CutBy<'a> {
    pub reason: Option<&'a str>,
    pub detail: Option<&'a str>,
}

/// Open a row for an item going on air at `started_at` (Unix milliseconds)
/// and return its id. Only one item airs at a time, so a row still open ends
/// at the same moment: cut for the reason in `cut_by`, or completed for an
/// ingest, which nothing cuts (it goes off air when its input ends, and the
/// programme can start again before the ingest watcher notices).
pub async fn insert_as_run(
    pool: &SqlitePool,
    channel_id: i32,
    entry: &NewAsRunEntry,
    started_at: i64,
    cut_by: CutBy<'_>,
) -> Result<i64, ProcessError> {
    let mut transaction = pool.begin().await?;

    sqlx::query(
        "UPDATE as_run
         SET ended_at = $2,
             status = CASE WHEN kind = 'ingest' THEN 'completed' ELSE 'cut' END,
             reason = CASE WHEN kind = 'ingest' THEN NULL ELSE $3 END,
             detail = CASE WHEN kind = 'ingest' THEN NULL ELSE $4 END
         WHERE channel_id = $1 AND ended_at IS NULL",
    )
    .bind(channel_id)
    .bind(started_at)
    .bind(cut_by.reason)
    .bind(cut_by.detail)
    .execute(&mut *transaction)
    .await?;

    let row = sqlx::query(
        "INSERT INTO as_run (channel_id, started_at, source, title, kind, origin, event_id,
            event_title, scheduled_at, seek, planned_seconds, replaces)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING id",
    )
    .bind(channel_id)
    .bind(started_at)
    .bind(&entry.source)
    .bind(&entry.title)
    .bind(entry.kind)
    .bind(entry.origin)
    .bind(entry.event_id)
    .bind(&entry.event_title)
    .bind(entry.scheduled_at)
    .bind(entry.seek)
    .bind(entry.planned_seconds)
    .bind(&entry.replaces)
    .fetch_one(&mut *transaction)
    .await?;

    transaction.commit().await?;

    Ok(row.get("id"))
}

/// Close the row `id` at `ended_at`, unless it was closed already (an ingest
/// that took over closes the clip it cut).
pub async fn finish_as_run(
    pool: &SqlitePool,
    id: i64,
    ended_at: i64,
    status: &str,
    reason: Option<&str>,
    detail: Option<&str>,
) -> Result<(), ProcessError> {
    sqlx::query(
        "UPDATE as_run SET ended_at = $2, status = $3, reason = $4, detail = $5
         WHERE id = $1 AND ended_at IS NULL",
    )
    .bind(id)
    .bind(ended_at)
    .bind(status)
    .bind(reason)
    .bind(detail)
    .execute(pool)
    .await?;

    Ok(())
}

/// Close the rows a playout that did not stop cleanly left open. When it
/// stopped is unknown: the row ends where the item would have ended, or at
/// `now` if that is earlier, marked as interrupted.
pub async fn close_open_as_run(
    pool: &SqlitePool,
    channel_id: i32,
    now: i64,
) -> Result<u64, ProcessError> {
    let result = sqlx::query(
        "UPDATE as_run
         SET ended_at = MIN(started_at + CAST(planned_seconds * 1000 AS INTEGER), $2),
             status = 'stopped', reason = 'interrupted'
         WHERE channel_id = $1 AND ended_at IS NULL",
    )
    .bind(channel_id)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

/// Items on air at some point of `[from, to)`, Unix milliseconds, in order.
pub async fn select_as_run(
    pool: &SqlitePool,
    channel_id: i32,
    from: i64,
    to: i64,
) -> Result<Vec<AsRunEntry>, ProcessError> {
    Ok(sqlx::query_as(
        "SELECT id, started_at, ended_at, source, title, kind, origin, event_id, event_title,
            scheduled_at, seek, planned_seconds, replaces, status, reason, detail
         FROM as_run
         WHERE channel_id = $1 AND started_at < $3 AND (ended_at IS NULL OR ended_at > $2)
         ORDER BY started_at, id",
    )
    .bind(channel_id)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?)
}
