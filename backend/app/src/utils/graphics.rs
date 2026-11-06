//! Graphics templates: what a lower third, a topic bar or a sponsor bug looks
//! like, designed once and put on air many times. A template is a list of
//! layers placed in percent of the frame; `{field}` in a text, or as a whole
//! image path, is filled in each time the template goes on air.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex as StdMutex, PoisonError},
    time::Duration,
};

use serde::{Deserialize, Serialize};

use crate::{
    db::models::GraphicsTemplate,
    file::norm_abs_path,
    player::controller::ChannelManager,
    utils::{errors::ServiceError, xmltv::library::has_image_extension},
};

/// Whether a saved model has visible artwork or text, not just a picture window.
pub(crate) fn graphic_has_content(template: &GraphicsTemplate, storage: &std::path::Path) -> bool {
    template.layers.iter().any(|layer| {
        layer.visible
            && match layer.kind {
                LayerKind::Image => layer.image.as_deref().is_some_and(|image| {
                    !image.is_empty()
                        && norm_abs_path(storage, image).is_ok_and(|(path, _, _)| path.is_file())
                }),
                LayerKind::Text => !layer.text.trim().is_empty(),
                LayerKind::Box => layer.fill.is_some(),
                LayerKind::Video => false,
            }
    })
}

/// A channel model and its current control state, separate from temporary GCs.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelGraphicView {
    pub kind: String,
    pub template: GraphicsTemplate,
    pub on: bool,
    pub ready: bool,
    pub scene_name: Option<String>,
}

pub async fn channel_graphics(
    manager: &ChannelManager,
) -> Result<Vec<ChannelGraphicView>, ServiceError> {
    use crate::db::{handles, models::GraphicsRole};
    use crate::utils::config::{SceneLShape, SceneLogo};

    let templates = handles::select_graphics_templates(&manager.db_pool, manager.id).await?;
    let bindings = handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let scenes = handles::select_graphics_scene_templates(&manager.db_pool, manager.id).await?;
    let storage = manager.config.read().await.channel.storage.clone();
    let mut result = Vec::new();

    for binding in bindings {
        let template = templates
            .iter()
            .find(|template| template.id == binding.template_id)
            .ok_or_else(|| ServiceError::Conflict("Missing channel graphics model".to_string()))?
            .clone();
        let ready = graphic_has_content(&template, &storage);
        let on = match binding.role {
            GraphicsRole::Logo => manager.logo_control.is_visible(),
            GraphicsRole::Ticker => {
                binding.enabled
                    && manager
                        .graphics_control
                        .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
                        == Some(true)
            }
            GraphicsRole::Lshape => manager.lshape_control.is_on(),
        };
        result.push(ChannelGraphicView {
            kind: binding.role.as_str().to_string(),
            template,
            on,
            ready,
            scene_name: None,
        });
    }

    for (id, scene) in scenes {
        let mut template = templates
            .iter()
            .find(|template| template.id == id)
            .ok_or_else(|| ServiceError::Conflict("Missing scene model".to_string()))?
            .clone();
        let picture = scene.lshape == SceneLShape::On
            || (scene.lshape == SceneLShape::Keep && manager.lshape_control.is_on());
        template.layers = Vec::new();

        if picture && let Some(base) = result.iter().find(|view| view.kind == "lshape") {
            template.layers = base.template.layers.clone();

            if let Some(image) = scene.lshape_graphic.as_ref()
                && let Some(layer) = template
                    .layers
                    .iter_mut()
                    .find(|layer| layer.kind == LayerKind::Image)
            {
                layer.image = Some(image.clone());
            }
        }

        if !picture
            && scene.logo != SceneLogo::Hide
            && let Some(base) = result.iter().find(|view| view.kind == "logo")
            && (base.on || matches!(scene.logo, SceneLogo::Show | SceneLogo::Replace))
        {
            let mut layers = base.template.layers.clone();

            if scene.logo == SceneLogo::Replace
                && let Some(layer) = layers
                    .iter_mut()
                    .find(|layer| layer.kind == LayerKind::Image)
            {
                layer.image = scene.logo_image.clone();
            }

            template.layers.extend(layers);
        }

        result.push(ChannelGraphicView {
            kind: "scene".to_string(),
            template,
            on: false,
            ready: true,
            scene_name: Some(scene.name),
        });
    }

    Ok(result)
}

