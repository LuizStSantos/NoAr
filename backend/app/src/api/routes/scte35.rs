//! The SCTE-35 ad breaks a channel signals on its MPEG-TS output: what is
//! coming and on air, and breaks asked for by hand, for example for a live
//! whose programme goes to commercials.

use std::time::SystemTime;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use log::*;
use noar_engine::{BreakInfo, BreakSource, BreakState, SpliceError, SpliceStatus};
use protect_axum::authorities::AuthDetails;
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        routes::{AuthUser, ensure_any_authority, logo::key_matches},
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
                BreakSource::Marked => "marked",
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
    let info = insert_break(&manager, id, request.duration, "by hand").await?;

    Ok(Json(BreakStatus::from(&info)))
}

async fn insert_break(
    manager: &ChannelManager,
    id: i32,
    duration: f64,
    by: &str,
) -> Result<BreakInfo, ServiceError> {
    let scte35 = manager.config.read().await.scte35.clone();

    if !scte35.signals(duration) {
        return Err(ServiceError::BadRequest(format!(
            "A break runs from {} to {} seconds",
            scte35.min_break, scte35.max_break
        )));
    }

    let info = manager
        .splice_control
        .insert_break(duration)
        .map_err(splice_error)?;

    info!(channel = id;
        "SCTE-35: break {} of <span class=\"log-number\">{:.0}</span> s asked for {by}",
        info.event_id, info.duration
    );

    Ok(info)
}

/// Header an encoder can send the key in, instead of the `key` query.
const KEY_HEADER: &str = "x-scte35-key";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KeyedAction {
    /// Mark a break, from now plus the pre-roll.
    Break,
    /// Call off a break that has not started: the one named, or the next.
    Cancel,
}

#[derive(Debug, Default, Deserialize)]
pub struct KeyedQuery {
    #[serde(default)]
    key: Option<String>,
    /// Seconds, for `break`.
    #[serde(default)]
    duration: Option<f64>,
    /// The break to call off, for `cancel`; the next one when left out.
    #[serde(default)]
    event: Option<u32>,
}

/// **Mark or call off a break, with the channel's SCTE-35 key**
///
/// For encoders and automation that cannot log in, like the logo key. The
/// key is set in Settings › Ad breaks; without one this answers 403.
///
/// ```BASH
/// curl -X POST 'http://127.0.0.1:8787/scte35/1/break?key=<KEY>&duration=120'
/// curl -X POST 'http://127.0.0.1:8787/scte35/1/cancel?key=<KEY>'
/// ```
pub async fn control_scte35_with_key(
    State(state): State<AppState>,
    Path((id, action)): Path<(i32, KeyedAction)>,
    Query(query): Query<KeyedQuery>,
    headers: HeaderMap,
) -> Result<Json<Scte35Status>, ServiceError> {
    let manager = {
        let guard = state.controller.read().await;
        guard.get(id)
    }
    .ok_or_else(|| ServiceError::NotFound(format!("Channel {id} not found!")))?;
    let expected = manager.config.read().await.scte35.key.clone();
    let given = headers
        .get(KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(query.key.as_deref())
        .unwrap_or_default();

    if !key_matches(&expected, given) {
        return Err(ServiceError::Forbidden(
            "Wrong or missing SCTE-35 key".to_string(),
        ));
    }

    match action {
        KeyedAction::Break => {
            let duration = query.duration.ok_or_else(|| {
                ServiceError::BadRequest("Give the break length: duration=<seconds>".to_string())
            })?;

            insert_break(&manager, id, duration, "with the key").await?;
        }
        KeyedAction::Cancel => {
            let event_id = match query.event {
                Some(event_id) => event_id,
                None => next_break_to_cancel(&manager)
                    .ok_or_else(|| ServiceError::NotFound("No break to call off".to_string()))?,
            };

            manager
                .splice_control
                .cancel_break(event_id)
                .map_err(splice_error)?;
            info!(channel = id; "SCTE-35: break {event_id} called off with the key");
        }
    }

    Ok(Json(status(&manager).await))
}

/// The next break that has not started yet.
fn next_break_to_cancel(manager: &ChannelManager) -> Option<u32> {
    manager
        .splice_control
        .status()
        .breaks
        .iter()
        .find(|b| matches!(b.state, BreakState::Scheduled | BreakState::Announced))
        .map(|b| b.event_id)
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
