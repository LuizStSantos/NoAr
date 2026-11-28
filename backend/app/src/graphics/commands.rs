//! The two graphics commands: put a template on air, take a template off.
//!
//! They work the same for every template: a temporary graphic (GC), a
//! picture-window template (L-shape) and an always-on template (logo,
//! ticker). A duration takes the template off by itself; zero or none keeps
//! it on until it is taken off.
//!
//! Two more serve controllers: the list of everything on air, and taking
//! everything off at once, optionally except some templates.

use std::{collections::HashMap, time::Duration};

use serde::Serialize;

use crate::{
    db::{handles, models::GraphicsTemplate},
    player::controller::ChannelManager,
    utils::{config::slug, errors::ServiceError},
};

/// Longest duration a command accepts: one day.
const MAX_SECONDS: f64 = 86_400.0;

/// The template a command names: its id, or its name as a slug
/// (`Comercial L-shape` is `comercial-l-shape`).
pub async fn find_template(
    manager: &ChannelManager,
    reference: &str,
) -> Result<GraphicsTemplate, ServiceError> {
    let reference = reference.trim();
    let templates = handles::select_graphics_templates(&manager.db_pool, manager.id).await?;
    let wanted = slug(reference);

    templates
        .into_iter()
        .find(|template| {
            reference.parse::<i32>().ok() == Some(template.id) || slug(&template.name) == wanted
        })
        .ok_or_else(|| ServiceError::NotFound(format!("Template '{reference}' not found")))
}

/// Whether the template is switched on and off rather than taken as a GC.
async fn is_channel_graphic(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
) -> Result<bool, ServiceError> {
    Ok(template.always_on == Some(true)
        || handles::graphics_template_is_bound(&manager.db_pool, manager.id, template.id).await?)
}

fn checked_duration(seconds: Option<f64>) -> Result<Option<Duration>, ServiceError> {
    match seconds {
        None => Ok(None),
        Some(seconds) if seconds.is_finite() && (0.0..=MAX_SECONDS).contains(&seconds) => {
            Ok((seconds > 0.0).then(|| Duration::from_secs_f64(seconds)))
        }
        Some(_) => Err(ServiceError::BadRequest(
            "duration is in seconds, 0 (until taken off) up to a day".to_string(),
        )),
    }
}

/// What a command did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CommandOutcome {
    /// On air; `take_id` is set for a temporary graphic.
    OnAir {
        template_id: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        take_id: Option<u64>,
    },
    /// It goes on air when the L-shape on air ends.
    Waiting { template_id: i32 },
    /// Taken off air (or it was not on).
    Off { template_id: i32 },
}

/// Put `template` on air. `duration` (seconds) replaces the template's own
/// for a GC; for a channel graphic it takes it off after that time. Zero
/// keeps it on until it is taken off.
pub async fn take(
    manager: &ChannelManager,
    template: GraphicsTemplate,
    fields: HashMap<String, String>,
    records: HashMap<i32, usize>,
    duration: Option<f64>,
) -> Result<CommandOutcome, ServiceError> {
    let template_id = template.id;
    let off_after = checked_duration(duration)?;

    if !is_channel_graphic(manager, &template).await? {
        let outcome = super::take_template(manager, template, fields, records, duration).await?;

        return Ok(match outcome {
            super::TakeOutcome::OnAir { take_id } => CommandOutcome::OnAir {
                template_id,
                take_id: Some(take_id),
            },
            super::TakeOutcome::Waiting => CommandOutcome::Waiting { template_id },
        });
    }

    {
        let _publication = manager.graphics_takes.lock_publication().await;
        super::control_channel_graphic(manager, template_id, true).await?;
    }

    let generation = manager.graphics_takes.arm_channel_timer(template_id);

    if let Some(after) = off_after {
        let manager = manager.clone();

        tokio::spawn(async move {
            tokio::time::sleep(after).await;

            // A later take or off of the same template replaces this timer.
            if !manager
                .graphics_takes
                .channel_timer_is(template_id, generation)
            {
                return;
            }

            let _publication = manager.graphics_takes.lock_publication().await;

            if let Err(error) = super::control_channel_graphic(&manager, template_id, false).await {
                log::warn!(channel = manager.id; "Template {template_id} not taken off after its duration: {error}");
            }
        });
    }

    Ok(CommandOutcome::OnAir {
        template_id,
        take_id: None,
    })
}

