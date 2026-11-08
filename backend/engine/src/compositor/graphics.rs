//! Graphics compositor: prepared layers, permanent models and timed takes.
//!
//! `rendering` rasterizes models away from the frame thread, then blends cached
//! pixels; `images` owns bounded animation decoding; `geometry` shares snapping
//! and masks. `logo` and `lshape` retain role-specific placement, clip overrides
//! and transition controls inside this same Graphics domain.
//!
//! This facade owns model specifications and runtime take state. Temporary
//! graphics are blended before the L-shape shrinks the picture into its window.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};

#[path = "graphics/geometry/graphics.rs"]
mod geometry;
#[path = "graphics/images/graphics.rs"]
pub(crate) mod images;
#[path = "graphics/logo/graphics.rs"]
pub mod logo;
#[path = "graphics/lshape/graphics.rs"]
pub mod lshape;
#[path = "graphics/rendering/graphics.rs"]
mod rendering;
#[path = "graphics/scroll/graphics.rs"]
mod scroll;

use crate::{compositor::text::BlockAlign, frame::VideoFrame, utils::config::RgbaColor};

pub use rendering::render_graphic;
pub use scroll::{ScrollDirection, TextScroll};

use self::{
    lshape::{LSHAPE_SWITCH_SECONDS, LShapeGraphic},
    rendering::{blend_layer, blend_layer_with_visibility, layer_level},
};

/// At most this many graphics on air at once; taking one more first takes
/// the oldest out.
pub const MAX_GRAPHICS_ON_AIR: usize = 16;

/// Resolve a native logo's layout for its editable template canvas, using the
/// same image dimensions, expression parser and chroma rounding as playout.
pub fn logo_layer_geometry(
    config: &crate::LogoConfig,
    width: u32,
    height: u32,
) -> Result<(f64, f64, f64, f64)> {
    anyhow::ensure!(width > 0 && height > 0, "output size must be positive");
    let logo = crate::compositor::graphics::logo::LogoOverlay::load(config, width, height)?;

    Ok((
        f64::from(logo.x) * 100.0 / f64::from(width),
        f64::from(logo.y) * 100.0 / f64::from(height),
        f64::from(logo.width) * 100.0 / f64::from(width),
        f64::from(logo.height) * 100.0 / f64::from(height),
    ))
}

impl crate::LShapeControl {
    /// Draw a bound L-shape model, preserving its complete picture geometry.
    /// Loading it does not take it on or off air.
    pub fn set_template(&self, spec: &GraphicSpec, width: u32, height: u32) -> Result<()> {
        let primary = spec
            .layers
            .iter()
            .position(|layer| matches!(layer.content, GraphicContent::Image { .. }))
            .unwrap_or(0);

        self.set_template_controlled(spec, primary, width, height)
    }

    pub fn set_template_controlled(
        &self,
        spec: &GraphicSpec,
        primary: usize,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let rendered = render_graphic(spec, width, height)?;
        anyhow::ensure!(
            rendered.picture.is_some(),
            "channel L-shape needs a video layer"
        );
        self.set_rendered_graphic(rendered.picture, spec, primary)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GraphicAnimation {
    None,
    #[default]
    Fade,
    /// Comes from the left of its place, and leaves that way.
    SlideLeft,
    SlideRight,
    /// Comes from below its place, and leaves that way.
    SlideUp,
    SlideDown,
    /// Revealed from left to right.
    Wipe,
    /// Drawn as a fade for now.
    Scale,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GraphicContent {
    Text {
        text: String,
        scroll: Option<TextScroll>,
        font_family: Option<String>,
        /// Percent of the frame height.
        font_size: f64,
        bold: bool,
        color: RgbaColor,
        align: BlockAlign,
        /// Box behind the text.
        fill: Option<RgbaColor>,
    },
    Image {
        path: PathBuf,
    },
    Box {
        fill: RgbaColor,
    },
    Shape {
        shape: GraphicShape,
        fill: Option<RgbaColor>,
        stroke: Option<RgbaColor>,
        /// Percent of output height, matching text and radius units.
        stroke_width: f64,
    },
}

/// Additional primitives share the same percent geometry and alpha plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicShape {
    Rectangle,
    Rounded,
    Ellipse,
    Line,
}

/// One layer, with position and size in percent of the frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphicLayerSpec {
    pub content: GraphicContent,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
    /// Corner radius, percent of the frame height.
    pub radius: f64,
    pub animation_in: GraphicAnimation,
    pub animation_out: GraphicAnimation,
    pub animation_ms: u32,
    pub delay_ms: u32,
}

/// Where the picture goes in a template that is an L-shape, in percent of
/// the frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PictureWindow {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A template with its fields filled in, ready to draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphicSpec {
    pub layers: Vec<GraphicLayerSpec>,
    /// With a picture window the template is an L-shape: its layers are the
    /// art around the picture, which shrinks into the window.
    pub picture: Option<PictureWindow>,
}

struct RenderedLayer {
    frame: VideoFrame,
    chroma_alpha: Vec<u8>,
    image_animation: Option<Arc<images::ImageAnimation>>,
    scroll: Option<scroll::RenderedScroll>,
    prepared_at: Instant,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    opacity: u8,
    animation_in: GraphicAnimation,
    animation_out: GraphicAnimation,
    animation: Duration,
    delay: Duration,
}

/// A graphic drawn for one frame size.
pub struct RenderedGraphic {
    layers: Vec<RenderedLayer>,
    frame_width: u32,
    frame_height: u32,
    /// For an L-shape template: its art and the window of the picture.
    picture: Option<Arc<LShapeGraphic>>,
}

pub(crate) struct RoleImageBlend {
    pub index: usize,
    pub opacity: f64,
    pub only: bool,
}

