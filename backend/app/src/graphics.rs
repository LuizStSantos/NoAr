//! Application facade for Graphics models and their runtime lifecycle.
//!
//! `templates` owns the JSON contract, validation and engine-spec conversion;
//! `channel` projects bound roles and visibility; `control` applies external and
//! scene actions. Assets and live data belong to `fonts` and `sources`.
//!
//! This facade owns take identities, edit generations and dynamic refresh so
//! timed events and HTTP adapters share one lifecycle without owning each other.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex as StdMutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::Serialize;

#[path = "graphics/channel/graphics.rs"]
mod channel;
#[path = "graphics/commands/graphics.rs"]
pub mod commands;
#[path = "graphics/fonts/graphics.rs"]
pub mod fonts;
#[path = "graphics/sources/graphics.rs"]
pub mod sources;
#[path = "graphics/templates/graphics.rs"]
mod templates;

use crate::{
    db::models::GraphicsTemplate, player::controller::ChannelManager, utils::errors::ServiceError,
};

pub(crate) use channel::graphic_has_content;
pub use channel::{
    ChannelGraphicView, channel_graphics, clear_removed_channel_graphic, control_channel_graphic,
    load_channel_ticker,
};
use templates::whole_placeholder;
pub use templates::{
    Animation, DuringLShape, LayerKind, LayerStyle, MAX_LAYERS, ShapeKind, TemplateInput,
    TemplateLayer, TextAlign, clock_value, fill_fields, graphic_spec, graphic_spec_for_channel,
    graphic_spec_for_channel_controlled, graphic_spec_for_records, prepare_template,
    template_fields, validate_take_fields,
};

/// Which template each graphic on air came from.
#[derive(Debug, Clone, Default)]
pub struct GraphicsTakes {
    records: Arc<StdMutex<Vec<TakeRecord>>>,
    version: Arc<AtomicU64>,
    editing: Arc<tokio::sync::Mutex<()>>,
    pending: Arc<StdMutex<HashMap<u64, tokio_util::sync::CancellationToken>>>,
    next_pending: Arc<AtomicU64>,
    generation_gate: Arc<StdMutex<()>>,
    /// The latest take/off command per channel graphic, so a duration timer
    /// left by an earlier command does nothing.
    channel_timers: Arc<StdMutex<HashMap<i32, u64>>>,
    next_channel_timer: Arc<AtomicU64>,
}

pub struct GraphicsLifecycleGuard {
    takes: GraphicsTakes,
}

impl Drop for GraphicsLifecycleGuard {
    fn drop(&mut self) {
        self.takes.cancel_pending();
    }
}

struct PendingTake {
    takes: GraphicsTakes,
    id: u64,
    cancel: tokio_util::sync::CancellationToken,
}

impl Drop for PendingTake {
    fn drop(&mut self) {
        self.takes
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

pub struct GraphicsEditGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
    version: Arc<AtomicU64>,
}

impl Drop for GraphicsEditGuard {
    fn drop(&mut self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TakeRecord {
    pub take_id: u64,
    pub template_id: i32,
    pub name: String,
    #[serde(skip)]
    pub values: HashMap<String, String>,
    #[serde(skip)]
    pub records: HashMap<i32, usize>,
}

impl GraphicsTakes {
    /// Start a new command generation for a channel graphic; any timer of an
    /// earlier command for it no longer applies.
    pub fn arm_channel_timer(&self, template_id: i32) -> u64 {
        let generation = self.next_channel_timer.fetch_add(1, Ordering::SeqCst) + 1;

        self.channel_timers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(template_id, generation);

        generation
    }

    pub fn channel_timer_is(&self, template_id: i32, generation: u64) -> bool {
        self.channel_timers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&template_id)
            == Some(&generation)
    }

    pub async fn begin_edit(&self) -> GraphicsEditGuard {
        let guard = self.editing.clone().lock_owned().await;
        self.cancel_pending();
        self.version.fetch_add(1, Ordering::SeqCst);

        GraphicsEditGuard {
            _guard: guard,
            version: self.version.clone(),
        }
    }

    pub fn invalidate(&self) {
        self.cancel_pending();
    }

    /// Cancel queued commands whenever output or its authored models change.
    pub fn cancel_pending(&self) {
        let _publication = self
            .generation_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.version.fetch_add(2, Ordering::SeqCst);

        for cancel in self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            cancel.cancel();
        }
    }