/// All controls share the handles used by clip marks and legacy HTTP routes.
pub async fn control_channel_graphic(
    manager: &ChannelManager,
    template_id: i32,
    on: bool,
) -> Result<(), ServiceError> {
    use crate::db::{handles, models::GraphicsRole};

    let bindings = handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;

    if let Some(binding) = bindings
        .iter()
        .find(|binding| binding.template_id == template_id)
    {
        let views = channel_graphics(manager).await?;
        let view = views
            .iter()
            .find(|view| view.template.id == template_id)
            .ok_or_else(|| ServiceError::NotFound("Channel model not found".to_string()))?;

        if on && !view.ready {
            return Err(ServiceError::Conflict(
                "Configure this channel model before putting it on air".to_string(),
            ));
        }

        if on && !binding.enabled && binding.role != GraphicsRole::Lshape {
            let mut enabled = binding.clone();
            enabled.enabled = true;
            handles::save_graphics_binding(&manager.db_pool, &enabled).await?;
            let config = crate::utils::config::get_config(&manager.db_pool, manager.id).await?;
            let running = manager.running_config();
            let (width, height) = running
                .as_deref()
                .map_or((config.output.width, config.output.height), |running| {
                    (running.output.width, running.output.height)
                });
            crate::player::output::load_logo(manager, &config, width, height);
            manager.update_config(config).await;
        }

        match binding.role {
            GraphicsRole::Logo => {
                if on {
                    let config = manager.config.read().await.clone();
                    let running = manager.running_config();
                    let (width, height) = running
                        .as_deref()
                        .map_or((config.output.width, config.output.height), |running| {
                            (running.output.width, running.output.height)
                        });
                    crate::player::output::load_logo(manager, &config, width, height);
                    manager.logo_control.show();
                } else {
                    manager.logo_control.hide();
                }
            }
            GraphicsRole::Ticker => {
                if on
                    && manager
                        .graphics_control
                        .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
                        .is_none()
                {
                    load_channel_ticker(manager).await?;
                }

                manager
                    .graphics_control
                    .set_always_on_visible(noar_engine::AlwaysOnSlot::Ticker, on);
            }
            GraphicsRole::Lshape => {
                if on && !manager.lshape_control.is_ready() {
                    let config = manager.config.read().await.clone();
                    crate::player::output::load_lshape(manager, &config).await;
                }

                if on && !manager.lshape_control.is_ready() {
                    return Err(ServiceError::Conflict(
                        "The L-shape artwork cannot be loaded".to_string(),
                    ));
                }

                if on {
                    manager.lshape_control.on();
                } else {
                    manager.lshape_control.off();
                }
            }
        }

        return Ok(());
    }

    let scenes = handles::select_graphics_scene_templates(&manager.db_pool, manager.id).await?;
    let (_, scene) = scenes
        .iter()
        .find(|(id, _)| *id == template_id)
        .ok_or_else(|| ServiceError::NotFound("Channel model not found".to_string()))?;
    if !on {
        return Err(ServiceError::BadRequest(
            "A scene is applied with on:true; use channel controls to change its result"
                .to_string(),
        ));
    }

    crate::api::routes::apply_graphics(manager, scene).await?;

    Ok(())
}

pub const MAX_LAYERS: usize = 64;

