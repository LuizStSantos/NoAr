use std::sync::atomic::Ordering;

use axum::{
    Json,
    extract::{Path, State},
};
use log::*;
use protect_axum::authorities::AuthDetails;
use serde::Deserialize;

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::{handles, models::Role},
    player::utils::get_data_map,
    utils::{
        control::{ControlParams, Process, ProcessCtl, control_state},
        errors::ServiceError,
    },
};

#[derive(Debug, Deserialize)]
pub struct AudioEffectsUpdate {
    volume: f64,
}

pub async fn update_audio_effects(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(update): Json<AudioEffectsUpdate>,
) -> Result<Json<&'static str>, ServiceError> {
    ensure_any_authority(&details, &[&Role::GlobalAdmin, &Role::ChannelAdmin])?;
    user.ensure_channel_or_admin(id)?;

    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::BadRequest(format!("Channel {id} not found!")))?;

    noar_engine::AudioEffectsControl::new(update.volume)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;

    let config_id = manager.config.read().await.general.id;
    handles::update_configuration_volume(&state.pool, config_id, update.volume).await?;

    manager
        .audio_effects
        .set_volume(update.volume)
        .map_err(|error| ServiceError::BadRequest(error.to_string()))?;

    manager.config.write().await.audio.volume = update.volume;

    Ok(Json("Success"))
}

/// **Control Playout**
///
/// - next
/// - back
/// - reset
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/playout -H 'Content-Type: application/json'
/// -d '{ "command": "reset" }' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn control_playout(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(control): Json<ControlParams>,
) -> Result<Json<serde_json::Map<String, serde_json::Value>>, ServiceError> {
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

    if manager.is_processing.load(Ordering::SeqCst) {
        return Err(ServiceError::Conflict(
            "A command is already being processed, please wait".to_string(),
        ));
    }

    manager.is_processing.store(true, Ordering::SeqCst);

    let resp = match control_state(&state.pool, &manager, &control.control).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => Err(e),
    };

    manager.is_processing.store(false, Ordering::SeqCst);

    resp
}

/// **Get current Clip**
///
/// ```BASH
/// curl -X GET http://127.0.0.1:8787/api/control/1/media/current
/// -H 'Content-Type: application/json' -H 'Authorization: Bearer <TOKEN>'
/// ```
///
/// **Response:**
///
/// ```JSON
///     {
///       "media": {
///         "ad": false,
///         "duration": 154.2,
///         "out": 154.2,
///         "in": 0.0,
///         "source": "/opt/tv-media/clip.mp4"
///       },
///       "index": 39,
///       "ingest": false,
///       "ingest_listener_id": null,
///       "ingest_listener_name": null,
///       "ingest_listener_backend": null,
///       "mode": "playlist",
///       "played": 67.808
///     }
/// ```
pub async fn media_current(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<serde_json::Map<String, serde_json::Value>>, ServiceError> {
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

    let media_map = get_data_map(&manager).await;

    Ok(Json(media_map))
}

/// #### NoAr Process Control
///
/// Control the NoAr process, like:
/// - start
/// - stop
/// - restart
/// - status
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/process
/// -H 'Content-Type: application/json' -H 'Authorization: Bearer <TOKEN>'
/// -d '{"command": "start"}'
/// ```
pub async fn process_control(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(proc): Json<Process>,
) -> Result<Json<&'static str>, ServiceError> {
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

    manager.list_init.store(true, Ordering::SeqCst);

    let guard = manager.begin_processing().ok_or_else(|| {
        ServiceError::Conflict("A command is already being processed, please wait".to_string())
    })?;

    // The command runs in its own task and holds the claim until it is done:
    // if the client disconnects halfway through a restart, the channel is
    // still started again, and the claim is released whatever happens.
    let task = tokio::spawn(async move {
        let _guard = guard;
        run_process_command(&manager, proc.command).await
    });

    task.await.map_err(|error| {
        error!(channel = id; "Channel command failed: {error}");
        ServiceError::InternalServerError
    })?
}

async fn run_process_command(
    manager: &crate::player::controller::ChannelManager,
    command: ProcessCtl,
) -> Result<Json<&'static str>, ServiceError> {
    match command {
        ProcessCtl::Status => {
            if manager.is_alive.load(Ordering::SeqCst) {
                Ok(Json("active"))
            } else {
                Ok(Json("not running"))
            }
        }
        ProcessCtl::Start => {
            if !manager.is_alive.load(Ordering::SeqCst) {
                manager.channel.lock().await.active = true;
                manager.start().await?;
            }
            Ok(Json("Success"))
        }
        ProcessCtl::Stop => {
            manager.channel.lock().await.active = false;
            manager.stop_all(true).await;
            manager.stop_supervisor().await;
            Ok(Json("Success"))
        }
        ProcessCtl::Restart => {
            manager.channel.lock().await.active = false;
            manager.stop_all(false).await;
            manager.stop_supervisor().await;

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            manager.channel.lock().await.active = true;
            manager.start().await?;
            Ok(Json("Success"))
        }
    }
}