/// How long an L-shape template takes to go in or out: as long as the
/// channel's L-shape.
fn picture_switch() -> Duration {
    Duration::from_secs_f64(LSHAPE_SWITCH_SECONDS)
}

impl RenderedGraphic {
    /// Retain steady artwork's clock when a channel model's data is rerasterized.
    fn inherit_motion(&mut self, previous: &Self) {
        let Some(epoch) = previous.layers.iter().map(|layer| layer.prepared_at).min() else {
            return;
        };

        for layer in &mut self.layers {
            layer.prepared_at = epoch;
        }
    }

    pub(crate) fn is_animated(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| layer.image_animation.is_some() || layer.scroll.is_some())
    }

    /// Fully entered artwork, including GIF frame selection, without I/O.
    pub(crate) fn blend_steady(&self, frame: &mut VideoFrame) {
        for layer in &self.layers {
            blend_layer(
                frame,
                layer,
                1.0,
                GraphicAnimation::None,
                layer.prepared_at.elapsed(),
            );
        }
    }

    pub(crate) fn picture_graphic(&self) -> Option<&LShapeGraphic> {
        self.picture.as_deref()
    }
    pub(crate) fn role_levels(
        &self,
        elapsed: Option<Duration>,
        hidden: bool,
        from: &[f64],
    ) -> Vec<f64> {
        self.layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                let target = if hidden { 0.0 } else { 1.0 };
                let Some(elapsed) = elapsed else {
                    return target;
                };
                let animation = if hidden {
                    layer.animation_out
                } else {
                    layer.animation_in
                };

                if animation == GraphicAnimation::None || layer.animation.is_zero() {
                    return target;
                }

                let start = from.get(index).copied().unwrap_or(1.0 - target);
                let elapsed = if hidden {
                    elapsed
                } else {
                    elapsed.saturating_sub(layer.delay)
                };
                let progress = (elapsed.as_secs_f64() / layer.animation.as_secs_f64()).min(1.0);

                start + (target - start) * progress
            })
            .collect()
    }

    /// Permanent logo layers use the native linear switch curve so converted
    /// fades keep their exact timing. Clip/L-shape suppression is a separate mask.
    pub(crate) fn blend_role(
        &self,
        frame: &mut VideoFrame,
        levels: &[f64],
        hidden: bool,
        visibility: f64,
        image: RoleImageBlend,
    ) {
        for (index, layer) in self.layers.iter().enumerate() {
            if image.only && index != image.index {
                continue;
            }

            let level = levels.get(index).copied().unwrap_or(1.0);

            if level <= 0.0 {
                continue;
            }

            let animation = if hidden {
                layer.animation_out
            } else {
                layer.animation_in
            };
            let factor = if index == image.index {
                image.opacity
            } else {
                1.0
            };
            blend_layer_with_visibility(
                frame,
                layer,
                level,
                animation,
                visibility * factor,
                layer.prepared_at.elapsed(),
            );
        }
    }

    /// How long its exit takes, from the moment it is taken out.
    fn exit_time(&self) -> Duration {
        if self.picture.is_some() {
            return picture_switch();
        }

        self.layers
            .iter()
            .filter(|layer| layer.animation_out != GraphicAnimation::None)
            .map(|layer| layer.animation)
            .max()
            .unwrap_or_default()
    }
}

struct OnAir {
    id: u64,
    graphic: Arc<RenderedGraphic>,
    taken_at: Instant,
    /// When it goes out by itself.
    out_after: Option<Duration>,
    taken_out_at: Option<Instant>,
}

impl OnAir {
    fn out_since(&self, now: Instant) -> Option<Duration> {
        let auto = self
            .out_after
            .map(|after| self.taken_at + after)
            .filter(|at| *at <= now);
        let at = match (self.taken_out_at, auto) {
            (Some(manual), Some(auto)) => Some(manual.min(auto)),
            (manual, auto) => manual.or(auto),
        };

        at.map(|at| now.saturating_duration_since(at))
    }

    fn finished(&self, now: Instant) -> bool {
        self.out_since(now)
            .is_some_and(|since| since >= self.graphic.exit_time())
    }
}

/// Stable channel roles, independent of the temporary take queue. A picture
/// window is deliberately not permanent: an L-shape is taken in and out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlwaysOnSlot {
    Logo,
    Ticker,
}

impl AlwaysOnSlot {
    fn index(self) -> usize {
        match self {
            Self::Logo => 0,
            Self::Ticker => 1,
        }
    }
}

struct AlwaysOn {
    graphic: OnAir,
    visible: bool,
}

#[derive(Default)]
struct GraphicsState {
    on_air: Vec<OnAir>,
    always_on: [Option<AlwaysOn>; 2],
    persistent: BTreeMap<u64, AlwaysOn>,
    // Native logo composition preserves clip marks, remote placement and desktop overlays.
    channel_logo: crate::LogoControl,
    next_id: u64,
}

/// What is on air, as told to the outside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicOnAir {
    pub id: u64,
    pub since: Duration,
    /// It is on its way out.
    pub leaving: bool,
}

/// The template graphics of an output, taken and taken out from outside the
/// playout.
#[derive(Clone, Default)]
pub struct GraphicsControl(Arc<Mutex<GraphicsState>>);

impl std::fmt::Debug for GraphicsControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphicsControl")
            .field("on_air", &self.state().on_air.len())
            .finish()
    }
}

impl GraphicsControl {
    /// The native compositor for the channel's bound logo model. Compatibility
    /// controls share this handle rather than creating a second logo plane.
    pub fn channel_logo(&self) -> crate::LogoControl {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .channel_logo
            .clone()
    }

