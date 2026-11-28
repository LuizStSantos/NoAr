//! Compatibility logo controls over a layered Graphics model.
//!
//! External placement/replacement, clip overrides and fade state keep one runtime
//! handle; authored pixels are prepared by the shared Graphics renderer.
//! `overlay` loads, scales and blends the logo; `position` reads its size and
//! placement expressions; `control` keeps what is on air and its transitions.

mod control;
mod overlay;
mod position;

pub use control::{
    LogoControl, LogoState, LogoTransition, LogoTransitions, MAX_LOGO_TRANSITION_SECONDS,
};
pub use overlay::{LogoOverlay, blend_logo};
#[cfg(feature = "desktop-base")]
pub(crate) use position::logo_position;
