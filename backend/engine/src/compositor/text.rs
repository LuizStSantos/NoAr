//! Text: the fonts, the rendering of text into pictures, and the text overlay
//! of the clips.
//! `fonts` keeps the font system, its glyph cache and the uploaded fonts;
//! `render` draws text into RGBA and YUV pictures; `overlay` is the text drawn
//! over the clips.

mod fonts;
mod overlay;
mod render;

pub(crate) use fonts::{available_font_families, init, register_uploaded_font, uploaded_font_name};
pub use overlay::TextOverlay;
pub use render::BlockAlign;
#[cfg(feature = "desktop-base")]
pub(crate) use render::{TextBitmap, render_left_aligned_text_bitmap, render_wrapped_text_bitmap};
pub(crate) use render::{
    TextStyle, alpha_composite, render_text_block, render_text_line, rgba_to_yuva420p,
};
