use axum::{
    Json,
    extract::{Query, State},
};
use protect_axum::authorities::AuthDetails;
use serde::Serialize;

use crate::{
    api::{routes::ensure_any_authority, state::AppState},
    db::{
        handles::{self, Activity, ActivityFilter},
        models::Role,
    },
    utils::errors::ServiceError,
};

/// One page of the history, newest first.
#[derive(Debug, Serialize)]
pub struct ActivityPage {
    pub entries: Vec<Activity>,
    /// Older entries follow: ask again with `before` = the last `id`.
    pub more: bool,
}

/// ### Activity
///
/// What users did: changes, sign-ins and external commands with a key, newest
/// first. Global admins only. Filters, all optional: `from` and `to` (Unix
/// seconds), `user` (name), `channel` (ID), `action` (`playlist.save`, or the
/// group `playlist`), `q` (text), `before` (an entry ID, for the next page)
/// and `limit` (1–1000, default 200).
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/api/activity?user=maria&action=playlist' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_activity(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
    Query(filter): Query<ActivityFilter>,
) -> Result<Json<ActivityPage>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    let limit = filter.limit.unwrap_or(200).clamp(1, 1000);
    let mut entries = handles::select_activity(&state.pool, &filter, limit + 1).await?;
    let more = entries.len() as i64 > limit;
    entries.truncate(limit as usize);

    Ok(Json(ActivityPage { entries, more }))
}
