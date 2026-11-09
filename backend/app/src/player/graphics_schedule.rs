//! Airs the scheduled graphics of a channel while it plays: each puts its
//! template on air when it starts and takes it off when it ends. Logo and
//! L-shape events of older versions are left alone.

use std::{collections::HashSet, time::Duration};

use chrono::{NaiveDateTime, TimeDelta};
use log::*;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

#[path = "graphics_schedule/graphics.rs"]
mod templates;

use crate::{db::handles, player::controller::ChannelManager, utils::time_machine::time_now};

const TICK: Duration = Duration::from_secs(1);

/// How late a start or an end without an end time is still put on air, as
/// when the channel was busy or just started.
const GRACE_SECONDS: i64 = 60;

/// What the ticker already did: an event's start (`false`) or end (`true`),
/// by event and start.
type Done = HashSet<(i32, NaiveDateTime, bool)>;

pub async fn run_graphics_ticker(manager: ChannelManager, cancel: CancellationToken) {
    let mut done = Done::new();
    let mut templates = templates::TemplateSchedules::default();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(TICK) => {}
        }

        if let Err(error) = tick(&manager, &mut done, &mut templates).await {
            warn!(channel = manager.id; "Scheduled graphics: {error}");
        }
    }

    templates.stop(&manager);
}

async fn tick(
    manager: &ChannelManager,
    done: &mut Done,
    templates: &mut templates::TemplateSchedules,
) -> Result<(), crate::utils::errors::ServiceError> {
    let events = handles::select_graphics_events(&manager.db_pool, manager.id).await?;
    let timezone = manager.config.read().await.channel.timezone;
    let now = time_now(&timezone).naive_local();
    let day = TimeDelta::try_days(1).unwrap_or_default();

    templates.tick(manager, &events, now, done).await?;

    done.retain(|(_, start, _)| *start > now - day - day);

    Ok(())
}