/// Restore the bound permanent text model. Released 2.2.1 has no persisted
/// ticker settings, so conversion creates an empty, disabled binding.
pub async fn load_channel_ticker(manager: &ChannelManager) -> Result<(), ServiceError> {
    let bindings =
        crate::db::handles::select_graphics_bindings(&manager.db_pool, manager.id).await?;
    let binding = bindings
        .iter()
        .find(|binding| binding.role == crate::db::models::GraphicsRole::Ticker)
        .ok_or_else(|| ServiceError::Conflict("Missing channel ticker binding".to_string()))?;

    if !binding.enabled {
        manager
            .graphics_control
            .clear_always_on(noar_engine::AlwaysOnSlot::Ticker);
        return Ok(());
    }

    let template = crate::db::handles::select_graphics_template(
        &manager.db_pool,
        manager.id,
        binding.template_id,
    )
    .await?
    .ok_or_else(|| ServiceError::Conflict("Missing channel ticker model".to_string()))?;
    let config = manager.config.read().await;
    let has_content = graphic_has_content(&template, &config.channel.storage);
    let spec = if has_content {
        graphic_spec(&template, &HashMap::new(), &config.channel.storage)?
    } else {
        // Preserve an explicit off state when text is cleared and later restored.
        noar_engine::GraphicSpec {
            layers: Vec::new(),
            picture: None,
        }
    };
    let (width, height) = (config.output.width, config.output.height);
    drop(config);
    let graphic =
        tokio::task::spawn_blocking(move || noar_engine::render_graphic(&spec, width, height))
            .await
            .map_err(|_| ServiceError::InternalServerError)?
            .map_err(|error| {
                ServiceError::Conflict(format!("Channel ticker cannot be rendered: {error:#}"))
            })?;
    let visible = has_content
        && manager
            .graphics_control
            .always_on_visible(noar_engine::AlwaysOnSlot::Ticker)
            .unwrap_or(true);
    manager
        .graphics_control
        .set_always_on(noar_engine::AlwaysOnSlot::Ticker, graphic, visible)
        .map_err(|error| ServiceError::Conflict(error.to_string()))?;
    Ok(())
}
const MAX_NAME_CHARS: usize = 100;
const MAX_TEXT_CHARS: usize = 1000;
const MAX_ANIMATION_MS: u32 = 10_000;
/// Small slack for percentages that came from pixel snapping.
const EDGE_SLACK: f64 = 0.001;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    #[default]
    Text,
    Image,
    Box,
    /// Where the picture goes: a template with one is an L-shape, its
    /// other layers the art around the picture.
    Video,
}

/// How a layer comes on and goes off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Animation {
    None,
    #[default]
    Fade,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
    Wipe,
    Scale,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What a template does when it is due while an L-shape is on air.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuringLShape {
    /// Drawn inside the shrunk picture, at the picture's scale.
    #[default]
    Follow,
    /// Goes on air when the L-shape ends.
    Wait,
    /// Does not go on air while the L-shape is on.
    Skip,
}

impl DuringLShape {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Follow => "follow",
            Self::Wait => "wait",
            Self::Skip => "skip",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "follow" => Some(Self::Follow),
            "wait" => Some(Self::Wait),
            "skip" => Some(Self::Skip),
            _ => None,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_opacity() -> f64 {
    1.0
}

fn default_font_size() -> f64 {
    4.0
}

fn default_color() -> String {
    "#ffffff".to_string()
}

fn default_animation_ms() -> u32 {
    400
}

fn default_duration() -> f64 {
    8.0
}

/// One layer of a template. Positions and sizes are percent of the frame;
/// the font size is percent of the frame height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateLayer {
    /// Stable within the template, so a renamed layer keeps its place.
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: LayerKind,
    #[serde(default = "default_true")]
    pub visible: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Text layers: what to write; `{field}` is filled in on air.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default)]
    pub bold: bool,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default)]
    pub align: TextAlign,
    /// Image layers: storage path, or a `{field}` chosen on air.
    #[serde(default)]
    pub image: Option<String>,
    /// Box layers, and the background behind a text.
    #[serde(default)]
    pub fill: Option<String>,
    /// Corner radius, percent of the frame height.
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub animation_in: Animation,
    #[serde(default)]
    pub animation_out: Animation,
    #[serde(default = "default_animation_ms")]
    pub animation_ms: u32,
    /// Wait before this layer comes on, to stagger the layers.
    #[serde(default)]
    pub delay_ms: u32,
}

