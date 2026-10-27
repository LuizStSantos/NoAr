//! Airs the graphics events of a channel while it plays: each one's logo and
//! L-shape changes when it starts, undone when it ends.

use std::{collections::HashSet, time::Duration};

use chrono::{NaiveDateTime, TimeDelta};
use log::*;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;

use crate::{
    api::routes::{apply_graphics, revert_graphics},
    db::handles,
    player::controller::ChannelManager,
    utils::time_machine::time_now,
};

const TICK: Duration = Duration::from_secs(1);

/// How late a start or an end without an end time is still put on air, as
/// when the channel was busy or just started.
const GRACE_SECONDS: i64 = 60;

/// What the ticker already did: an event's start (`false`) or end (`true`),
/// by event and start.
type Done = HashSet<(i32, NaiveDateTime, bool)>;

pub async fn run_graphics_ticker(manager: ChannelManager, cancel: CancellationToken) {
    let mut done = Done::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = sleep(TICK) => {}
        }

        if let Err(error) = tick(&manager, &mut done).await {
            warn!(channel = manager.id; "Scheduled graphics: {error}");
        }
    }
}

async fn tick(
    manager: &ChannelManager,
    done: &mut Done,
) -> Result<(), crate::utils::errors::ServiceError> {
    let events = handles::select_graphics_events(&manager.db_pool, manager.id).await?;
    let timezone = manager.config.read().await.channel.timezone;
    let now = time_now(&timezone).naive_local();
    let grace = TimeDelta::try_seconds(GRACE_SECONDS).unwrap_or_default();
    let day = TimeDelta::try_days(1).unwrap_or_default();

    for event in &events {
        for date in [(now - day).date(), now.date()] {
            let Some((start, end)) = event.window_on(date) else {
                continue;
            };
            // On from its start until its end; without an end, only around
            // the start, so a restart does not bring back an old change.
            let on_now = now >= start && end.map_or(now - start <= grace, |end| now < end);

            if on_now && done.insert((event.id, start, false)) {
                match apply_graphics(manager, &event.actions).await {
                    Ok(()) => info!(channel = manager.id;
                        "Scheduled graphics <span class=\"log-addr\">{}</span> on air",
                        event.title
                    ),
                    Err(error) => warn!(channel = manager.id;
                        "Scheduled graphics <span class=\"log-addr\">{}</span>: {error}",
                        event.title
                    ),
                }
            }

            if let Some(end) = end
                && now >= end
                && now - end <= grace
                && done.insert((event.id, start, true))
            {
                match revert_graphics(manager, &event.actions).await {
                    Ok(()) => info!(channel = manager.id;
                        "Scheduled graphics <span class=\"log-addr\">{}</span> ended",
                        event.title
                    ),
                    Err(error) => warn!(channel = manager.id;
                        "Scheduled graphics <span class=\"log-addr\">{}</span> did not end: {error}",
                        event.title
                    ),
                }
            }
        }
    }

    done.retain(|(_, start, _)| *start > now - day - day);

    Ok(())
}
