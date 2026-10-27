use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, Query, State},
};
use chrono::{NaiveDateTime, TimeDelta};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{
        handles,
        models::{Role, ScheduleEvent},
    },
    player::{
        controller::ChannelManager,
        schedule::{ActiveEvent, return_to_programme},
    },
    utils::{
        errors::ServiceError,
        schedule::{
            OccurrenceView, ScheduleEventInput,
            live::{
                LiveSource, NewLive, SignalTest, list_lives, save_live, test_signal,
                update_live_address,
            },
            occurrence_now, occurrence_views, parse_date, prepare_event, timing_changed,
        },
        time_machine::time_now,
        xmltv::{GuideBlock, guide_blocks, library::program_title},
    },
};

#[derive(Debug, Deserialize)]
pub struct ScheduleQuery {
    /// `YYYY-MM-DD`; today in the channel time zone when missing.
    date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDay {
    date: String,
    occurrences: Vec<OccurrenceView>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleStatus {
    /// The event on air.
    active: Option<ActiveEvent>,
    /// Events that are due and wait for the clip on air to end.
    queued: Vec<OccurrenceView>,
    /// Occurrences in the next 24 hours that have not aired yet.
    next: Vec<OccurrenceView>,
}

#[derive(Debug, Deserialize)]
pub struct SignalRequest {
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct LiveAddress {
    /// Path of the `.live` file, relative to the storage.
    path: String,
    url: String,
}

async fn authorized_manager(
    state: &AppState,
    id: i32,
    user: &AuthUser,
    details: &AuthDetails<Role>,
) -> Result<ChannelManager, ServiceError> {
    ensure_any_authority(
        details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let guard = state.controller.read().await;

    guard
        .get(id)
        .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))
}

/// **Scheduled events of a day**
///
/// Every occurrence that overlaps the day, with its run state.
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/api/schedule/1?date=2026-10-14' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_schedule_day(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(query): Query<ScheduleQuery>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<ScheduleDay>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let timezone = manager.config.read().await.channel.timezone;
    let date = match query.date.as_deref() {
        Some(date) => parse_date(date).map_err(ServiceError::BadRequest)?,
        None => time_now(&timezone).date_naive(),
    };
    let from = date.and_time(Default::default());
    let to = from + TimeDelta::try_days(1).unwrap_or_default();
    let events = handles::select_schedule_events(&state.pool, id).await?;
    let day = |offset: i64| {
        (date + TimeDelta::try_days(offset).unwrap_or_default())
            .format("%Y-%m-%d")
            .to_string()
    };
    let runs = handles::select_schedule_runs(&state.pool, id, &day(-1), &day(1)).await?;

    Ok(Json(ScheduleDay {
        date: day(0),
        occurrences: occurrence_views(&events, &runs, from, to),
    }))
}

/// **The day's programme**
///
/// Guide entries of the day, with program names from the `.nfo` files and
/// times moved by the scheduled events; the scheduler's timeline shows them.
pub async fn get_schedule_grade(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Query(query): Query<ScheduleQuery>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<GuideBlock>>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let config = manager.config.read().await.clone();
    let date = match query.date.as_deref() {
        Some(date) => parse_date(date).map_err(ServiceError::BadRequest)?,
        None => time_now(&config.channel.timezone).date_naive(),
    };
    let events = handles::select_schedule_events(&state.pool, id).await?;

    Ok(Json(guide_blocks(&config, &events, date).await?))
}

/// **All scheduled events of a channel**
pub async fn get_schedule_events(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<ScheduleEvent>>, ServiceError> {
    authorized_manager(&state, id, &user, &details).await?;

    Ok(Json(
        handles::select_schedule_events(&state.pool, id).await?,
    ))
}

/// **Add a scheduled event**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/schedule/1 -H 'Content-Type: application/json' \
/// -d '{"kind":"media","sources":["Breaks/a.mp4"],"start_date":"2026-10-14","start_time":"13:00:00",
///      "entry_mode":"after_clip","after_mode":"continue"}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn add_schedule_event(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<ScheduleEventInput>,
) -> Result<Json<ScheduleEvent>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let config = manager.config.read().await.clone();
    let mut event = prepare_event(&config, 0, input).await?;