    pub fn lifecycle_guard(&self) -> GraphicsLifecycleGuard {
        self.cancel_pending();

        GraphicsLifecycleGuard {
            takes: self.clone(),
        }
    }

    pub(crate) async fn lock_publication(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.editing.clone().lock_owned().await
    }

    /// Lifecycle callbacks cannot await the edit lock. Keep this guard only
    /// across final state publication, after pixel preparation has finished.
    pub(crate) fn guard_if_current(&self, version: u64) -> Option<std::sync::MutexGuard<'_, ()>> {
        let publication = self
            .generation_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        (self.version.load(Ordering::SeqCst) == version).then_some(publication)
    }

    fn publish_if_current<T>(&self, version: u64, publish: impl FnOnce() -> T) -> Option<T> {
        let _publication = self.guard_if_current(version)?;

        Some(publish())
    }

    fn queue(&self, version: u64) -> Result<PendingTake, ServiceError> {
        let _publication = self
            .generation_gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        if self.version.load(Ordering::SeqCst) != version {
            return Err(ServiceError::Conflict(
                "The graphics changed while preparing this take".to_string(),
            ));
        }

        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);

        if pending.len() >= MAX_PENDING_TAKES {
            return Err(ServiceError::Conflict(
                "Too many graphics are waiting for the L-shape".to_string(),
            ));
        }

        let id = self.next_pending.fetch_add(1, Ordering::Relaxed);
        let cancel = tokio_util::sync::CancellationToken::new();
        pending.insert(id, cancel.clone());

        Ok(PendingTake {
            takes: self.clone(),
            id,
            cancel,
        })
    }

    fn records(&self) -> std::sync::MutexGuard<'_, Vec<TakeRecord>> {
        self.records.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn add(&self, record: TakeRecord) {
        self.records().push(record);
    }

    /// The record of each id still on air; the others are forgotten.
    fn keep_only(&self, on_air: &[u64]) -> Vec<TakeRecord> {
        let mut records = self.records();

        records.retain(|record| on_air.contains(&record.take_id));
        records.clone()
    }
}

/// A graphic on air, as answered by the API.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GraphicOnAirView {
    pub take_id: u64,
    pub template_id: i32,
    pub name: String,
    /// Seconds since it was taken.
    pub since: f64,
    pub leaving: bool,
}

pub fn graphics_on_air(manager: &ChannelManager) -> Vec<GraphicOnAirView> {
    let on_air = manager.graphics_control.on_air();
    let ids: Vec<u64> = on_air.iter().map(|graphic| graphic.id).collect();
    let records = manager.graphics_takes.keep_only(&ids);

    on_air
        .into_iter()
        .filter_map(|graphic| {
            let record = records.iter().find(|record| record.take_id == graphic.id)?;

            Some(GraphicOnAirView {
                take_id: graphic.id,
                template_id: record.template_id,
                name: record.name.clone(),
                since: graphic.since.as_secs_f64(),
                leaving: graphic.leaving,
            })
        })
        .collect()
}

/// What happened to a take.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TakeOutcome {
    OnAir {
        take_id: u64,
    },
    /// It goes on air when the L-shape ends.
    Waiting,
}

const MAX_PENDING_TAKES: usize = 16;
/// How long a take may wait for an L-shape to end.
const MAX_LSHAPE_WAIT: Duration = Duration::from_secs(3600);
const LSHAPE_WAIT_STEP: Duration = Duration::from_millis(200);

