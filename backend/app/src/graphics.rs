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
#[path = "graphics/control/graphics.rs"]
pub mod control;
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
    graphic_spec_for_records, prepare_template, template_fields,
};

/// Which template each graphic on air came from.
#[derive(Debug, Clone, Default)]
pub struct GraphicsTakes {
    records: Arc<StdMutex<Vec<TakeRecord>>>,
    version: Arc<AtomicU64>,
    editing: Arc<tokio::sync::Mutex<()>>,
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
    pub async fn begin_edit(&self) -> GraphicsEditGuard {
        let guard = self.editing.clone().lock_owned().await;
        self.version.fetch_add(1, Ordering::SeqCst);

        GraphicsEditGuard {
            _guard: guard,
            version: self.version.clone(),
        }
    }

    pub fn invalidate(&self) {
        self.version.fetch_add(2, Ordering::SeqCst);
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
) -> Result<u64, ServiceError> {
    let (width, height) = {
        let config = manager.config.read().await;
        (config.output.width, config.output.height)
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
    let take_id = manager.graphics_control.take(graphic, duration);

    manager.graphics_takes.add(TakeRecord {
        take_id,
        template_id: template.id,
        name: template.name.clone(),
        values,
        records,
    });

    Ok(take_id)
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
                let manager = manager.clone();

                tokio::spawn(async move {
                    let mut waited = Duration::ZERO;

                    while lshape_on(&manager) && waited < MAX_LSHAPE_WAIT {
                        tokio::time::sleep(LSHAPE_WAIT_STEP).await;
                        waited += LSHAPE_WAIT_STEP;
                    }

                    if waited < MAX_LSHAPE_WAIT
                        && let Err(error) = draw_and_take(
                            &manager,
                            &template,
                            spec,
                            duration,
                            values.clone(),
                            records.clone(),
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

    let take_id = draw_and_take(manager, &template, spec, duration, values, records).await?;

    Ok(TakeOutcome::OnAir { take_id })
}

/// Restore user-authored permanent compositions independently of compatibility roles.
pub async fn load_permanent_templates(manager: &ChannelManager) -> Result<(), ServiceError> {
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
        let spec = graphic_spec_for_channel(&template, &HashMap::new(), &config)?;
        let running = manager.running_config();
        let (width, height) = running
            .as_deref()
            .map_or((config.output.width, config.output.height), |running| {
                (running.output.width, running.output.height)
            });
        let graphic =
            tokio::task::spawn_blocking(move || noar_engine::render_graphic(&spec, width, height))
                .await
                .map_err(|_| ServiceError::InternalServerError)?
                .map_err(|error| {
                    ServiceError::Conflict(format!(
                        "Permanent template cannot be rendered: {error:#}"
                    ))
                })?;
        manager
            .graphics_control
            .set_persistent(id, graphic)
            .map_err(|error| ServiceError::Conflict(error.to_string()))?;
        ids.push(id);
    }

    manager.graphics_control.retain_persistent(&ids);

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

                match kind {
                    DynamicScope::Role => match binding.map(|binding| binding.role) {
                        Some(crate::db::models::GraphicsRole::Logo) => {
                            let mut updated = config.clone();
                            updated.logo_model = Some(template.clone());
                            let manager = manager.clone();
                            tokio::task::spawn_blocking(move || {
                                crate::player::output::refresh_logo(
                                    &manager, &updated, width, height,
                                )
                            })
                            .await
                            .map_err(|_| ServiceError::InternalServerError)??;
                        }
                        Some(crate::db::models::GraphicsRole::Lshape) => {
                            let control = manager.lshape_control.clone();
                            let primary = template
                                .layers
                                .iter()
                                .filter(|layer| layer.visible)
                                .position(|layer| {
                                    Some(layer.id.as_str()) == template.control_layer_id.as_deref()
                                })
                                .unwrap_or(usize::MAX);
                            let draw = spec.clone();
                            tokio::task::spawn_blocking(move || {
                                control.set_template_controlled(&draw, primary, width, height)
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
                            manager
                                .graphics_control
                                .refresh_always_on(noar_engine::AlwaysOnSlot::Ticker, graphic)
                                .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                        }
                        None => {}
                    },
                    DynamicScope::Permanent => {
                        manager
                            .graphics_control
                            .set_persistent(
                                id,
                                pixels.expect("pixels rendered for a persistent template"),
                            )
                            .map_err(|error| ServiceError::Conflict(error.to_string()))?;
                    }
                    DynamicScope::Take => {
                        manager.graphics_control.update_graphic(
                            id,
                            pixels.expect("pixels rendered for an active take"),
                        );
                    }
                }

                Ok(true)
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
