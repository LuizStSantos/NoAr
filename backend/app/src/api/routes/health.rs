use std::{sync::atomic::Ordering, time::Duration};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;

use crate::{api::state::AppState, player::controller::ChannelManager};

/// Output silent for longer than this (it carries silence too, as long as
/// it airs anything) means the playout froze.
const OUTPUT_STALE: Duration = Duration::from_secs(10);
/// A playout that just started may not have output anything yet.
const STARTING_GRACE_SECONDS: i64 = 30;

#[derive(Debug, Serialize)]
pub struct HealthReport {
    /// `ok` when every channel set to run is on air.
    pub status: &'static str,
    pub version: &'static str,
    pub channels: Vec<ChannelHealth>,
}

#[derive(Debug, Serialize)]
pub struct ChannelHealth {
    pub id: i32,
    /// `on_air`, `starting`, `stopped` (stopped on purpose), `frozen`
    /// (running, output stalled), `failed` (off air, being retried) or
    /// `down` (set to run, not running).
    pub state: &'static str,
    /// Seconds since the playout started.
    pub uptime: Option<i64>,
    /// Seconds since the output last carried anything.
    pub output_age: Option<f64>,
}

impl ChannelHealth {
    fn healthy(&self) -> bool {
        matches!(self.state, "on_air" | "starting" | "stopped")
    }
}

/// What a channel's playout is doing, from what it reports.
fn channel_state(
    active: bool,
    alive: bool,
    failed: bool,
    uptime: Option<i64>,
    output_age: Option<Duration>,
) -> &'static str {
    if failed {
        return "failed";
    }
    if !alive {
        return if active { "down" } else { "stopped" };
    }

    match output_age {
        Some(age) if age <= OUTPUT_STALE => "on_air",
        _ if uptime.is_none_or(|uptime| uptime < STARTING_GRACE_SECONDS) => "starting",
        _ => "frozen",
    }
}

async fn channel_health(manager: &ChannelManager) -> ChannelHealth {
    let active = manager.channel.lock().await.active;
    let uptime = manager.uptime();
    let output_age = manager.output_age();

    ChannelHealth {
        id: manager.id,
        state: channel_state(
            active,
            manager.is_alive.load(Ordering::SeqCst),
            manager.failure().is_some(),
            uptime,
            output_age,
        ),
        uptime,
        output_age: output_age.map(|age| (age.as_secs_f64() * 10.0).round() / 10.0),
    }
}

/// **Health**
///
/// For uptime monitors: answers `200` when every channel set to run is on
/// air, `503` when one is down, failed or frozen. No login needed; it tells
/// only channel ids and states.
///
/// ```BASH
/// curl -i http://127.0.0.1:8787/health
/// ```
pub async fn get_health(State(state): State<AppState>) -> impl IntoResponse {
    let managers = state.controller.read().await.managers.clone();
    let mut channels = Vec::with_capacity(managers.len());

    for manager in &managers {
        channels.push(channel_health(manager).await);
    }

    let healthy = channels.iter().all(ChannelHealth::healthy);
    let report = HealthReport {
        status: if healthy { "ok" } else { "error" },
        version: env!("CARGO_PKG_VERSION"),
        channels,
    };
    let code = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(report))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn a_running_playout_with_output_is_on_air() {
        assert_eq!(
            channel_state(true, true, false, Some(600), Some(SECOND)),
            "on_air"
        );
    }

    #[test]
    fn a_running_playout_without_output_is_frozen_after_the_grace() {
        assert_eq!(channel_state(true, true, false, Some(5), None), "starting");
        assert_eq!(channel_state(true, true, false, Some(600), None), "frozen");
        assert_eq!(
            channel_state(true, true, false, Some(600), Some(OUTPUT_STALE + SECOND)),
            "frozen"
        );
    }

    #[test]
    fn a_stopped_channel_is_healthy_unless_it_should_run() {
        assert_eq!(channel_state(false, false, false, None, None), "stopped");
        assert_eq!(channel_state(true, false, false, None, None), "down");
        assert_eq!(channel_state(true, false, true, None, None), "failed");
    }

    #[test]
    fn only_problems_make_the_report_unhealthy() {
        let health = |state| ChannelHealth {
            id: 1,
            state,
            uptime: None,
            output_age: None,
        };

        for state in ["on_air", "starting", "stopped"] {
            assert!(health(state).healthy());
        }
        for state in ["frozen", "failed", "down"] {
            assert!(!health(state).healthy());
        }
    }
}