async fn draw_and_take(
    manager: &ChannelManager,
    template: &GraphicsTemplate,
    spec: noar_engine::GraphicSpec,
    duration: Option<Duration>,
    values: HashMap<String, String>,
    records: HashMap<i32, usize>,
    version: u64,
) -> Result<u64, ServiceError> {
    let (width, height) = {
        let config = manager.config.read().await;
        manager
            .running_config()
            .as_deref()
            .map_or((config.output.width, config.output.height), |running| {
                (running.output.width, running.output.height)
            })
    };
    let graphic =
        tokio::task::spawn_blocking(move || noar_engine::render_graphic(&spec, width, height))
            .await
            .map_err(|_| ServiceError::InternalServerError)?
            .map_err(|error| {
                ServiceError::BadRequest(format!(
                    "the graphic cannot be drawn ({})",
                    error.root_cause()
                ))
            })?;
    manager
        .graphics_takes
        .publish_if_current(version, || {
            let take_id = manager.graphics_control.take(graphic, duration);
            manager.graphics_takes.add(TakeRecord {
                take_id,
                template_id: template.id,
                name: template.name.clone(),
                values,
                records,
            });

            take_id
        })
        .ok_or_else(|| {
            ServiceError::Conflict("The graphics changed while preparing this take".to_string())
        })
}

/// Put `template` on air with `values` in its fields, following its rule
/// for an L-shape on air. `duration` replaces the template's own.
pub async fn take_template(
    manager: &ChannelManager,
    template: GraphicsTemplate,
    values: HashMap<String, String>,
    records: HashMap<i32, usize>,
    duration: Option<f64>,
) -> Result<TakeOutcome, ServiceError> {
    let _publication = manager.graphics_takes.lock_publication().await;
    take_template_prepared(manager, template, values, records, duration, false).await
}

/// The caller must hold the channel's graphics publication or edit lock.
pub(crate) async fn take_template_locked(
    manager: &ChannelManager,
    template: GraphicsTemplate,
    values: HashMap<String, String>,
    records: HashMap<i32, usize>,
    duration: Option<f64>,
) -> Result<TakeOutcome, ServiceError> {
    take_template_prepared(manager, template, values, records, duration, true).await
}

async fn take_template_prepared(
    manager: &ChannelManager,
    template: GraphicsTemplate,
    values: HashMap<String, String>,
    records: HashMap<i32, usize>,
    duration: Option<f64>,
    scheduled: bool,
) -> Result<TakeOutcome, ServiceError> {
    validate_take_fields(&values, &records)?;
    let version = manager.graphics_takes.version.load(Ordering::SeqCst);
    let mut saved =
        crate::db::handles::select_graphics_template(&manager.db_pool, manager.id, template.id)
            .await?
            .ok_or_else(|| {
                ServiceError::NotFound("Graphics template not found in this channel".to_string())
            })?;
    // Scheduled takes may replace Wait with Follow after checking their own end.
    if scheduled
        && saved.during_lshape == DuringLShape::Wait
        && template.during_lshape == DuringLShape::Follow
    {
        saved.during_lshape = DuringLShape::Follow;
    }

    if saved != template {
        return Err(ServiceError::Conflict(
            "The graphics template changed; take it again".to_string(),
        ));
    }

    let seconds = duration.unwrap_or(template.duration);

    if !seconds.is_finite() || !(0.0..=86_400.0).contains(&seconds) {
        return Err(ServiceError::BadRequest(
            "duration is in seconds, 0 (until taken out) up to a day".to_string(),
        ));
    }

    let duration = (seconds > 0.0).then(|| Duration::from_secs_f64(seconds));
    let config = manager.config.read().await.clone();
    let spec = graphic_spec_for_records(&template, &values, &records, &config)?;

    let lshape_on = |manager: &ChannelManager| {
        manager.lshape_control.is_on() || manager.graphics_control.picture_on_air()
    };

    // An L-shape template replaces the L-shape on air; the rule is for the
    // graphics that go over the picture.
    if spec.picture.is_none() && lshape_on(manager) {
        match template.during_lshape {
            DuringLShape::Follow => {}
            DuringLShape::Skip => {
                return Err(ServiceError::Conflict(format!(
                    "'{}' does not go on air during an L-shape",
                    template.name
                )));
            }
            DuringLShape::Wait => {
                let pending = manager.graphics_takes.queue(version)?;
                let manager = manager.clone();

                tokio::spawn(async move {
                    let _pending = pending;
                    let deadline = tokio::time::Instant::now() + MAX_LSHAPE_WAIT;

                    while lshape_on(&manager) {
                        tokio::select! {
                            _ = _pending.cancel.cancelled() => return,
                            _ = tokio::time::sleep_until(deadline) => return,
                            _ = tokio::time::sleep(LSHAPE_WAIT_STEP) => {}
                        }
                    }

                    let _publication = tokio::select! {
                        _ = _pending.cancel.cancelled() => return,
                        guard = manager.graphics_takes.lock_publication() => guard,
                    };

                    if _pending.cancel.is_cancelled()
                        || manager.graphics_takes.version.load(Ordering::SeqCst) != version
                    {
                        return;
                    }

                    if let Err(error) = draw_and_take(
                        &manager, &template, spec, duration, values, records, version,
                    )
                    .await
                    {
                        log::warn!(
                            "Graphic '{}' not taken after the L-shape: {error}",
                            template.name
                        );
                    }
                });

                return Ok(TakeOutcome::Waiting);
            }
        }
    }

    let take_id =
        draw_and_take(manager, &template, spec, duration, values, records, version).await?;

    Ok(TakeOutcome::OnAir { take_id })
}