    /// Install the native image model in the permanent logo role. Its fade and
    /// external overrides are independent of the temporary take queue.
    pub fn set_channel_logo(
        &self,
        config: Option<&crate::LogoConfig>,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.channel_logo().set_configured(config, width, height)?;
        self.clear_always_on(AlwaysOnSlot::Logo);
        Ok(())
    }

    fn state(&self) -> std::sync::MutexGuard<'_, GraphicsState> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Configure a permanent role. It has no expiry and does not consume a
    /// temporary take id. Reject picture windows before changing the role.
    pub fn set_always_on(
        &self,
        slot: AlwaysOnSlot,
        graphic: RenderedGraphic,
        visible: bool,
    ) -> Result<()> {
        if graphic.picture.is_some() {
            return Err(anyhow!(
                "an always-on graphic cannot contain a picture window"
            ));
        }

        self.state().always_on[slot.index()] = Some(AlwaysOn {
            graphic: OnAir {
                id: 0,
                graphic: Arc::new(graphic),
                taken_at: Instant::now(),
                out_after: None,
                taken_out_at: None,
            },
            visible,
        });

        Ok(())
    }

    pub fn refresh_always_on(&self, slot: AlwaysOnSlot, graphic: RenderedGraphic) -> Result<()> {
        if graphic.picture.is_some() {
            return Err(anyhow!("a permanent graphic cannot contain a video window"));
        }

        if let Some(current) = &mut self.state().always_on[slot.index()] {
            current.graphic.graphic = Arc::new(graphic);
        }

        Ok(())
    }

    /// Hide or restore a configured role explicitly. Repeating a command
    /// does not restart its transition; hidden roles retain their definition.
    pub fn set_always_on_visible(&self, slot: AlwaysOnSlot, visible: bool) -> bool {
        let mut state = self.state();
        let Some(always_on) = &mut state.always_on[slot.index()] else {
            return false;
        };

        if always_on.visible != visible {
            let now = Instant::now();
            always_on.visible = visible;

            if visible {
                always_on.graphic.taken_at = now;
                always_on.graphic.taken_out_at = None;
            } else {
                always_on.graphic.taken_out_at = Some(now);
            }
        }

        true
    }

    pub fn always_on_visible(&self, slot: AlwaysOnSlot) -> Option<bool> {
        self.state().always_on[slot.index()]
            .as_ref()
            .map(|always_on| always_on.visible)
    }

    /// Remove a role's definition, for a configuration change rather than
    /// an operator's ordinary take-out command.
    pub fn clear_always_on(&self, slot: AlwaysOnSlot) {
        self.state().always_on[slot.index()] = None;
    }

    /// Blend channel roles in full-frame space, after the L-shape. Clip
    /// logo gaps and the L-shape's logo fade affect only the logo role.
    pub(crate) fn blend_always_on(&self, frame: &mut VideoFrame, logo_visibility: f64) {
        self.blend_always_on_at(frame, logo_visibility, Instant::now());
    }

    fn blend_always_on_at(&self, frame: &mut VideoFrame, logo_visibility: f64, now: Instant) {
        let graphics: Vec<_> = {
            let state = self.state();

            [AlwaysOnSlot::Logo, AlwaysOnSlot::Ticker]
                .into_iter()
                .filter_map(|slot| {
                    let entry = state.always_on[slot.index()].as_ref()?;

                    if !entry.visible && entry.graphic.taken_out_at.is_none() {
                        return None;
                    }

                    Some((
                        slot,
                        entry.graphic.graphic.clone(),
                        now.saturating_duration_since(entry.graphic.taken_at),
                        entry.graphic.out_since(now),
                    ))
                })
                .chain(state.persistent.values().filter_map(|entry| {
                    if !entry.visible && entry.graphic.taken_out_at.is_none() {
                        return None;
                    }

                    Some((
                        AlwaysOnSlot::Ticker,
                        entry.graphic.graphic.clone(),
                        now.saturating_duration_since(entry.graphic.taken_at),
                        entry.graphic.out_since(now),
                    ))
                }))
                .collect()
        };

        for (slot, graphic, since_take, since_out) in graphics {
            if graphic.frame_width != frame.width() || graphic.frame_height != frame.height() {
                continue;
            }

            let visibility = match slot {
                AlwaysOnSlot::Logo => logo_visibility.clamp(0.0, 1.0),
                AlwaysOnSlot::Ticker => 1.0,
            };

            for layer in &graphic.layers {
                let (level, animation) = layer_level(layer, since_take, since_out);

                // Apply visibility to final opacity, not animation progress:
                // hiding the logo must not move a completed slide backwards.
                blend_layer_with_visibility(
                    frame,
                    layer,
                    level,
                    animation,
                    visibility,
                    since_take.saturating_sub(layer.delay),
                );
            }
        }
    }

    /// Replace a permanent model without resetting its visibility or entry time.
    pub fn set_persistent(&self, id: u64, graphic: RenderedGraphic) -> Result<()> {
        if graphic.picture.is_some() {
            return Err(anyhow!(
                "a permanent template cannot contain a video window"
            ));
        }

        let mut state = self.state();

        if let Some(current) = state.persistent.get_mut(&id) {
            current.graphic.graphic = Arc::new(graphic);
        } else {
            if state.persistent.len() >= MAX_GRAPHICS_ON_AIR {
                return Err(anyhow!("too many permanent templates"));
            }

            state.persistent.insert(
                id,
                AlwaysOn {
                    graphic: OnAir {
                        id,
                        graphic: Arc::new(graphic),
                        taken_at: Instant::now(),
                        out_after: None,
                        taken_out_at: None,
                    },
                    visible: true,
                },
            );
        }

        Ok(())
    }

    pub fn retain_persistent(&self, ids: &[u64]) {
        self.state().persistent.retain(|id, _| ids.contains(id));
    }

