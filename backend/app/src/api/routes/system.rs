use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{handles, models::Role},
    utils::{
        errors::ServiceError,
        server,
        update::{self, UpdateStatus},
        updater,
    },
};

/// ### System Statistics
///
/// Get statistics about CPU, Ram, Disk, etc. usage.
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/system/1
/// -H 'Content-Type: application/json' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_system_stat(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<crate::utils::system::SystemStat>, ServiceError> {
    ensure_any_authority(
        &details,
        &[&Role::GlobalAdmin, &Role::ChannelAdmin, &Role::User],
    )?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    let config = manager.config.read().await.clone();

    let stat = state.system.stat(&config).await;

    Ok(Json(stat))
}

#[derive(Debug, Serialize)]
pub struct ServerInfo {
    /// Unix time this run of NoAr started.
    started_at: i64,
    version: &'static str,
    channels: usize,
    channels_running: usize,
}

/// ### Server
///
/// When NoAr started and how many channels are on air.
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/system/server -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_server_info(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<ServerInfo>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    let managers = state.controller.read().await.managers.clone();
    let channels_running = managers
        .iter()
        .filter(|manager| manager.is_alive.load(Ordering::SeqCst))
        .count();

    Ok(Json(ServerInfo {
        started_at: server::started_at(),
        version: env!("CARGO_PKG_VERSION"),
        channels: managers.len(),
        channels_running,
    }))
}

/// ### Restart
///
/// Stop every channel and start NoAr again in the same process, like after
/// a reboot: the channels that were on air start again by themselves.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/system/restart -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn restart_server(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<&'static str>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    server::request_restart(state.shutdown.clone());

    Ok(Json("Restarting"))
}

/// ### Update status
///
/// The installed version and the newest published image, as of the last
/// check.
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/system/update -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn get_update_status(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    Ok(Json(update::status(&state.pool).await?))
}

/// ### Check for an update
///
/// Ask GitHub for the newest published image now.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/system/update/check -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn check_for_update(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    Ok(Json(update::check(&state.pool).await?))
}

/// Changes to the update settings; fields left out keep their value.
#[derive(Debug, Deserialize)]
pub struct UpdateSettingsRequest {
    auto_check: Option<bool>,
    auto_update: Option<bool>,
    /// Server time, `HH:MM`.
    update_time: Option<String>,
}

/// ### Update settings
///
/// Turn the check every six hours and the automatic install on or off, and
/// set the time of the automatic install.
///
/// ```BASH
/// curl -X PUT http://127.0.0.1:8787/api/system/update/settings -H 'Content-Type: application/json' \
/// -d '{ "auto_check": true, "auto_update": true, "update_time": "03:00" }' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn save_update_settings(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
    Json(request): Json<UpdateSettingsRequest>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    let mut settings = handles::select_update_settings(&state.pool).await?;

    if let Some(time) = request.update_time {
        // Checks the format.
        updater::next_time(&time)?;
        settings.update_time = time.trim().to_string();
    }

    settings.auto_check = request.auto_check.unwrap_or(settings.auto_check);
    settings.auto_update = request.auto_update.unwrap_or(settings.auto_update);
    handles::update_update_settings(&state.pool, &settings).await?;

    Ok(Json(update::status(&state.pool).await?))
}

/// ### Install the update
///
/// Ask the updater container to install the newest image now. NoAr answers,
/// then stops when the updater replaces it, and starts again on the new
/// version.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/system/update/install -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn install_update(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;
    updater::install().await?;

    Ok(Json(update::status(&state.pool).await?))
}

#[derive(Debug, Deserialize)]
pub struct ScheduleUpdateRequest {
    /// Server time, `HH:MM`: the next time the clock shows it.
    time: String,
}

/// ### Schedule the update
///
/// Install the newest image at the next `time` on the server clock;
/// `DELETE` cancels it.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/system/update/schedule -H 'Content-Type: application/json' \
/// -d '{ "time": "03:00" }' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn schedule_update(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
    Json(request): Json<ScheduleUpdateRequest>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    let mut settings = handles::select_update_settings(&state.pool).await?;

    settings.scheduled_at = Some(updater::next_time(&request.time)?);
    handles::update_update_settings(&state.pool, &settings).await?;

    Ok(Json(update::status(&state.pool).await?))
}

pub async fn cancel_scheduled_update(
    State(state): State<AppState>,
    details: AuthDetails<Role>,
) -> Result<Json<UpdateStatus>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin])?;

    let mut settings = handles::select_update_settings(&state.pool).await?;

    settings.scheduled_at = None;
    handles::update_update_settings(&state.pool, &settings).await?;

    Ok(Json(update::status(&state.pool).await?))
}