    event.id = handles::insert_schedule_event(&state.pool, &event).await?;
    manager.schedule.invalidate();

    Ok(Json(event))
}

/// **Change a scheduled event**
pub async fn update_schedule_event(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<ScheduleEventInput>,
) -> Result<Json<ScheduleEvent>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let config = manager.config.read().await.clone();
    let not_found = || ServiceError::NotFound(format!("Event {event_id} not found"));
    let previous = handles::select_schedule_event(&state.pool, id, event_id)
        .await?
        .ok_or_else(not_found)?;
    let event = prepare_event(&config, event_id, input).await?;

    if !handles::update_schedule_event(&state.pool, &event).await? {
        return Err(not_found());
    }

    // The event is saved: whatever happens below, the ticker and the queue
    // must work from the new version, so no step returns before they do.
    let mut outcome = Ok(());

    // An event moved to a later time today must play again, even when its
    // earlier time already ran or failed (for example a live without signal).
    if timing_changed(&previous, &event) {
        let today = time_now(&config.channel.timezone)
            .naive_local()
            .date()
            .format("%Y-%m-%d")
            .to_string();

        if let Err(error) = handles::reset_schedule_runs_from(&state.pool, event_id, &today).await {
            outcome = Err(error);
        }
    }

    // An occurrence already waiting for the clip on air (or for its signal)
    // airs as the event is now, or not at all when the change took it away.
    for dropped in manager.schedule.revise(event_id, Some(&event)) {
        if let Err(error) =
            handles::finish_schedule_run(&state.pool, event_id, &dropped.date_key(), "skipped")
                .await
        {
            outcome = outcome.and(Err(error));
        }
    }

    manager.schedule.invalidate();
    outcome?;

    Ok(Json(event))
}

/// **Remove a scheduled event**
pub async fn delete_schedule_event(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<&'static str, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;

    if !handles::delete_schedule_event(&state.pool, id, event_id).await? {
        return Err(ServiceError::NotFound(format!(
            "Event {event_id} not found"
        )));
    }

    // It must not air from the queue either, where it may wait for the clip
    // on air to end.
    manager.schedule.revise(event_id, None);
    manager.schedule.invalidate();

    Ok("Event removed")
}

/// **Play a scheduled event now**
///
/// Cuts what is on air and plays the event right away.
pub async fn trigger_schedule_event(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<&'static str, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let event = handles::select_schedule_event(&state.pool, id, event_id)
        .await?
        .ok_or_else(|| ServiceError::NotFound(format!("Event {event_id} not found")))?;
    let timezone = manager.config.read().await.channel.timezone;
    let now = time_now(&timezone).naive_local();
    let occurrence = occurrence_now(&event, now)
        .ok_or_else(|| ServiceError::Conflict("The event has no valid length".to_string()))?;

    if !manager.is_alive.load(Ordering::SeqCst) {
        return Err(ServiceError::Conflict(
            "The channel is not playing".to_string(),
        ));
    }

    manager.schedule.trigger(occurrence);

    Ok("Event queued")
}

/// **Back to the programme**
///
/// Stops the scheduled event on air; the programme continues as the event
/// says (where it stopped, or on the clock).
pub async fn stop_schedule_event(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<bool>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;

    Ok(Json(return_to_programme(&manager).await))
}

/// **Scheduler state**
///
/// The event on air, events waiting for the clip on air, and what comes in
/// the next 24 hours.
pub async fn get_schedule_status(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<ScheduleStatus>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let timezone = manager.config.read().await.channel.timezone;
    let now = time_now(&timezone).naive_local();
    let to = now + TimeDelta::try_days(1).unwrap_or_default();
    let events = handles::select_schedule_events(&state.pool, id).await?;
    let date = |value: NaiveDateTime| value.format("%Y-%m-%d").to_string();
    let now_text = now.format("%Y-%m-%dT%H:%M:%S").to_string();
    let runs = handles::select_schedule_runs(
        &state.pool,
        id,
        &date(now - TimeDelta::try_days(1).unwrap_or_default()),
        &date(to),
    )
    .await?;
    let next = occurrence_views(&events, &runs, now, to)
        .into_iter()
        .filter(|view| view.status.is_none() && view.start > now_text)
        .collect();
    let queued = manager
        .schedule
        .queued()
        .into_iter()
        .map(|occurrence| OccurrenceView::new(occurrence, Some("running".to_string())))
        .collect();

    let mut active = manager.schedule.active();

    if let Some(clip) = active.as_mut().and_then(|event| event.interrupted.as_mut()) {
        let storage = manager.config.read().await.channel.storage.clone();
        clip.program = program_title(&storage, &clip.source).await;
    }

    Ok(Json(ScheduleStatus {
        active,
        queued,
        next,
    }))
}

