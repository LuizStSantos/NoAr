use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use protect_axum::authorities::AuthDetails;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{handles, models::Role},
    utils::{
        errors::ServiceError,
        report::{PlayoutState, Report, ReportQuery, build_report},
    },
};

/// **Report of playlist days**
///
/// What aired and what is still to come, item by item, for the playlist days
/// `from` to `to` (both included, at most 62).
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/api/report/1?from=2026-10-01&to=2026-10-31' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_report(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(query): Query<ReportQuery>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Report>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let (from, to) = query.dates()?;
    let manager = state
        .controller
        .read()
        .await
        .get(id)
        .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let config = manager.config.read().await.clone();
    let events = handles::select_schedule_events(&state.pool, id).await?;
    let playout = PlayoutState {
        running: manager.is_alive.load(Ordering::SeqCst),
        date: manager.current_date.lock().await.clone(),
        time_shift: manager.channel.lock().await.time_shift,
    };

    Ok(Json(
        build_report(&config, &state.pool, &events, &playout, from, to).await?,
    ))
}
