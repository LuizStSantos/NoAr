use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, State},
};
use protect_axum::authorities::AuthDetails;
use serde::Serialize;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::models::Role,
    utils::{errors::ServiceError, server},
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