/// Restore user-authored permanent compositions independently of compatibility roles.
pub async fn load_permanent_templates(manager: &ChannelManager) -> Result<(), ServiceError> {
    let version = manager.graphics_takes.version.load(Ordering::SeqCst);
    load_permanent_templates_at_version(manager, version).await
}

async fn load_permanent_templates_at_version(
    manager: &ChannelManager,
    version: u64,
) -> Result<(), ServiceError> {
    let bindings =
        crate::db::handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let templates =
        crate::db::handles::select_graphics_templates(&manager.db_pool, manager.id).await?;
    let config = manager.config.read().await.clone();
    let mut ids = Vec::new();

    for template in templates.into_iter().filter(|template| {
        template.always_on == Some(true)
            && !bindings
                .iter()
                .any(|binding| binding.template_id == template.id)
    }) {
        let id = template.id as u64;
        ids.push(id);
        let prepared: Result<(), ServiceError> = async {
            let spec = graphic_spec_for_channel(&template, &HashMap::new(), &config)?;
            let running = manager.running_config();
            let (width, height) = running
                .as_deref()
                .map_or((config.output.width, config.output.height), |running| {
                    (running.output.width, running.output.height)
                });
            let graphic = tokio::task::spawn_blocking(move || {
                noar_engine::render_graphic(&spec, width, height)
            })
            .await
            .map_err(|_| ServiceError::InternalServerError)?
            .map_err(|error| {
                ServiceError::Conflict(format!("Permanent template cannot be rendered: {error:#}"))
            })?;
            if let Some(result) = manager.graphics_takes.publish_if_current(version, || {
                manager.graphics_control.set_persistent(id, graphic)
            }) {
                result.map_err(|error| ServiceError::Conflict(error.to_string()))?;
            }

            Ok(())
        }
        .await;

        if let Err(error) = prepared {
            log::warn!(channel=manager.id; "Permanent graphic '{}' kept its last frame or was skipped: {error}", template.name);
        }
    }

    let _ = manager
        .graphics_takes
        .publish_if_current(version, || manager.graphics_control.retain_persistent(&ids));

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum DynamicScope {
    Role,
    Permanent,
    Take,
}

/// Dynamic values refresh off the frame thread and retain take/animation identity.
pub async fn run_dynamic_graphics(
    manager: ChannelManager,
    cancel: tokio_util::sync::CancellationToken,
) {
    let mut rendered = HashMap::<(DynamicScope, u64), noar_engine::GraphicSpec>::new();
    let mut last_warning = std::time::Instant::now() - Duration::from_secs(30);

    loop {
        tokio::select! {_=cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(250))=>{}}

        if let Err(error) = refresh_dynamic_graphics(&manager, &mut rendered).await
            && last_warning.elapsed() >= Duration::from_secs(30)
        {
            log::warn!(channel=manager.id; "Dynamic graphics kept their last frame: {error}");
            last_warning = std::time::Instant::now();
        }
    }
}

