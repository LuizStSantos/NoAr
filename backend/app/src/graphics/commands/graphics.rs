//! The two graphics commands: put a template on air, take a template off.
//!
//! They work the same for every template: a temporary graphic (GC), a
//! picture-window template (L-shape) and an always-on template (logo,
//! ticker). A duration takes the template off by itself; zero or none keeps
//! it on until it is taken off.

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
