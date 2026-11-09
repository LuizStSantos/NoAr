//! The graphics cues of a video, run while it plays: at each cue's time the
//! template goes on air (for its duration) or is taken off.

use std::time::Duration;

use log::*;
use tokio_util::sync::CancellationToken;

use crate::{
    graphics::commands,
    player::controller::ChannelManager,
    utils::logo_marks::{CueAction, GraphicsCue},
};

/// Runs a clip's cues while it plays; dropping it calls off the ones not
/// run yet.
pub struct ClipCues(CancellationToken);

impl Drop for ClipCues {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Start running `cues`, each `(seconds after the clip starts, cue)`.
pub fn run_clip_cues(manager: &ChannelManager, cues: Vec<(f64, GraphicsCue)>) -> ClipCues {
    let cancel = CancellationToken::new();

    if cues.is_empty() {
        return ClipCues(cancel);
    }

    let manager = manager.clone();
    let stop = cancel.clone();
    let started = tokio::time::Instant::now();

    tokio::spawn(async move {
        for (after, cue) in cues {
            let at = started + Duration::from_secs_f64(after.max(0.0));

            tokio::select! {
                _ = stop.cancelled() => return,
                _ = tokio::time::sleep_until(at) => {}
            }

            if let Err(error) = run_cue(&manager, &cue).await {
                warn!(channel = manager.id; "Graphics cue at {:.1} s: {error}", cue.at);
            }
        }
    });

    ClipCues(cancel)
}

async fn run_cue(
    manager: &ChannelManager,
    cue: &GraphicsCue,
) -> Result<(), crate::utils::errors::ServiceError> {
    let template = commands::find_template(manager, &cue.template_id.to_string()).await?;

    match cue.action {
        CueAction::Take => {
            commands::take(
                manager,
                template,
                Default::default(),
                Default::default(),
                cue.duration,
            )
            .await?;
        }
        CueAction::Out => {
            commands::off(manager, &template).await?;
        }
    }

    Ok(())
}
