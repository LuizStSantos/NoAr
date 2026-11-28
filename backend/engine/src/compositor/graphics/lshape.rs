//! The L-shape: the picture shrinks into a corner of the screen, over a
//! full-screen graphic that fills the band around it (news, sponsors, the
//! next programme), and grows back when it ends. It is switched from outside
//! the playout, like the logo, or by stretches marked in the video itself.
//! `shape` holds its timing, corners and window; `control` what is on air;
//! `apply` draws it into the frames.

mod apply;
mod control;
mod shape;

pub(crate) use apply::{LShapeScaler, apply_lshape, marked_level};
pub use control::{LShapeControl, LShapePlacement};
pub use shape::{
    LSHAPE_LOGO_SECONDS, LSHAPE_MAX_SCALE, LSHAPE_MIN_SCALE, LSHAPE_SECONDS, LSHAPE_SWITCH_SECONDS,
    LShapeConfig, LShapeCorner, LShapeGraphic, lshape_phases,
};
