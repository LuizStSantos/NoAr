//! What users did: the history behind Settings › Activity
//! (`migrations/00036_activity_log.sql`).

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

use crate::utils::errors::ProcessError;

/// Days the history is kept; older entries are removed.
pub const ACTIVITY_DAYS: i64 = 180;

/// One thing a user did, or tried to do.
#[derive(Clone, Debug, Default, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct Activity {
    #[serde(default)]
    pub id: i64,
    /// Unix time in seconds.
    pub at: i64,
    /// `None` for a sign-in attempt or an external command with a key.
    pub user_id: Option<i32>,
    pub username: String,
    pub role: String,
    pub channel_id: Option<i32>,
    /// A stable name such as `playlist.save`.
    pub action: String,
    /// What it was done to: a date, a name, a file.
    pub target: String,
    pub method: String,
    pub route: String,
    /// The HTTP status of the answer.
    pub status: i64,
    pub ip: String,
}

/// Which entries to list, newest first.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityFilter {
    /// From this Unix time, in seconds.
    pub from: Option<i64>,
    /// Up to this Unix time, in seconds.
    pub to: Option<i64>,
    /// The user's name, as recorded.
    pub user: Option<String>,
    pub channel: Option<i32>,
    /// An action (`playlist.save`) or a group of them (`playlist`).
    pub action: Option<String>,
    /// Text in the user, action, target or address.
    pub q: Option<String>,
    /// Entries before this one, for the next page.
    pub before: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn insert_activity(pool: &SqlitePool, entry: &Activity) -> Result<(), ProcessError> {
    sqlx::query(
        "INSERT INTO activity_log
         (at, user_id, username, role, channel_id, action, target, method, route, status, ip)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(entry.at)
    .bind(entry.user_id)
    .bind(&entry.username)
    .bind(&entry.role)
    .bind(entry.channel_id)
    .bind(&entry.action)
    .bind(&entry.target)
    .bind(&entry.method)
    .bind(&entry.route)
    .bind(entry.status)
    .bind(&entry.ip)
    .execute(pool)
    .await?;

    Ok(())
}

/// Remove the entries from before `before` (Unix seconds).
pub async fn prune_activity(pool: &SqlitePool, before: i64) -> Result<u64, ProcessError> {
    let result = sqlx::query("DELETE FROM activity_log WHERE at < $1")
        .bind(before)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

/// Up to `limit` entries that match `filter`, newest first.
pub async fn select_activity(
    pool: &SqlitePool,
    filter: &ActivityFilter,
    limit: i64,
) -> Result<Vec<Activity>, ProcessError> {
    let mut query: QueryBuilder<Sqlite> = QueryBuilder::new(
        "SELECT id, at, user_id, username, role, channel_id, action, target, method, route, status, ip
         FROM activity_log WHERE 1 = 1",
    );

    if let Some(from) = filter.from {
        query.push(" AND at >= ").push_bind(from);
    }
    if let Some(to) = filter.to {
        query.push(" AND at <= ").push_bind(to);
    }
    if let Some(user) = filter.user.as_deref().filter(|user| !user.is_empty()) {
        query.push(" AND username = ").push_bind(user.to_string());
    }
    if let Some(channel) = filter.channel {
        query.push(" AND channel_id = ").push_bind(channel);
    }
    if let Some(action) = filter.action.as_deref().filter(|action| !action.is_empty()) {
        query
            .push(" AND (action = ")
            .push_bind(action.to_string())
            .push(" OR substr(action, 1, ")
            .push_bind(action.len() as i64 + 1)
            .push(") = ")
            .push_bind(format!("{action}."))
            .push(")");
    }
    if let Some(text) = filter
        .q
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        query
            .push(" AND instr(lower(username || ' ' || action || ' ' || target || ' ' || ip), ")
            .push_bind(text.to_lowercase())
            .push(") > 0");
    }
    if let Some(before) = filter.before {
        query.push(" AND id < ").push_bind(before);
    }

    query.push(" ORDER BY id DESC LIMIT ").push_bind(limit);

    Ok(query.build_query_as().fetch_all(pool).await?)
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::db::handles;

    fn entry(at: i64, username: &str, action: &str, channel: Option<i32>) -> Activity {
        Activity {
            at,
            username: username.to_string(),
            action: action.to_string(),
            channel_id: channel,
            method: "POST".to_string(),
            route: "/api/test".to_string(),
            status: 200,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn the_history_is_filtered_paged_and_pruned() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        handles::db_migrate(&pool).await.unwrap();
        for item in [
            entry(100, "ana", "playlist.save", Some(1)),
            entry(200, "bruno", "playlist.delete", Some(2)),
            entry(300, "ana", "schedule.create", Some(1)),
            entry(400, "ana", "playlists.other", Some(1)),
        ] {
            insert_activity(&pool, &item).await.unwrap();
        }
        let list = |filter: ActivityFilter| {
            let pool = pool.clone();
            async move {
                select_activity(&pool, &filter, 10)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|item| item.at)
                    .collect::<Vec<_>>()
            }
        };

        assert_eq!(list(ActivityFilter::default()).await, [400, 300, 200, 100]);
        // A group is the actions it starts, not any name that begins alike.
        let playlist = ActivityFilter {
            action: Some("playlist".into()),
            ..Default::default()
        };
        assert_eq!(list(playlist).await, [200, 100]);
        let ana_on_one = ActivityFilter {
            user: Some("ana".into()),
            channel: Some(1),
            from: Some(150),
            ..Default::default()
        };
        assert_eq!(list(ana_on_one).await, [400, 300]);
        let text = ActivityFilter {
            q: Some("SCHEDULE".into()),
            ..Default::default()
        };
        assert_eq!(list(text).await, [300]);
        let first = select_activity(&pool, &ActivityFilter::default(), 2)
            .await
            .unwrap();
        let next = ActivityFilter {
            before: first.last().map(|item| item.id),
            ..Default::default()
        };
        assert_eq!(list(next).await, [200, 100]);

        assert_eq!(prune_activity(&pool, 250).await.unwrap(), 2);
        assert_eq!(list(ActivityFilter::default()).await, [400, 300]);
    }
}
