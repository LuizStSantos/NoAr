//! Graphics templates: what a lower third, a topic bar or a sponsor bug looks
//! like, designed once and put on air many times. A template is a list of
//! layers placed in percent of the frame; `{field}` in a text, or as a whole
//! image path, is filled in each time the template goes on air.

use serde::{Deserialize, Serialize};

use crate::{db::models::GraphicsTemplate, file::norm_abs_path, utils::errors::ServiceError};

pub const MAX_LAYERS: usize = 64;
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
            LayerKind::Box => {}
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
}