    pub fn persistent_visible(&self, id: u64) -> Option<bool> {
        self.state().persistent.get(&id).map(|entry| entry.visible)
    }

    pub fn set_persistent_visible(&self, id: u64, visible: bool) -> bool {
        let mut state = self.state();
        let Some(entry) = state.persistent.get_mut(&id) else {
            return false;
        };

        if entry.visible != visible {
            let now = Instant::now();
            entry.visible = visible;

            if visible {
                entry.graphic.taken_at = now;
                entry.graphic.taken_out_at = None;
            } else {
                entry.graphic.taken_out_at = Some(now);
            }
        }

        true
    }

    /// Put a graphic on air; `duration` takes it out by itself. Answers its
    /// id for taking it out.
    pub fn take(&self, graphic: RenderedGraphic, duration: Option<Duration>) -> u64 {
        let now = Instant::now();
        let mut state = self.state();

        state.on_air.retain(|graphic| !graphic.finished(now));

        // One L-shape at a time: a new one takes the one on air out.
        if graphic.picture.is_some() {
            for other in &mut state.on_air {
                if other.graphic.picture.is_some() && other.out_since(now).is_none() {
                    other.taken_out_at = Some(now);
                }
            }
        }

        while state.on_air.len() >= MAX_GRAPHICS_ON_AIR {
            state.on_air.remove(0);
        }

        state.next_id += 1;
        let id = state.next_id;

        state.on_air.push(OnAir {
            id,
            graphic: Arc::new(graphic),
            taken_at: now,
            out_after: duration.filter(|duration| !duration.is_zero()),
            taken_out_at: None,
        });

        id
    }

    /// Refresh pixels for a live clock/data value without restarting take/exit time.
    pub fn update_graphic(&self, id: u64, graphic: RenderedGraphic) -> bool {
        let mut state = self.state();
        let Some(current) = state.on_air.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        current.graphic = Arc::new(graphic);

        true
    }

    /// Start a graphic's exit; `false` when it is not on air.
    pub fn take_out(&self, id: u64) -> bool {
        let now = Instant::now();
        let mut state = self.state();

        match state
            .on_air
            .iter_mut()
            .find(|graphic| graphic.id == id && graphic.out_since(now).is_none())
        {
            Some(graphic) => {
                graphic.taken_out_at = Some(now);
                true
            }
            None => false,
        }
    }

    /// Start the exit of every temporary graphic. Permanent roles are unchanged.
    pub fn take_all_out(&self) {
        let now = Instant::now();

        for graphic in &mut self.state().on_air {
            if graphic.out_since(now).is_none() {
                graphic.taken_out_at = Some(now);
            }
        }
    }

    pub fn on_air(&self) -> Vec<GraphicOnAir> {
        let now = Instant::now();

        self.state()
            .on_air
            .iter()
            .filter(|graphic| !graphic.finished(now))
            .map(|graphic| GraphicOnAir {
                id: graphic.id,
                since: now.saturating_duration_since(graphic.taken_at),
                leaving: graphic.out_since(now).is_some(),
            })
            .collect()
    }

    /// Whether an L-shape template is on air, going in or out.
    pub fn picture_on_air(&self) -> bool {
        let now = Instant::now();

        self.state()
            .on_air
            .iter()
            .any(|graphic| graphic.graphic.picture.is_some() && !graphic.finished(now))
    }

    /// The L-shape template on air and how far it is in (0 to 1), for a
    /// frame of `width` x `height`.
    pub(crate) fn picture(&self, width: u32, height: u32) -> Option<(Arc<LShapeGraphic>, f64)> {
        self.picture_at(width, height, Instant::now())
    }

    fn picture_at(
        &self,
        width: u32,
        height: u32,
        now: Instant,
    ) -> Option<(Arc<LShapeGraphic>, f64)> {
        let state = self.state();
        let switch = picture_switch().as_secs_f64();

        state.on_air.iter().rev().find_map(|graphic| {
            let picture = graphic.graphic.picture.as_ref()?;

            if graphic.finished(now)
                || graphic.graphic.frame_width != width
                || graphic.graphic.frame_height != height
            {
                return None;
            }

            let entered = (now
                .saturating_duration_since(graphic.taken_at)
                .as_secs_f64()
                / switch)
                .min(1.0);
            let left = graphic
                .out_since(now)
                .map_or(0.0, |since| (since.as_secs_f64() / switch).min(1.0));
            let level = entered.min(1.0 - left);

            (level > 0.0).then(|| (picture.clone(), level))
        })
    }

    /// Blend every graphic on air onto `frame`, oldest first.
    pub(crate) fn blend(&self, frame: &mut VideoFrame) {
        self.blend_at(frame, Instant::now());
    }