/// A template as sent by the web interface or the API.
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateInput {
    pub name: String,
    #[serde(default)]
    pub group: String,
    #[serde(default = "default_duration")]
    pub duration: f64,
    #[serde(default)]
    pub during_lshape: DuringLShape,
    #[serde(default)]
    pub layers: Vec<TemplateLayer>,
}

/// The `{field}` names of a template, in the order they first appear.
pub fn template_fields(layers: &[TemplateLayer]) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();

    for layer in layers {
        let mut found = Vec::new();

        match layer.kind {
            LayerKind::Text => found.extend(placeholders(&layer.text)),
            LayerKind::Image => found.extend(layer.image.as_deref().and_then(whole_placeholder)),
            LayerKind::Box | LayerKind::Video => {}
        }

        for field in found {
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
    }

    fields
}

fn is_field_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

/// Every `{name}` in a text; braces around anything else stay as text.
fn placeholders(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;

    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];

        let Some(close) = after.find('}') else {
            break;
        };

        let name = &after[..close];

        if !name.is_empty() && name.chars().all(is_field_char) {
            found.push(name.to_string());
            rest = &after[close + 1..];
        } else {
            rest = after;
        }
    }

    found
}

/// The field name when the whole value is one `{name}`.
fn whole_placeholder(value: &str) -> Option<String> {
    let name = value.trim().strip_prefix('{')?.strip_suffix('}')?;

    (!name.is_empty() && name.chars().all(is_field_char)).then(|| name.to_string())
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`.
fn is_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };

    matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn check_layer(layer: &TemplateLayer) -> Result<(), String> {
    let label = if layer.name.trim().is_empty() {
        layer.id.trim().to_string()
    } else {
        layer.name.trim().to_string()
    };
    let in_frame = |value: f64| (-EDGE_SLACK..=100.0 + EDGE_SLACK).contains(&value);

    if layer.id.trim().is_empty() {
        return Err("every layer needs an id".to_string());
    }

    if layer.name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("layer '{label}': the name is too long"));
    }

    if ![layer.x, layer.y, layer.width, layer.height]
        .iter()
        .all(|value| value.is_finite() && in_frame(*value))
    {
        return Err(format!(
            "layer '{label}': position and size are percent of the frame, 0 to 100"
        ));
    }

    if layer.width <= 0.0 || layer.height <= 0.0 {
        return Err(format!("layer '{label}': width and height must be above 0"));
    }

    if !in_frame(layer.x + layer.width) || !in_frame(layer.y + layer.height) {
        return Err(format!(
            "layer '{label}': it goes past the edge of the frame"
        ));
    }

    if !(0.0..=1.0).contains(&layer.opacity) {
        return Err(format!("layer '{label}': opacity goes from 0 to 1"));
    }

    if !(layer.font_size > 0.0 && layer.font_size <= 50.0) {
        return Err(format!(
            "layer '{label}': the font size is percent of the frame height, above 0 up to 50"
        ));
    }

    if !(0.0..=50.0).contains(&layer.radius) {
        return Err(format!(
            "layer '{label}': the corner radius goes from 0 to 50"
        ));
    }

    if layer.text.chars().count() > MAX_TEXT_CHARS {
        return Err(format!("layer '{label}': the text is too long"));
    }

    if !is_color(&layer.color) {
        return Err(format!("layer '{label}': the color must look like #ffffff"));
    }

    if let Some(fill) = layer.fill.as_deref().filter(|fill| !fill.is_empty())
        && !is_color(fill)
    {
        return Err(format!("layer '{label}': the fill must look like #ffffff"));
    }

    if layer.kind == LayerKind::Image
        && layer
            .image
            .as_deref()
            .is_none_or(|image| image.trim().is_empty())
    {
        return Err(format!("layer '{label}': choose the image"));
    }

    if layer.animation_ms > MAX_ANIMATION_MS || layer.delay_ms > MAX_ANIMATION_MS {
        return Err(format!(
            "layer '{label}': animation and delay go up to {MAX_ANIMATION_MS} ms"
        ));
    }

    Ok(())
}

/// Check a template from the web interface or the API and turn it into a
/// row: names are trimmed and image paths become storage-relative.
pub fn prepare_template(
    storage: &std::path::Path,
    channel_id: i32,
    id: i32,
    input: TemplateInput,
) -> Result<GraphicsTemplate, ServiceError> {
    let bad_request = ServiceError::BadRequest;
    let name = input.name.trim().to_string();
    let group = input.group.trim().to_string();

    if name.is_empty() {
        return Err(bad_request("give the template a name".to_string()));
    }

    if name.chars().count() > MAX_NAME_CHARS || group.chars().count() > MAX_NAME_CHARS {
        return Err(bad_request("the name or the group is too long".to_string()));
    }

    if !input.duration.is_finite() || !(0.0..=86_400.0).contains(&input.duration) {
        return Err(bad_request(
            "duration is in seconds, 0 (until taken out) up to a day".to_string(),
        ));
    }

    if input.layers.len() > MAX_LAYERS {
        return Err(bad_request(format!(
            "a template has at most {MAX_LAYERS} layers"
        )));
    }

    let mut layers = input.layers;

    for layer in &mut layers {
        layer.id = layer.id.trim().to_string();
        layer.name = layer.name.trim().to_string();
        check_layer(layer).map_err(bad_request)?;

        layer.image = match layer.image.as_deref().map(str::trim) {
            Some("") => None,
            Some(image) if whole_placeholder(image).is_some() => Some(image.to_string()),
            Some(image) => Some(norm_abs_path(storage, image)?.2),
            None => None,
        };
    }

    if layers
        .iter()
        .filter(|layer| layer.kind == LayerKind::Video)
        .count()
        > 1
    {
        return Err(bad_request(
            "a template has at most one picture (video) layer".to_string(),
        ));
    }

    for (index, layer) in layers.iter().enumerate() {
        if layers[..index].iter().any(|other| other.id == layer.id) {
            return Err(bad_request(format!(
                "two layers have the id '{}'",
                layer.id
            )));
        }
    }

    Ok(GraphicsTemplate {
        id,
        channel_id,
        name,
        group,
        duration: input.duration,
        during_lshape: input.during_lshape,
        fields: template_fields(&layers),
        layers,
    })
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` as a color; anything else is none.
fn parse_color(value: &str) -> Option<noar_engine::RgbaColor> {
    let hex = value.strip_prefix('#')?;

    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let digit = |index: usize| u8::from_str_radix(&hex[index..index + 1], 16).ok();
    let pair = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
    let color = |r, g, b, a| noar_engine::RgbaColor { r, g, b, a };

    match hex.len() {
        3 => Some(color(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17, 255)),
        6 => Some(color(pair(0)?, pair(2)?, pair(4)?, 255)),
        8 => Some(color(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
        _ => None,
    }
}

/// `text` with every `{field}` replaced by its value; a field without a
/// value is left empty.
pub fn fill_fields(text: &str, values: &HashMap<String, String>) -> String {
    let mut filled = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];

        let Some(close) = after.find('}') else {
            break;
        };

        let name = &after[..close];
        filled.push_str(&rest[..open]);

        if !name.is_empty() && name.chars().all(is_field_char) {
            filled.push_str(values.get(name).map(String::as_str).unwrap_or_default());
            rest = &after[close + 1..];
        } else {
            filled.push('{');
            rest = after;
        }
    }

    filled.push_str(rest);

    filled
}

/// The image of a layer: its own storage path, or the path given for its
/// field. A field left empty draws nothing.
fn layer_image(
    image: &str,
    values: &HashMap<String, String>,
    storage: &std::path::Path,
) -> Result<Option<std::path::PathBuf>, ServiceError> {
    let image = match whole_placeholder(image) {
        Some(field) => match values.get(&field).map(|value| value.trim()) {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => return Ok(None),
        },
        None => image.to_string(),
    };
    let (path, _, relative) = norm_abs_path(storage, &image)?;

    if !path.is_file() || !has_image_extension(&path) {
        return Err(ServiceError::BadRequest(format!(
            "{relative} is not an image in the storage"
        )));
    }

    Ok(Some(path))
}

fn engine_animation(animation: Animation) -> noar_engine::GraphicAnimation {
    use noar_engine::GraphicAnimation as Engine;

    match animation {
        Animation::None => Engine::None,
        Animation::Fade => Engine::Fade,
        Animation::SlideLeft => Engine::SlideLeft,
        Animation::SlideRight => Engine::SlideRight,
        Animation::SlideUp => Engine::SlideUp,
        Animation::SlideDown => Engine::SlideDown,
        Animation::Wipe => Engine::Wipe,
        Animation::Scale => Engine::Scale,
    }
}

/// A template with `values` in its fields, as the engine draws it. Hidden
/// layers are left out.
pub fn graphic_spec(
    template: &GraphicsTemplate,
    values: &HashMap<String, String>,
    storage: &std::path::Path,
) -> Result<noar_engine::GraphicSpec, ServiceError> {
    let mut layers = Vec::with_capacity(template.layers.len());
    let mut picture = None;

    for layer in template.layers.iter().filter(|layer| layer.visible) {
        let fill = layer.fill.as_deref().and_then(parse_color);
        let content = match layer.kind {
            LayerKind::Video => {
                picture = Some(noar_engine::PictureWindow {
                    x: layer.x,
                    y: layer.y,
                    width: layer.width,
                    height: layer.height,
                });
                continue;
            }
            LayerKind::Box => match fill {
                Some(fill) => noar_engine::GraphicContent::Box { fill },
                None => continue,
            },
            LayerKind::Text => noar_engine::GraphicContent::Text {
                text: fill_fields(&layer.text, values),
                font_family: layer.font_family.clone(),
                font_size: layer.font_size,
                bold: layer.bold,
                color: parse_color(&layer.color).unwrap_or(noar_engine::RgbaColor {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                }),
                align: match layer.align {
                    TextAlign::Left => noar_engine::BlockAlign::Left,
                    TextAlign::Center => noar_engine::BlockAlign::Center,
                    TextAlign::Right => noar_engine::BlockAlign::Right,
                },
                fill,
            },
            LayerKind::Image => {
                match layer_image(layer.image.as_deref().unwrap_or_default(), values, storage)? {
                    Some(path) => noar_engine::GraphicContent::Image { path },
                    None => continue,
                }
            }
        };

        layers.push(noar_engine::GraphicLayerSpec {
            content,
            x: layer.x,
            y: layer.y,
            width: layer.width,
            height: layer.height,
            opacity: layer.opacity,
            radius: layer.radius,
            animation_in: engine_animation(layer.animation_in),
            animation_out: engine_animation(layer.animation_out),
            animation_ms: layer.animation_ms,
            delay_ms: layer.delay_ms,
        });
    }

    Ok(noar_engine::GraphicSpec { layers, picture })
}

/// Which template each graphic on air came from.
#[derive(Debug, Clone, Default)]
pub struct GraphicsTakes(Arc<StdMutex<Vec<TakeRecord>>>);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TakeRecord {
    pub take_id: u64,
    pub template_id: i32,
    pub name: String,
}

impl GraphicsTakes {
    fn records(&self) -> std::sync::MutexGuard<'_, Vec<TakeRecord>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
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
    });

    Ok(take_id)
}

