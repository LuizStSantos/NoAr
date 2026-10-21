//! The SCTE-35 ad breaks a channel signals on its MPEG-TS output: what is
//! coming and on air, and breaks asked for by hand, for example for a live
//! whose programme goes to commercials.

use std::time::SystemTime;

use axum::{
    Json,
    extract::{Path, State},
};
use log::*;
use noar_engine::{BreakInfo, BreakSource, BreakState, SpliceError, SpliceStatus};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority},
        state::AppState,
    },
    db::models::Role,
    player::controller::ChannelManager,
    utils::errors::ServiceError,
};

#[derive(Debug, Deserialize)]
pub struct BreakRequest {
    /// Seconds.
    duration: f64,
}

#[derive(Debug, Serialize)]
pub struct Scte35Status {
    /// SCTE-35 is on in the settings.
    enabled: bool,
    /// The output on air carries it.
    active: bool,
    pid: u16,
    preroll: f64,
    min_break: f64,
    max_break: f64,
    /// Why the output carries no SCTE-35 although it should.
    error: Option<String>,
    breaks: Vec<BreakStatus>,
    recent: Vec<BreakStatus>,
}

#[derive(Debug, Serialize)]
pub struct BreakStatus {
    event_id: u32,
    source: &'static str,
    state: &'static str,
    duration: f64,
    /// Unix milliseconds.
    starts_at: i64,
    ends_at: i64,
    out_messages: u8,
    in_messages: u8,
    out_key_frame: Option<bool>,
    in_key_frame: Option<bool>,
}

impl From<&BreakInfo> for BreakStatus {
    fn from(info: &BreakInfo) -> Self {
        Self {
            event_id: info.event_id,
            source: match info.source {
                BreakSource::Commercials => "commercials",
                BreakSource::Manual => "manual",
            },
            state: match info.state {
                BreakState::Scheduled => "scheduled",
                BreakState::Announced => "announced",
                BreakState::OnAir => "on_air",
                BreakState::Ended => "ended",
                BreakState::Cancelled => "cancelled",
            },
            duration: info.duration,
            starts_at: unix_millis(info.starts_at),
            ends_at: unix_millis(info.ends_at),
            out_messages: info.out_messages,
            in_messages: info.in_messages,
            out_key_frame: info.out_key_frame,
            in_key_frame: info.in_key_frame,
        }
    }
}

fn unix_millis(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// **What the SCTE-35 signal is doing**
pub async fn get_scte35_state(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Scte35Status>, ServiceError> {
    let manager = authorized(&state, id, &user, &details).await?;

    Ok(Json(status(&manager).await))
}

/// **Signal a break by hand**
///
/// The break starts once the pre-roll has passed, and lasts `duration`
/// seconds. It only marks the stream: what airs meanwhile is up to the
/// programme.
///
/// ```BASH
/// curl -X POST http://127.0.0.1:8787/api/control/1/scte35/break -H 'Content-Type: application/json' \
/// -d '{"duration": 120}' -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn insert_scte35_break(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    user: AuthUser,
    details: AuthDetails<Role>,
    Json(request): Json<BreakRequest>,
) -> Result<Json<BreakStatus>, ServiceError> {
    let manager = authorized(&state, id, &user, &details).await?;
    let scte35 = manager.config.read().await.scte35.clone();

    if !scte35.signals(request.duration) {
        return Err(ServiceError::BadRequest(format!(
            "A break runs from {} to {} seconds",
            scte35.min_break, scte35.max_break
        )));
    }

    let info = manager
        .splice_control
        .insert_break(request.duration)
        .map_err(splice_error)?;

    info!(channel = id;
        "SCTE-35: break {} of <span class=\"log-number\">{:.0}</span> s asked for by hand",
        info.event_id, info.duration
    );

    Ok(Json(BreakStatus::from(&info)))
}

/// **Call off a break that has not started**
///
/// ```BASH
/// curl -X DELETE http://127.0.0.1:8787/api/control/1/scte35/break/1761060000 \
/// -H 'Authorization: Bearer <TOKEN>'
/// ```
pub async fn cancel_scte35_break(
    State(state): State<AppState>,
    Path((id, event_id)): Path<(i32, u32)>,
    user: AuthUser,
    details: AuthDetails<Role>,
) -> Result<Json<Scte35Status>, ServiceError> {
    let manager = authorized(&state, id, &user, &details).await?;

    manager
        .splice_control
        .cancel_break(event_id)
        .map_err(splice_error)?;

    info!(channel = id; "SCTE-35: break {event_id} called off by hand");

    Ok(Json(status(&manager).await))
}

fn splice_error(error: SpliceError) -> ServiceError {
    match error {
        SpliceError::NotFound(_) => ServiceError::NotFound(error.to_string()),
        SpliceError::Inactive | SpliceError::Overlaps(_) | SpliceError::Started(_) => {
            ServiceError::Conflict(error.to_string())
        }
        SpliceError::InvalidDuration => ServiceError::BadRequest(error.to_string()),
    }
}

async fn authorized(
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

async fn status(manager: &ChannelManager) -> Scte35Status {
    let scte35 = manager.config.read().await.scte35.clone();
    let SpliceStatus {
        active,
        error,
        breaks,
        recent,
        ..
    } = manager.splice_control.status();

    Scte35Status {
        enabled: scte35.enable,
        active,
        pid: scte35.pid,
        preroll: scte35.preroll,
        min_break: scte35.min_break,
        max_break: scte35.max_break,
        error: scte35.enable.then_some(error).flatten(),
        breaks: breaks.iter().map(BreakStatus::from).collect(),
        recent: recent.iter().map(BreakStatus::from).collect(),
    }
}