async fn refresh_dynamic_graphics(
    manager: &ChannelManager,
    rendered: &mut HashMap<(DynamicScope, u64), noar_engine::GraphicSpec>,
) -> Result<(), ServiceError> {
    let version = manager.graphics_takes.version.load(Ordering::SeqCst);

    if !version.is_multiple_of(2) {
        return Ok(());
    }

    let templates =
        crate::db::handles::select_graphics_templates(&manager.db_pool, manager.id).await?;
    let bindings =
        crate::db::handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let config = manager.config.read().await.clone();
    let running = manager.running_config();
    let (width, height) = running
        .as_deref()
        .map_or((config.output.width, config.output.height), |running| {
            (running.output.width, running.output.height)
        });
    let active = graphics_on_air(manager);
    let takes = manager.graphics_takes.records().clone();
    let mut retained = Vec::new();
    let mut warning = None;

    for template in templates.iter().filter(|template| {
        template.layers.iter().any(|layer| {
            layer.visible && (layer.kind == LayerKind::Clock || !layer.style.data_fields.is_empty())
        })
    }) {
        let binding = bindings
            .iter()
            .find(|binding| binding.template_id == template.id);
        let mut targets = Vec::new();

        if let Some(binding) = binding {
            targets.push((
                DynamicScope::Role,
                binding.template_id as u64,
                HashMap::new(),
            ));
        } else if template.always_on == Some(true) {
            targets.push((DynamicScope::Permanent, template.id as u64, HashMap::new()));
        }

        for take in active.iter().filter(|take| take.template_id == template.id) {
            targets.push((
                DynamicScope::Take,
                take.take_id,
                takes
                    .iter()
                    .find(|record| record.take_id == take.take_id)
                    .map(|record| record.values.clone())
                    .unwrap_or_default(),
            ));
        }

        for (kind, id, values) in targets {
            let key = (kind, id);
            retained.push(key);
            let records = if kind == DynamicScope::Take {
                takes
                    .iter()
                    .find(|record| record.take_id == id)
                    .map(|take| take.records.clone())
                    .unwrap_or_default()
            } else {
                HashMap::new()
            };
            let spec = match graphic_spec_for_records(template, &values, &records, &config) {
                Ok(spec) => spec,
                Err(error) => {
                    warning = Some(error);
                    continue;
                }
            };

            if rendered.get(&key) == Some(&spec) {
                continue;
            }

            // A bad image or feed must not starve clocks in other compositions.
            let published: Result<bool, ServiceError> = async {
                let pixels = if kind == DynamicScope::Role {
                    None
                } else {
                    let draw = spec.clone();
                    Some(
                        tokio::task::spawn_blocking(move || {
                            noar_engine::render_graphic(&draw, width, height)
                        })
                        .await
                        .map_err(|_| ServiceError::InternalServerError)?
                        .map_err(|error| ServiceError::Conflict(error.to_string()))?,
                    )
                };
                let _editing = manager.graphics_takes.editing.clone().lock_owned().await;

                if manager.graphics_takes.version.load(Ordering::SeqCst) != version {
                    return Ok(false);
                }

                // Enabling a saved role changes its persisted visibility without
                // cancelling waiting takes. Recheck that snapshot after locking.
                if kind == DynamicScope::Role
                    && crate::db::handles::select_graphics_template(
                        &manager.db_pool,
                        manager.id,
                        template.id,
                    )
                    .await?
                    .as_ref()
                        != Some(template)
                {
                    return Ok(false);
                }

                match kind {
                    DynamicScope::Role => match binding.map(|binding| binding.role) {
                        Some(crate::db::models::GraphicsRole::Logo) => {
                            let mut updated = config.clone();
                            updated.logo_model = Some(template.clone());
                            let manager = manager.clone();
                            tokio::task::spawn_blocking(move || {
                                crate::player::output::refresh_logo_guarded(
                                    &manager,
                                    &updated,
                                    width,
                                    height,
                                    || manager.graphics_takes.guard_if_current(version),
                                )
                            })
                            .await
                            .map_err(|_| ServiceError::InternalServerError)??;
                        }
                        Some(crate::db::models::GraphicsRole::Lshape) => {
                            let control = manager.lshape_control.clone();
                            let takes = manager.graphics_takes.clone();
                            let (draw, primary) =
                                graphic_spec_for_channel_controlled(template, &values, &config)?;
                            tokio::task::spawn_blocking(move || {
                                control.set_template_controlled_guarded(
                                    &draw,
                                    primary,
                                    (width, height),
                                    || takes.guard_if_current(version),
                                )
                            })
                            .await
                            .map_err(|_| ServiceError::InternalServerError)?
                            .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                        }
                        Some(crate::db::models::GraphicsRole::Ticker) => {
                            let spec = spec.clone();
                            let graphic = tokio::task::spawn_blocking(move || {
                                noar_engine::render_graphic(&spec, width, height)
                            })
                            .await
                            .map_err(|_| ServiceError::InternalServerError)?
                            .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                            let Some(result) =
                                manager.graphics_takes.publish_if_current(version, || {
                                    manager.graphics_control.refresh_always_on(
                                        noar_engine::AlwaysOnSlot::Ticker,
                                        graphic,
                                    )
                                })
                            else {
                                return Ok(false);
                            };
                            result.map_err(|error| ServiceError::Conflict(error.to_string()))?;
                        }
                        None => {}
                    },
                    DynamicScope::Permanent => {
                        let Some(result) =
                            manager.graphics_takes.publish_if_current(version, || {
                                manager.graphics_control.set_persistent(
                                    id,
                                    pixels.expect("pixels rendered for a persistent template"),
                                )
                            })
                        else {
                            return Ok(false);
                        };
                        result.map_err(|error| ServiceError::Conflict(error.to_string()))?;
                    }
                    DynamicScope::Take => {
                        if manager
                            .graphics_takes
                            .publish_if_current(version, || {
                                manager.graphics_control.update_graphic(
                                    id,
                                    pixels.expect("pixels rendered for an active take"),
                                )
                            })
                            .is_none()
                        {
                            return Ok(false);
                        }
                    }
                }

                Ok(manager.graphics_takes.version.load(Ordering::SeqCst) == version)
            }
            .await;

            match published {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(error) => {
                    warning = Some(error);
                    continue;
                }
            }

            rendered.insert(key, spec);
        }
    }

    rendered.retain(|key, _| retained.contains(key));

    warning.map_or(Ok(()), Err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_takes_are_bounded_and_output_lifecycle_cancels_them() {
        let takes = GraphicsTakes::default();
        let before_start = takes.queue(takes.version.load(Ordering::SeqCst)).unwrap();
        let lifecycle = takes.lifecycle_guard();
        assert!(before_start.cancel.is_cancelled());
        drop(before_start);
        let pending: Vec<_> = (0..MAX_PENDING_TAKES)
            .map(|_| takes.queue(takes.version.load(Ordering::SeqCst)).unwrap())
            .collect();
        assert!(matches!(
            takes.queue(takes.version.load(Ordering::SeqCst)),
            Err(ServiceError::Conflict(_))
        ));
        drop(lifecycle);
        assert!(pending.iter().all(|take| take.cancel.is_cancelled()));
        // Cancelled work keeps its admission slot until its task actually ends.
        assert!(takes.queue(takes.version.load(Ordering::SeqCst)).is_err());
        drop(pending);
        assert!(takes.queue(takes.version.load(Ordering::SeqCst)).is_ok());
    }

    #[tokio::test]
    async fn editing_waits_for_publication_and_cancels_queued_takes() {
        let takes = GraphicsTakes::default();
        let pending = takes.queue(takes.version.load(Ordering::SeqCst)).unwrap();
        let publication = takes.lock_publication().await;
        let editing = takes.begin_edit();
        tokio::pin!(editing);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), &mut editing)
                .await
                .is_err()
        );
        assert!(!pending.cancel.is_cancelled());
        drop(publication);
        let edit = editing.await;
        assert!(pending.cancel.is_cancelled());
        assert!(!takes.version.load(Ordering::SeqCst).is_multiple_of(2));
        drop(edit);
        assert!(takes.version.load(Ordering::SeqCst).is_multiple_of(2));
    }

    #[test]
    fn synchronous_cancellation_cannot_interleave_checked_publication() {
        use std::sync::{Barrier, mpsc};

        let takes = GraphicsTakes::default();
        let version = takes.version.load(Ordering::SeqCst);
        let control = noar_engine::GraphicsControl::default();
        let graphic =
            noar_engine::render_graphic(&noar_engine::GraphicSpec::default(), 16, 16).unwrap();
        let checked = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_takes = takes.clone();
        let worker_control = control.clone();
        let worker_checked = checked.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            worker_takes.publish_if_current(version, || {
                // Pause after checking the generation, exactly before engine publication.
                worker_checked.wait();
                worker_release.wait();
                let take_id = worker_control.take(graphic, None);
                worker_takes.add(TakeRecord {
                    take_id,
                    template_id: 1,
                    name: "Serialized take".to_string(),
                    values: HashMap::new(),
                    records: HashMap::new(),
                });

                take_id
            })
        });
        checked.wait();
        let (started_tx, started_rx) = mpsc::channel();
        let (cancelled_tx, cancelled_rx) = mpsc::channel();
        let canceller_takes = takes.clone();
        let canceller = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            canceller_takes.cancel_pending();
            cancelled_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        let cancellation_blocked = cancelled_rx
            .recv_timeout(Duration::from_millis(20))
            .is_err();
        release.wait();
        let take_id = worker.join().unwrap().unwrap();
        if cancellation_blocked {
            cancelled_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        }

        canceller.join().unwrap();
        assert!(
            cancellation_blocked,
            "cancellation must wait for the checked take and its record"
        );
        assert_eq!(control.on_air()[0].id, take_id);
        assert_eq!(takes.records()[0].take_id, take_id);
        assert!(
            takes
                .publish_if_current(version, || panic!("stale publication executed"))
                .is_none()
        );
        assert!(
            takes.queue(version).is_err(),
            "a cancellation cannot miss stale queue admission"
        );
    }

    #[test]
    fn cancelled_preparations_cannot_publish_ticker_permanent_or_temporary_pixels() {
        use std::sync::Barrier;

        for scope in [
            DynamicScope::Role,
            DynamicScope::Permanent,
            DynamicScope::Take,
        ] {
            let takes = GraphicsTakes::default();
            let version = takes.version.load(Ordering::SeqCst);
            let control = noar_engine::GraphicsControl::default();
            let prepared = Arc::new(Barrier::new(2));
            let release = Arc::new(Barrier::new(2));
            let worker_takes = takes.clone();
            let worker_control = control.clone();
            let worker_prepared = prepared.clone();
            let worker_release = release.clone();
            let worker = std::thread::spawn(move || {
                let graphic =
                    noar_engine::render_graphic(&noar_engine::GraphicSpec::default(), 16, 16)
                        .unwrap();
                worker_prepared.wait();
                worker_release.wait();
                worker_takes.publish_if_current(version, || match scope {
                    DynamicScope::Role => worker_control
                        .set_always_on(noar_engine::AlwaysOnSlot::Ticker, graphic, true)
                        .unwrap(),
                    DynamicScope::Permanent => {
                        worker_control.set_persistent(1, graphic).unwrap();
                        worker_control.retain_persistent(&[1]);
                    }
                    DynamicScope::Take => {
                        worker_control.take(graphic, None);
                    }
                })
            });
            prepared.wait();
            takes.cancel_pending();
            let next =
                noar_engine::render_graphic(&noar_engine::GraphicSpec::default(), 16, 16).unwrap();
            control.set_persistent(99, next).unwrap();
            release.wait();
            assert!(
                worker.join().unwrap().is_none(),
                "old pixels survived {scope:?} cancellation"
            );
            assert_eq!(
                control.always_on_visible(noar_engine::AlwaysOnSlot::Ticker),
                None
            );
            assert_eq!(control.persistent_visible(1), None);
            assert_eq!(
                control.persistent_visible(99),
                Some(true),
                "stale retain must not remove the new startup model"
            );
            assert!(control.on_air().is_empty());
        }
    }

    #[test]
    fn expanded_text_is_bounded_across_layers_before_rendering() {
        let layer: TemplateLayer = serde_json::from_value(serde_json::json!({
            "id": "text", "kind": "text", "x": 0, "y": 0, "width": 100,
            "height": 10, "text": "{x}".repeat(200),
        }))
        .unwrap();
        let mut template = GraphicsTemplate {
            layers: vec![layer],
            ..Default::default()
        };
        let values = HashMap::from([("x".to_string(), "a".repeat(1000))]);
        assert!(graphic_spec(&template, &values, &std::env::temp_dir()).is_ok());
        template.layers.push(template.layers[0].clone());
        assert!(matches!(
            graphic_spec(&template, &values, &std::env::temp_dir()),
            Err(ServiceError::BadRequest(_))
        ));
    }
}
