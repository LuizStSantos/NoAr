//! Ownership of scheduled model instances and reversible permanent visibility.

use std::collections::HashMap;

use chrono::{NaiveDateTime, TimeDelta};

use crate::{
    db::{
        handles,
        models::{GraphicsEvent, GraphicsRole},
    },
    graphics::{DuringLShape, TakeOutcome, take_template_locked},
    player::controller::ChannelManager,
    utils::errors::ServiceError,
};

use super::Done;

type Occurrence = (i32, NaiveDateTime);

#[derive(Clone, Copy)]
enum Activation {
    Temporary(u64),
    Permanent(i32),
}

enum Visibility {
    Logo,
    Ticker(Option<bool>),
    Permanent(bool),
}

struct Run {
    event: GraphicsEvent,
    end: Option<NaiveDateTime>,
    activation: Activation,
}

#[derive(Default)]
pub(super) struct TemplateSchedules {
    runs: HashMap<Occurrence, Run>,
    visibility: HashMap<i32, Visibility>,
}

impl TemplateSchedules {
    fn visible(&self, manager: &ChannelManager, id: i32) -> bool {
        match self.visibility.get(&id) {
            Some(Visibility::Logo) => manager.logo_control.is_visible(),
            Some(Visibility::Ticker(_)) => {
                manager
                    .graphics_control
                    .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
                    == Some(true)
            }
            Some(Visibility::Permanent(_)) => {
                manager.graphics_control.persistent_visible(id as u64) == Some(true)
            }
            None => false,
        }
    }

    pub async fn tick(
        &mut self,
        manager: &ChannelManager,
        events: &[GraphicsEvent],
        now: NaiveDateTime,
        done: &mut Done,
    ) -> Result<(), ServiceError> {
        let on_air = manager.graphics_control.on_air();
        let expired: Vec<_> = self
            .runs
            .iter()
            .filter(|(_, run)| {
                run.end.is_some_and(|end| now >= end)
                    || (run.end.is_none() && matches!(run.activation, Activation::Permanent(id) if !self.visible(manager, id)))
                    || matches!(run.activation, Activation::Temporary(id) if !on_air.iter().any(|take| take.id == id))
                    || !events
                        .iter()
                        .any(|event| event.enabled && *event == run.event)
            })
            .map(|(key, _)| *key)
            .collect();

        for key in expired {
            let run = self
                .runs
                .remove(&key)
                .expect("key collected from active runs");
            let changed = !events
                .iter()
                .any(|event| event.enabled && *event == run.event);
            let overridden = run.end.is_none()
                && matches!(run.activation, Activation::Permanent(id) if !self.visible(manager, id));
            self.finish(manager, run.activation, !overridden);

            if changed {
                done.remove(&(key.0, key.1, false));
            }
        }

        let day = TimeDelta::days(1);
        let grace = TimeDelta::seconds(super::GRACE_SECONDS);

        for event in events.iter().filter(|event| event.template.is_some()) {
            for date in [(now - day).date(), now.date()] {
                let Some((start, end)) = event.window_on(date) else {
                    continue;
                };
                let key = (event.id, start);
                let on_now = now >= start && end.map_or(now - start <= grace, |end| now < end);

                if !on_now || done.contains(&(event.id, start, false)) {
                    continue;
                }

                match self.start(manager, event).await {
                    Ok(Some(activation)) => {
                        done.insert((event.id, start, false));
                        if end.is_none()
                            && let Activation::Permanent(id) = activation
                        {
                            self.runs.retain(|_, run| !(run.end.is_none() && matches!(run.activation, Activation::Permanent(other) if other == id)));
                        }

                        self.runs.insert(
                            key,
                            Run {
                                event: event.clone(),
                                end,
                                activation,
                            },
                        );
                        log::info!(channel=manager.id; "Scheduled model '{}' on air", event.title);
                    }
                    Ok(None) => {} // Wait within this occurrence, never spawn an unowned take.
                    Err(error) => {
                        done.insert((event.id, start, false));
                        log::warn!(channel=manager.id; "Scheduled model '{}' not taken: {error}", event.title);
                    }
                }
            }
        }

        Ok(())
    }