/// Take `template` off air: every GC of it, or the channel graphic itself.
pub async fn off(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
) -> Result<CommandOutcome, ServiceError> {
    let template_id = template.id;
    let _publication = manager.graphics_takes.lock_publication().await;

    if is_channel_graphic(manager, template).await? {
        manager.graphics_takes.arm_channel_timer(template_id);
        super::control_channel_graphic(manager, template_id, false).await?;
    } else {
        for graphic in super::graphics_on_air(manager)
            .into_iter()
            .filter(|graphic| graphic.template_id == template_id && !graphic.leaving)
        {
            manager.graphics_control.take_out(graphic.take_id);
        }
    }

    Ok(CommandOutcome::Off { template_id })
}

/// One graphic on air, as an external controller sees it.
#[derive(Debug, Serialize)]
pub struct OnAirGraphic {
    pub template_id: i32,
    /// The name to use in commands (`comercial-l-shape`).
    pub template: String,
    pub name: String,
    /// `temporary` for a GC; `logo`, `ticker`, `lshape` or `permanent` for
    /// the channel's graphics.
    pub kind: String,
    /// GCs only: seconds since taken, and the id of this take.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_id: Option<u64>,
}

/// Everything on air: the channel's graphics that are on, then the GCs.
pub async fn on_air(manager: &ChannelManager) -> Result<Vec<OnAirGraphic>, ServiceError> {
    let mut list: Vec<_> = super::channel_graphics(manager)
        .await?
        .into_iter()
        .filter(|view| view.on)
        .map(|view| OnAirGraphic {
            template_id: view.template.id,
            template: slug(&view.template.name),
            name: view.template.name,
            kind: view.kind,
            since: None,
            take_id: None,
        })
        .collect();
    let templates = handles::select_graphics_templates(&manager.db_pool, manager.id).await?;

    list.extend(
        super::graphics_on_air(manager)
            .into_iter()
            .filter(|graphic| !graphic.leaving)
            .map(|graphic| OnAirGraphic {
                template_id: graphic.template_id,
                template: templates
                    .iter()
                    .find(|template| template.id == graphic.template_id)
                    .map_or_else(
                        || graphic.template_id.to_string(),
                        |template| slug(&template.name),
                    ),
                name: graphic.name,
                kind: "temporary".to_string(),
                since: Some(graphic.since),
                take_id: Some(graphic.take_id),
            }),
    );

    Ok(list)
}

/// Take everything off air (GCs, GCs waiting for the L-shape, and the
/// channel's logo, ticker, L-shape and always-on graphics) except `keep`.
pub async fn all_off(
    manager: &ChannelManager,
    keep: &[GraphicsTemplate],
) -> Result<(), ServiceError> {
    let keep: Vec<i32> = keep.iter().map(|template| template.id).collect();
    let channel = super::channel_graphics(manager).await?;

    {
        let _publication = manager.graphics_takes.lock_publication().await;

        if keep.is_empty() {
            manager.graphics_takes.cancel_pending();
            manager.graphics_control.take_all_out();
        } else {
            manager.graphics_takes.cancel_pending_except(&keep);

            for graphic in super::graphics_on_air(manager)
                .into_iter()
                .filter(|graphic| !graphic.leaving && !keep.contains(&graphic.template_id))
            {
                manager.graphics_control.take_out(graphic.take_id);
            }
        }
    }

    for view in channel
        .into_iter()
        .filter(|view| view.on && !keep.contains(&view.template.id))
    {
        off(manager, &view.template).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_checked_and_zero_means_until_off() {
        assert_eq!(checked_duration(None).unwrap(), None);
        assert_eq!(checked_duration(Some(0.0)).unwrap(), None);
        assert_eq!(
            checked_duration(Some(50.0)).unwrap(),
            Some(Duration::from_secs(50))
        );
        assert!(checked_duration(Some(-1.0)).is_err());
        assert!(checked_duration(Some(f64::NAN)).is_err());
        assert!(checked_duration(Some(90_000.0)).is_err());
    }

    #[test]
    fn templates_are_named_by_slug() {
        assert_eq!(slug("Comercial L-shape"), "comercial-l-shape");
        assert_eq!(slug("  Logo do canal "), "logo-do-canal");
    }
}