/// Put `template` on air with `values` in its fields, following its rule
/// for an L-shape on air. `duration` replaces the template's own.
pub async fn take_template(
    manager: &ChannelManager,
    template: GraphicsTemplate,
    values: HashMap<String, String>,
    duration: Option<f64>,
) -> Result<TakeOutcome, ServiceError> {
    let seconds = duration.unwrap_or(template.duration);

    if !seconds.is_finite() || !(0.0..=86_400.0).contains(&seconds) {
        return Err(ServiceError::BadRequest(
            "duration is in seconds, 0 (until taken out) up to a day".to_string(),
        ));
    }

    let duration = (seconds > 0.0).then(|| Duration::from_secs_f64(seconds));
    let storage = manager.config.read().await.channel.storage.clone();
    let spec = graphic_spec(&template, &values, &storage)?;

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
                        && let Err(error) = draw_and_take(&manager, &template, spec, duration).await
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

    let take_id = draw_and_take(manager, &template, spec, duration).await?;

    Ok(TakeOutcome::OnAir { take_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_layer(id: &str, text: &str) -> TemplateLayer {
        serde_json::from_value(serde_json::json!({
            "id": id, "x": 5, "y": 80, "width": 40, "height": 8, "text": text
        }))
        .unwrap()
    }

    #[test]
    fn fields_come_from_texts_and_whole_image_placeholders_in_order() {
        let mut sponsor = text_layer("sponsor", "");
        sponsor.kind = LayerKind::Image;
        sponsor.image = Some(" {sponsor} ".to_string());

        let mut fixed_image = text_layer("bug", "");
        fixed_image.kind = LayerKind::Image;
        fixed_image.image = Some("Graphics/{not-a-field}.png".to_string());

        let layers = vec![
            text_layer("name", "{name}"),
            text_layer("role", "{role} at {farm} · {name}"),
            sponsor,
            fixed_image,
            text_layer("plain", "Price {in R$} {} {open"),
        ];

        assert_eq!(
            template_fields(&layers),
            ["name", "role", "farm", "sponsor"]
        );
    }

    #[test]
    fn layers_take_defaults_and_are_checked() {
        let layer = text_layer("name", "{name}");

        assert!(layer.visible);
        assert_eq!(layer.opacity, 1.0);
        assert_eq!(layer.color, "#ffffff");
        assert_eq!(layer.animation_in, Animation::Fade);
        assert_eq!(layer.animation_ms, 400);
        assert!(check_layer(&layer).is_ok());

        let mut wide = layer.clone();
        wide.x = 70.0;
        assert!(check_layer(&wide).is_err(), "70 + 40 goes past the edge");

        let mut snapped = layer.clone();
        snapped.x = 60.0005;
        assert!(check_layer(&snapped).is_ok(), "pixel snapping slack");

        let mut empty = layer.clone();
        empty.height = 0.0;
        assert!(check_layer(&empty).is_err());

        let mut short_color = layer.clone();
        short_color.color = "#fa0".to_string();
        short_color.fill = Some("#000000cc".to_string());
        assert!(check_layer(&short_color).is_ok());

        let mut imageless = layer;
        imageless.kind = LayerKind::Image;
        assert!(check_layer(&imageless).is_err());
    }

    #[test]
    fn during_lshape_reads_back_what_it_writes() {
        for value in [DuringLShape::Follow, DuringLShape::Wait, DuringLShape::Skip] {
            assert_eq!(DuringLShape::parse(value.as_str()), Some(value));
        }

        assert_eq!(DuringLShape::parse("other"), None);
    }

    #[test]
    fn prepare_template_trims_and_keeps_field_images() {
        let storage = std::env::temp_dir();
        let mut sponsor = text_layer(" sponsor ", "");
        sponsor.kind = LayerKind::Image;
        sponsor.image = Some("{sponsor}".to_string());

        let input = TemplateInput {
            name: "  Sponsor bug ".to_string(),
            group: " Sponsors ".to_string(),
            duration: 0.0,
            during_lshape: DuringLShape::Skip,
            layers: vec![sponsor],
        };
        let template = prepare_template(&storage, 3, 0, input).unwrap();

        assert_eq!(template.name, "Sponsor bug");
        assert_eq!(template.group, "Sponsors");
        assert_eq!(template.layers[0].id, "sponsor");
        assert_eq!(template.layers[0].image.as_deref(), Some("{sponsor}"));
        assert_eq!(template.fields, ["sponsor"]);

        let too_long = TemplateInput {
            name: "Clock".to_string(),
            group: String::new(),
            duration: 90_000.0,
            during_lshape: DuringLShape::Follow,
            layers: Vec::new(),
        };
        assert!(prepare_template(&storage, 3, 0, too_long).is_err());
    }

    #[test]
    fn fields_are_filled_and_unknown_braces_stay() {
        let values = HashMap::from([
            ("name".to_string(), "Maria".to_string()),
            ("role".to_string(), "Farmer".to_string()),
        ]);

        assert_eq!(fill_fields("{name} · {role}", &values), "Maria · Farmer");
        assert_eq!(fill_fields("{name}{missing}!", &values), "Maria!");
        assert_eq!(fill_fields("R$ {in cash} {", &values), "R$ {in cash} {");
    }

    #[test]
    fn colors_are_read_in_every_hex_form() {
        let color = |value| parse_color(value).map(|c| (c.r, c.g, c.b, c.a));

        assert_eq!(color("#fa0"), Some((255, 170, 0, 255)));
        assert_eq!(color("#f39223"), Some((243, 146, 35, 255)));
        assert_eq!(color("#00000080"), Some((0, 0, 0, 128)));
        assert_eq!(color("orange"), None);
        assert_eq!(color("#12345"), None);
    }

    #[test]
    fn a_spec_fills_texts_and_drops_hidden_layers_and_empty_image_fields() {
        let storage = std::env::temp_dir();
        let mut hidden = text_layer("hidden", "secret");
        hidden.visible = false;
        let mut sponsor = text_layer("sponsor", "");
        sponsor.kind = LayerKind::Image;
        sponsor.image = Some("{sponsor}".to_string());
        let mut bar = text_layer("bar", "");
        bar.kind = LayerKind::Box;
        bar.fill = Some("#f39223".to_string());

        let template = GraphicsTemplate {
            layers: vec![bar, text_layer("name", "{name}"), hidden, sponsor],
            ..Default::default()
        };
        let values = HashMap::from([("name".to_string(), "Maria".to_string())]);
        let spec = graphic_spec(&template, &values, &storage).unwrap();

        assert_eq!(spec.layers.len(), 2);
        assert!(matches!(
            spec.layers[0].content,
            noar_engine::GraphicContent::Box { .. }
        ));

        let noar_engine::GraphicContent::Text { text, .. } = &spec.layers[1].content else {
            panic!("a text layer");
        };
        assert_eq!(text, "Maria");

        let missing = HashMap::from([("sponsor".to_string(), "nowhere.png".to_string())]);
        assert!(
            graphic_spec(&template, &missing, &storage).is_err(),
            "a missing image"
        );
    }

    #[test]
    fn a_video_layer_makes_the_template_an_lshape() {
        let storage = std::env::temp_dir();
        let mut art = text_layer("art", "");
        art.kind = LayerKind::Box;
        art.fill = Some("#f39223".to_string());
        let mut picture = text_layer("picture", "");
        picture.kind = LayerKind::Video;
        (picture.x, picture.y, picture.width, picture.height) = (30.0, 0.0, 70.0, 70.0);

        let template = GraphicsTemplate {
            layers: vec![art, picture.clone()],
            ..Default::default()
        };
        let spec = graphic_spec(&template, &HashMap::new(), &storage).unwrap();

        assert_eq!(
            spec.layers.len(),
            1,
            "the video layer is the window, not a layer"
        );
        assert_eq!(
            spec.picture,
            Some(noar_engine::PictureWindow {
                x: 30.0,
                y: 0.0,
                width: 70.0,
                height: 70.0
            })
        );

        let mut second = picture.clone();
        second.id = "second".to_string();
        let input = TemplateInput {
            name: "Two pictures".to_string(),
            group: String::new(),
            duration: 0.0,
            during_lshape: DuringLShape::Follow,
            layers: vec![picture, second],
        };
        assert!(prepare_template(&storage, 1, 0, input).is_err());
    }
}