/// **Lives in the storage**
pub async fn get_lives(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<LiveSource>>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let storage = manager.config.read().await.channel.storage.clone();

    Ok(Json(list_lives(&storage).await))
}

/// **Create or update a live**
///
/// Writes `Lives/<name>/signal.live` and `tvshow.nfo`; upload the poster to
/// that folder with the file API.
pub async fn add_live(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(live): Json<NewLive>,
) -> Result<Json<LiveSource>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let storage = manager.config.read().await.channel.storage.clone();

    Ok(Json(save_live(&storage, live).await?))
}

/// **Change the address of a live**
///
/// Rewrites the live's `.live` file; its guide data and artwork stay.
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/schedule/1/lives -H 'Content-Type: application/json' \
/// -d '{"path":"Lives/Big Match/signal.live","url":"srt://203.0.113.10:9000"}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn update_live(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(live): Json<LiveAddress>,
) -> Result<Json<LiveSource>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let storage = manager.config.read().await.channel.storage.clone();

    Ok(Json(
        update_live_address(&storage, &live.path, &live.url).await?,
    ))
}

/// **Test a live signal**
pub async fn test_live_signal(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<SignalRequest>,
) -> Result<Json<SignalTest>, ServiceError> {
    authorized_manager(&state, id, &user, &details).await?;

    Ok(Json(test_signal(&request.url).await?))
}

/// **Graphics events of a channel**
///
/// Logo and L-shape changes at a set time, undone at the end.
///
/// ```BASH
/// curl -X GET 'http://127.0.0.1:8787/api/schedule/1/graphics' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_graphics_events(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Vec<crate::db::models::GraphicsEvent>>, ServiceError> {
    authorized_manager(&state, id, &user, &details).await?;

    Ok(Json(
        handles::select_graphics_events(&state.pool, id).await?,
    ))
}

/// **Add a graphics event**
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/schedule/1/graphics -H 'Content-Type: application/json' \
/// -d '{"title": "Patrocínio", "start_date": "2026-10-27", "start_time": "12:00", "end_time": "12:05",
///      "actions": {"logo": "hide", "lshape": "on", "lshape_corner": "bottom-left"}}' \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn add_graphics_event(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<crate::utils::schedule::graphics::GraphicsEventInput>,
) -> Result<Json<crate::db::models::GraphicsEvent>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut event =
        crate::utils::schedule::graphics::prepare_graphics_event(&storage, id, 0, input)?;

    event.id = handles::insert_graphics_event(&state.pool, &event).await?;

    Ok(Json(event))
}

/// **Change a graphics event**
pub async fn update_graphics_event(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(input): Json<crate::utils::schedule::graphics::GraphicsEventInput>,
) -> Result<Json<crate::db::models::GraphicsEvent>, ServiceError> {
    let manager = authorized_manager(&state, id, &user, &details).await?;
    let storage = manager.config.read().await.channel.storage.clone();
    let event =
        crate::utils::schedule::graphics::prepare_graphics_event(&storage, id, event_id, input)?;

    if !handles::update_graphics_event(&state.pool, &event).await? {
        return Err(ServiceError::NotFound(format!(
            "Graphics event {event_id} not found"
        )));
    }

    Ok(Json(event))
}

/// **Remove a graphics event**
pub async fn delete_graphics_event(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, i32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<&'static str>, ServiceError> {
    authorized_manager(&state, id, &user, &details).await?;

    if !handles::delete_graphics_event(&state.pool, id, event_id).await? {
        return Err(ServiceError::NotFound(format!(
            "Graphics event {event_id} not found"
        )));
    }

    Ok(Json("removed"))
}