    async fn start(
        &mut self,
        manager: &ChannelManager,
        event: &GraphicsEvent,
    ) -> Result<Option<Activation>, ServiceError> {
        let _publication = manager.graphics_takes.lock_publication().await;
        let selection = event
            .template
            .as_ref()
            .expect("template event filtered by caller");
        let mut template =
            handles::select_graphics_template(&manager.db_pool, manager.id, selection.template_id)
                .await?
                .ok_or_else(|| {
                    ServiceError::NotFound("Scheduled model no longer exists".to_string())
                })?;
        let bindings = handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
        let role = bindings
            .iter()
            .find(|binding| binding.template_id == template.id)
            .map(|binding| binding.role);
        let permanent = matches!(role, Some(GraphicsRole::Logo | GraphicsRole::Ticker))
            || template.always_on == Some(true);

        if permanent {
            let has_content = {
                let config = manager.config.read().await;
                crate::graphics::graphic_has_content(&template, &config)
            };

            if !has_content {
                return Err(ServiceError::Conflict(
                    "Scheduled permanent model has no drawable content".to_string(),
                ));
            }

            if self.visibility.contains_key(&template.id) {
                return Ok(Some(Activation::Permanent(template.id)));
            }

            let previous = match role {
                Some(GraphicsRole::Logo) => {
                    manager.logo_control.set_scheduled_visible(true);
                    Visibility::Logo
                }
                Some(GraphicsRole::Ticker) => {
                    let previous = manager
                        .graphics_control
                        .always_on_visible(noar_engine::AlwaysOnSlot::Ticker);

                    if previous.is_none() {
                        let config = manager.config.read().await.clone();
                        let spec = crate::graphics::graphic_spec_for_channel(
                            &template,
                            &HashMap::new(),
                            &config,
                        )?;
                        let graphic = tokio::task::spawn_blocking(move || {
                            noar_engine::render_graphic(
                                &spec,
                                config.output.width,
                                config.output.height,
                            )
                        })
                        .await
                        .map_err(|_| ServiceError::InternalServerError)?
                        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                        manager
                            .graphics_control
                            .set_always_on(noar_engine::AlwaysOnSlot::Ticker, graphic, true)
                            .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                    } else {
                        manager
                            .graphics_control
                            .set_always_on_visible(noar_engine::AlwaysOnSlot::Ticker, true);
                    }

                    Visibility::Ticker(previous)
                }
                _ => {
                    if manager
                        .graphics_control
                        .persistent_visible(template.id as u64)
                        .is_none()
                    {
                        crate::graphics::load_permanent_templates(manager).await?;
                    }

                    let previous = manager
                        .graphics_control
                        .persistent_visible(template.id as u64)
                        .ok_or_else(|| {
                            ServiceError::Conflict(
                                "Scheduled permanent model is unavailable".to_string(),
                            )
                        })?;
                    manager
                        .graphics_control
                        .set_persistent_visible(template.id as u64, true);
                    Visibility::Permanent(previous)
                }
            };
            self.visibility.insert(template.id, previous);

            return Ok(Some(Activation::Permanent(template.id)));
        }

        if template.during_lshape == DuringLShape::Wait
            && !template
                .layers
                .iter()
                .any(|layer| layer.visible && layer.kind == crate::graphics::LayerKind::Video)
        {
            if manager.lshape_control.is_on() || manager.graphics_control.picture_on_air() {
                return Ok(None);
            }

            template.during_lshape = DuringLShape::Follow;
        }

        let take = take_template_locked(
            manager,
            template,
            selection.values.clone(),
            selection.records.clone(),
            Some(0.0),
        )
        .await?;

        match take {
            TakeOutcome::OnAir { take_id } => Ok(Some(Activation::Temporary(take_id))),
            TakeOutcome::Waiting => Err(ServiceError::Conflict(
                "Scheduled take unexpectedly entered the wait queue".to_string(),
            )),
        }
    }

    fn finish(&mut self, manager: &ChannelManager, activation: Activation, restore: bool) {
        match activation {
            Activation::Temporary(id) => {
                manager.graphics_control.take_out(id);
            }
            Activation::Permanent(id) => {
                if self.runs.values().any(
                    |run| matches!(run.activation, Activation::Permanent(other) if other == id),
                ) {
                    return;
                }

                match self.visibility.remove(&id) {
                    Some(Visibility::Logo) => {
                        if restore {
                            manager.logo_control.set_scheduled_visible(false);
                        } else {
                            manager.logo_control.abandon_scheduled_visibility();
                        }
                    }
                    Some(Visibility::Ticker(Some(previous))) if restore => {
                        manager
                            .graphics_control
                            .set_always_on_visible(noar_engine::AlwaysOnSlot::Ticker, previous);
                    }
                    Some(Visibility::Ticker(None)) => manager
                        .graphics_control
                        .clear_always_on(noar_engine::AlwaysOnSlot::Ticker),
                    Some(Visibility::Permanent(previous)) if restore => {
                        manager
                            .graphics_control
                            .set_persistent_visible(id as u64, previous);
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn stop(&mut self, manager: &ChannelManager) {
        let runs = std::mem::take(&mut self.runs);

        for run in runs.into_values() {
            self.finish(manager, run.activation, true);
        }
    }
}