    fn blend_at(&self, frame: &mut VideoFrame, now: Instant) {
        let graphics: Vec<(Arc<RenderedGraphic>, Duration, Option<Duration>)> = {
            let mut state = self.state();

            if state.on_air.is_empty() {
                return;
            }

            state.on_air.retain(|graphic| !graphic.finished(now));
            state
                .on_air
                .iter()
                .map(|graphic| {
                    (
                        graphic.graphic.clone(),
                        now.saturating_duration_since(graphic.taken_at),
                        graphic.out_since(now),
                    )
                })
                .collect()
        };

        for (graphic, since_take, since_out) in graphics {
            if graphic.picture.is_some() {
                continue;
            }

            if graphic.frame_width != frame.width() || graphic.frame_height != frame.height() {
                continue;
            }

            for layer in &graphic.layers {
                let (level, animation) = layer_level(layer, since_take, since_out);
                blend_layer(
                    frame,
                    layer,
                    level,
                    animation,
                    since_take.saturating_sub(layer.delay),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::frame::PixelFormat;

    use super::*;

    const WHITE: RgbaColor = RgbaColor {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    fn black_frame(width: u32, height: u32) -> VideoFrame {
        let mut frame = VideoFrame::new(PixelFormat::Yuv420p, width, height);
        frame.data_mut(0).fill(16);
        frame.data_mut(1).fill(128);
        frame.data_mut(2).fill(128);
        frame
    }

    fn luma(frame: &VideoFrame, x: usize, y: usize) -> u8 {
        frame.data(0)[y * frame.stride(0) + x]
    }

    fn white_box(x: f64, y: f64, animation: GraphicAnimation) -> GraphicLayerSpec {
        GraphicLayerSpec {
            content: GraphicContent::Box { fill: WHITE },
            x,
            y,
            width: 25.0,
            height: 25.0,
            opacity: 1.0,
            radius: 0.0,
            animation_in: animation,
            animation_out: animation,
            animation_ms: 400,
            delay_ms: 0,
        }
    }

    #[test]
    fn live_pixel_updates_preserve_take_duration_entry_and_exit_times() {
        let control = GraphicsControl::default();
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::Fade)],
        };
        let id = control.take(
            render_graphic(&spec, 320, 180).unwrap(),
            Some(Duration::from_secs(3)),
        );
        let entered = control.state().on_air[0].taken_at;
        control.take_out(id);
        let leaving = control.state().on_air[0].taken_out_at;
        assert!(control.update_graphic(id, render_graphic(&spec, 320, 180).unwrap()));
        let state = control.state();
        assert_eq!(state.on_air[0].taken_at, entered);
        assert_eq!(state.on_air[0].taken_out_at, leaving);
        assert_eq!(state.on_air[0].out_after, Some(Duration::from_secs(3)));
    }

    #[test]
    fn scrolling_refresh_preserves_motion_phase_for_temporary_and_permanent_takes() {
        let control = GraphicsControl::default();
        let spec = GraphicSpec {
            picture: None,
            layers: vec![GraphicLayerSpec {
                content: GraphicContent::Text {
                    text: "Markets · São Paulo · USD 5.10".to_string(),
                    scroll: Some(TextScroll {
                        direction: ScrollDirection::Left,
                        speed: 80.0,
                        gap: 40,
                    }),
                    font_family: None,
                    font_size: 6.0,
                    bold: false,
                    color: WHITE,
                    align: BlockAlign::Left,
                    fill: None,
                },
                width: 80.0,
                height: 20.0,
                ..white_box(10.0, 70.0, GraphicAnimation::None)
            }],
        };
        let id = control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        control
            .set_persistent(42, render_graphic(&spec, 320, 180).unwrap())
            .unwrap();
        let now = Instant::now() + Duration::from_secs(1);
        let mut before = black_frame(320, 180);
        control.blend_at(&mut before, now);
        control.blend_always_on_at(&mut before, 1.0, now);
        assert!(control.update_graphic(id, render_graphic(&spec, 320, 180).unwrap()));
        control
            .set_persistent(42, render_graphic(&spec, 320, 180).unwrap())
            .unwrap();
        let mut after = black_frame(320, 180);
        control.blend_at(&mut after, now);
        control.blend_always_on_at(&mut after, 1.0, now);
        assert_eq!(before.data(0), after.data(0));
        assert!(after.data(0).iter().any(|&luma| luma > 16));
        let mut later = black_frame(320, 180);
        control.blend_at(&mut later, now + Duration::from_millis(500));
        control.blend_always_on_at(&mut later, 1.0, now + Duration::from_millis(500));
        assert_ne!(after.data(0), later.data(0));
    }

    #[test]
    fn user_permanent_models_preserve_time_visibility_and_deterministic_order() {
        let control = GraphicsControl::default();
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::None)],
        };
        control
            .set_persistent(42, render_graphic(&spec, 320, 180).unwrap())
            .unwrap();
        let entered = control.state().persistent[&42].graphic.taken_at;
        assert!(control.set_persistent_visible(42, false));
        control
            .set_persistent(42, render_graphic(&spec, 320, 180).unwrap())
            .unwrap();
        assert_eq!(control.persistent_visible(42), Some(false));
        assert_eq!(control.state().persistent[&42].graphic.taken_at, entered);
        control.take_all_out();
        assert_eq!(control.persistent_visible(42), Some(false));
        assert!(control.set_persistent_visible(42, true));
        let mut frame = black_frame(320, 180);
        control.blend_always_on(&mut frame, 0.0);
        assert!(luma(&frame, 20, 20) > 200);
        control.retain_persistent(&[]);
        assert_eq!(control.persistent_visible(42), None);
    }

    #[test]
    fn permanent_roles_survive_expiry_queue_pressure_and_take_all_out() {
        let control = GraphicsControl::default();
        let spec = |x, y| GraphicSpec {
            picture: None,
            layers: vec![white_box(x, y, GraphicAnimation::None)],
        };
        control
            .set_always_on(
                AlwaysOnSlot::Logo,
                render_graphic(&spec(0.0, 0.0), 320, 180).unwrap(),
                true,
            )
            .unwrap();
        control
            .set_always_on(
                AlwaysOnSlot::Ticker,
                render_graphic(&spec(0.0, 50.0), 320, 180).unwrap(),
                true,
            )
            .unwrap();
        let taken = control.state().always_on[0]
            .as_ref()
            .unwrap()
            .graphic
            .taken_at;

        for _ in 0..MAX_GRAPHICS_ON_AIR + 5 {
            control.take(
                render_graphic(&spec(50.0, 0.0), 320, 180).unwrap(),
                Some(Duration::from_secs(1)),
            );
        }

        control.take_all_out();
        let mut frame = black_frame(320, 180);
        let later = taken + Duration::from_secs(86_400);
        control.blend_at(&mut frame, later);
        assert_eq!(luma(&frame, 170, 10), 16, "temporary graphics left");
        control.blend_always_on_at(&mut frame, 1.0, later);
        assert!(luma(&frame, 10, 10) > 230, "permanent logo survived");
        assert!(luma(&frame, 10, 100) > 230, "permanent ticker survived");
        assert!(control.on_air().is_empty());
    }

    #[test]
    fn permanent_visibility_is_explicit_and_does_not_reset_on_repeated_commands() {
        let control = GraphicsControl::default();
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::Fade)],
        };
        control
            .set_always_on(
                AlwaysOnSlot::Logo,
                render_graphic(&spec, 320, 180).unwrap(),
                false,
            )
            .unwrap();
        let mut frame = black_frame(320, 180);
        control.blend_always_on(&mut frame, 1.0);
        assert_eq!(luma(&frame, 10, 10), 16, "configured but disabled");
        assert!(control.set_always_on_visible(AlwaysOnSlot::Logo, true));
        let entered = control.state().always_on[0]
            .as_ref()
            .unwrap()
            .graphic
            .taken_at;
        assert!(control.set_always_on_visible(AlwaysOnSlot::Logo, true));
        assert_eq!(
            control.state().always_on[0]
                .as_ref()
                .unwrap()
                .graphic
                .taken_at,
            entered
        );
        control.blend_always_on_at(&mut frame, 1.0, entered + Duration::from_secs(1));
        assert!(luma(&frame, 10, 10) > 230);

        assert!(control.set_always_on_visible(AlwaysOnSlot::Logo, false));
        let left = control.state().always_on[0]
            .as_ref()
            .unwrap()
            .graphic
            .taken_out_at
            .unwrap();
        let mut hidden = black_frame(320, 180);
        control.blend_always_on_at(&mut hidden, 1.0, left + Duration::from_secs(1));
        assert_eq!(luma(&hidden, 10, 10), 16);
        assert_eq!(control.always_on_visible(AlwaysOnSlot::Logo), Some(false));
        assert!(control.set_always_on_visible(AlwaysOnSlot::Logo, true));
        control.clear_always_on(AlwaysOnSlot::Logo);
        assert_eq!(control.always_on_visible(AlwaysOnSlot::Logo), None);
        assert!(!control.set_always_on_visible(AlwaysOnSlot::Logo, true));
    }

    #[test]
    fn suppressing_a_permanent_logo_never_suppresses_the_ticker_or_moves_a_slide() {
        let control = GraphicsControl::default();
        let spec = |x, y, animation| GraphicSpec {
            picture: None,
            layers: vec![white_box(x, y, animation)],
        };
        control
            .set_always_on(
                AlwaysOnSlot::Logo,
                render_graphic(&spec(50.0, 0.0, GraphicAnimation::SlideLeft), 320, 180).unwrap(),
                true,
            )
            .unwrap();
        control
            .set_always_on(
                AlwaysOnSlot::Ticker,
                render_graphic(&spec(0.0, 50.0, GraphicAnimation::None), 320, 180).unwrap(),
                true,
            )
            .unwrap();
        let now = Instant::now() + Duration::from_secs(1);
        let mut frame = black_frame(320, 180);
        control.blend_always_on_at(&mut frame, 0.0, now);
        assert_eq!(
            luma(&frame, 170, 10),
            16,
            "logo suppressed for clip/L-shape"
        );
        assert!(
            luma(&frame, 10, 100) > 230,
            "ticker stays in full-frame space"
        );
        let mut partial = black_frame(320, 180);
        control.blend_always_on_at(&mut partial, 0.5, now);
        assert_eq!(
            luma(&partial, 150, 10),
            16,
            "suppression must not rewind the slide"
        );
        assert!((80..200).contains(&luma(&partial, 170, 10)));
    }

    #[test]
    fn a_picture_window_cannot_replace_a_permanent_role() {
        let control = GraphicsControl::default();
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::None)],
        };
        control
            .set_always_on(
                AlwaysOnSlot::Logo,
                render_graphic(&spec, 320, 180).unwrap(),
                true,
            )
            .unwrap();
        let picture = GraphicSpec {
            picture: Some(PictureWindow {
                x: 0.0,
                y: 0.0,
                width: 70.0,
                height: 70.0,
            }),
            ..spec
        };
        assert!(
            control
                .set_always_on(
                    AlwaysOnSlot::Logo,
                    render_graphic(&picture, 320, 180).unwrap(),
                    true
                )
                .is_err()
        );
        let mut frame = black_frame(320, 180);
        control.blend_always_on(&mut frame, 1.0);
        assert!(
            luma(&frame, 10, 10) > 230,
            "invalid replacement kept the old role"
        );
        assert!(!control.picture_on_air());
    }

    #[test]
    fn empty_permanent_state_and_wrong_output_sizes_leave_frames_unchanged() {
        let control = GraphicsControl::default();
        let mut frame = black_frame(320, 180);
        control.blend_always_on(&mut frame, 1.0);
        assert!(frame.data(0).iter().all(|sample| *sample == 16));
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::None)],
        };
        control
            .set_always_on(
                AlwaysOnSlot::Logo,
                render_graphic(&spec, 640, 360).unwrap(),
                true,
            )
            .unwrap();
        control.blend_always_on(&mut frame, 1.0);
        assert!(frame.data(0).iter().all(|sample| *sample == 16));
    }

    #[test]
    fn a_box_lands_where_its_percentages_say() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(50.0, 50.0, GraphicAnimation::None)],
        };
        let graphic = render_graphic(&spec, 320, 180).unwrap();
        let control = GraphicsControl::default();
        control.take(graphic, None);

        let mut frame = black_frame(320, 180);
        control.blend(&mut frame);

        assert!(luma(&frame, 170, 100) > 200, "inside the box");
        assert_eq!(luma(&frame, 150, 100), 16, "left of the box");
        assert_eq!(luma(&frame, 170, 80), 16, "above the box");
        assert_eq!(
            luma(&frame, 250, 100),
            16,
            "right of the box (50% + 25% = 240 px)"
        );
    }

    #[test]
    fn a_fade_comes_in_and_goes_out_over_its_time() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::Fade)],
        };
        let control = GraphicsControl::default();
        control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        let taken = control.state().on_air[0].taken_at;

        let at = |offset_ms: u64| {
            let mut frame = black_frame(320, 180);
            control.blend_at(&mut frame, taken + Duration::from_millis(offset_ms));
            luma(&frame, 10, 10)
        };

        assert_eq!(at(0), 16, "nothing at the start of the fade");
        let half = at(200);
        assert!(half > 60 && half < 200, "half way: {half}");
        assert!(at(400) > 230, "fully in");

        control.state().on_air[0].taken_out_at = Some(taken + Duration::from_millis(1000));
        assert!(at(1000) > 230, "the exit starts from fully in");
        let leaving = at(1200);
        assert!(leaving > 60 && leaving < 200, "half way out: {leaving}");
        assert_eq!(at(1400), 16, "gone");
        assert!(
            control.state().on_air.is_empty(),
            "a finished graphic is dropped"
        );
    }

    #[test]
    fn a_duration_takes_the_graphic_out_by_itself() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::None)],
        };
        let control = GraphicsControl::default();
        control.take(
            render_graphic(&spec, 320, 180).unwrap(),
            Some(Duration::from_secs(2)),
        );
        let taken = control.state().on_air[0].taken_at;

        let mut frame = black_frame(320, 180);
        control.blend_at(&mut frame, taken + Duration::from_millis(1900));
        assert!(luma(&frame, 10, 10) > 230);

        let mut frame = black_frame(320, 180);
        control.blend_at(&mut frame, taken + Duration::from_millis(2001));
        assert_eq!(luma(&frame, 10, 10), 16);
    }

    #[test]
    fn a_slide_starts_off_its_place_and_ends_on_it() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(50.0, 0.0, GraphicAnimation::SlideLeft)],
        };
        let control = GraphicsControl::default();
        control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        let taken = control.state().on_air[0].taken_at;

        let mut early = black_frame(320, 180);
        control.blend_at(&mut early, taken + Duration::from_millis(100));
        let mut done = black_frame(320, 180);
        control.blend_at(&mut done, taken + Duration::from_millis(400));

        assert!(
            luma(&early, 100, 10) > 16,
            "early on it is left of its place"
        );
        assert_eq!(luma(&done, 150, 10), 16, "in place it starts at 160 px");
        assert!(luma(&done, 170, 10) > 230);
    }

    #[test]
    fn taking_out_and_listing() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::Fade)],
        };
        let control = GraphicsControl::default();
        let first = control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        let second = control.take(render_graphic(&spec, 320, 180).unwrap(), None);

        assert_ne!(first, second);
        assert!(control.take_out(first));
        assert!(!control.take_out(first), "already leaving");
        assert!(!control.take_out(999));

        let on_air = control.on_air();
        assert_eq!(on_air.len(), 2);
        assert!(on_air[0].leaving);
        assert!(!on_air[1].leaving);

        control.take_all_out();
        assert!(control.on_air().iter().all(|graphic| graphic.leaving));
    }

    #[test]
    fn a_graphic_for_another_frame_size_is_not_drawn() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![white_box(0.0, 0.0, GraphicAnimation::None)],
        };
        let control = GraphicsControl::default();
        control.take(render_graphic(&spec, 640, 360).unwrap(), None);

        let mut frame = black_frame(320, 180);
        control.blend(&mut frame);

        assert_eq!(luma(&frame, 10, 10), 16);
    }

    #[test]
    fn a_text_layer_draws_letters_on_its_fill() {
        let spec = GraphicSpec {
            picture: None,
            layers: vec![GraphicLayerSpec {
                content: GraphicContent::Text {
                    text: "NoAr".to_string(),
                    scroll: None,
                    font_family: None,
                    font_size: 20.0,
                    bold: true,
                    color: RgbaColor {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    align: BlockAlign::Left,
                    fill: Some(WHITE),
                },
                ..white_box(10.0, 10.0, GraphicAnimation::None)
            }],
        };
        let control = GraphicsControl::default();
        control.take(render_graphic(&spec, 320, 180).unwrap(), None);

        let mut frame = black_frame(320, 180);
        control.blend(&mut frame);

        let (mut dark, mut light) = (0, 0);

        for y in 18..62 {
            for x in 32..112 {
                match luma(&frame, x, y) {
                    0..=60 => dark += 1,
                    200.. => light += 1,
                    _ => {}
                }
            }
        }

        assert!(light > 1000, "the white fill: {light}");
        assert!(dark > 50, "black letters on it: {dark}");
    }

    #[test]
    fn a_picture_template_is_an_lshape_with_its_layers_as_art() {
        let spec = GraphicSpec {
            layers: vec![GraphicLayerSpec {
                width: 100.0,
                height: 100.0,
                ..white_box(0.0, 0.0, GraphicAnimation::Fade)
            }],
            picture: Some(PictureWindow {
                x: 25.0,
                y: 0.0,
                width: 75.0,
                height: 75.0,
            }),
        };
        let control = GraphicsControl::default();
        let first = control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        let taken = control.state().on_air[0].taken_at;

        assert!(control.picture_on_air());

        let mut frame = black_frame(320, 180);
        control.blend_at(&mut frame, taken + Duration::from_secs(5));
        assert_eq!(
            luma(&frame, 10, 10),
            16,
            "its layers are not drawn over the picture"
        );

        let (graphic, level) = control
            .picture_at(320, 180, taken + Duration::from_millis(600))
            .unwrap();
        assert!(level > 0.4 && level < 0.6, "half way in: {level}");

        let mut frame = black_frame(320, 180);
        frame.data_mut(0).fill(50);
        crate::compositor::graphics::lshape::apply_lshape(
            &mut frame,
            &graphic,
            1.0,
            &mut Default::default(),
        )
        .unwrap();
        assert!(
            luma(&frame, 10, 170) > 230,
            "the art shows around the window"
        );
        assert_eq!(luma(&frame, 200, 60), 50, "the picture fills the window");

        assert!(
            control.picture_at(320, 640, taken).is_none(),
            "another frame size"
        );

        control.take(render_graphic(&spec, 320, 180).unwrap(), None);
        let on_air = control.on_air();
        assert!(
            on_air
                .iter()
                .find(|graphic| graphic.id == first)
                .unwrap()
                .leaving
        );
    }

    /// Writes what a lower third looks like on a synthetic picture, alone
    /// and during an L-shape, as raw YUV 4:2:0 1280x720 files:
    /// `NOAR_GRAPHICS_PREVIEW=<dir> NOAR_GRAPHICS_PREVIEW_LSHAPE=<png>
    /// cargo test -p noar-engine graphics_preview -- --ignored`.
    #[test]
    #[ignore = "writes preview images for people to look at"]
    fn graphics_preview() {
        let dir = PathBuf::from(std::env::var("NOAR_GRAPHICS_PREVIEW").unwrap());
        let lshape_art = std::env::var("NOAR_GRAPHICS_PREVIEW_LSHAPE").unwrap();
        let (width, height) = (1280, 720);
        let color = |r, g, b| RgbaColor { r, g, b, a: 255 };
        let layer = |content, x, y, w, h| GraphicLayerSpec {
            content,
            x,
            y,
            width: w,
            height: h,
            opacity: 1.0,
            radius: 0.0,
            animation_in: GraphicAnimation::None,
            animation_out: GraphicAnimation::None,
            animation_ms: 0,
            delay_ms: 0,
        };
        let text = |text: &str, size, bold, fg, fill| GraphicContent::Text {
            text: text.to_string(),
            scroll: None,
            font_family: None,
            font_size: size,
            bold,
            color: fg,
            align: BlockAlign::Left,
            fill: Some(fill),
        };
        let spec = GraphicSpec {
            picture: None,
            layers: vec![
                layer(
                    GraphicContent::Box {
                        fill: color(243, 146, 35),
                    },
                    6.0,
                    72.0,
                    1.0,
                    14.0,
                ),
                layer(
                    text(
                        "Maria da Silva",
                        5.0,
                        true,
                        color(20, 21, 23),
                        color(255, 255, 255),
                    ),
                    7.0,
                    72.0,
                    40.0,
                    8.0,
                ),
                layer(
                    text(
                        "Produtora rural · Fazenda Boa Vista",
                        3.0,
                        false,
                        color(255, 255, 255),
                        color(20, 21, 23),
                    ),
                    7.0,
                    80.0,
                    40.0,
                    6.0,
                ),
            ],
        };

        let picture = || {
            let mut frame = VideoFrame::new(PixelFormat::Yuv420p, width, height);
            let (ys, us, vs) = (frame.stride(0), frame.stride(1), frame.stride(2));

            for y in 0..height as usize {
                for x in 0..width as usize {
                    let sky = y < height as usize * 55 / 100;
                    let (dx, dy) = (x as f64 - 900.0, y as f64 - 200.0);
                    let sun = dx * dx + dy * dy < 80.0 * 80.0;
                    frame.data_mut(0)[y * ys + x] = if sun {
                        225
                    } else if sky {
                        (170 - y / 8) as u8
                    } else {
                        (90 + (x / 40 % 2) * 12) as u8
                    };
                }
            }

            for y in 0..height as usize / 2 {
                for x in 0..width as usize / 2 {
                    let sky = y < height as usize * 55 / 200;
                    frame.data_mut(1)[y * us + x] = if sky { 150 } else { 100 };
                    frame.data_mut(2)[y * vs + x] = if sky { 110 } else { 115 };
                }
            }

            frame
        };
        let write = |frame: &VideoFrame, name: &str| {
            let mut bytes = Vec::new();

            for plane in 0..3 {
                let (w, h) = if plane == 0 {
                    (width, height)
                } else {
                    (width / 2, height / 2)
                };
                let stride = frame.stride(plane);

                for row in 0..h as usize {
                    bytes.extend_from_slice(
                        &frame.data(plane)[row * stride..row * stride + w as usize],
                    );
                }
            }

            std::fs::write(dir.join(name), bytes).unwrap();
        };

        let control = GraphicsControl::default();
        control.take(render_graphic(&spec, width, height).unwrap(), None);

        let mut alone = picture();
        control.blend(&mut alone);
        write(&alone, "lower-third.yuv");

        let lshape = crate::compositor::graphics::lshape::LShapeGraphic::load(
            &crate::LShapeConfig {
                path: lshape_art,
                scale: 75.0,
                corner: crate::LShapeCorner::TopRight,
            },
            width,
            height,
        )
        .unwrap();
        let mut during = picture();
        control.blend(&mut during);
        crate::compositor::graphics::lshape::apply_lshape(
            &mut during,
            &lshape,
            1.0,
            &mut Default::default(),
        )
        .unwrap();
        write(&during, "lower-third-during-lshape.yuv");
    }
}
