//! # oripop-core
//!
//! The 2D drawing API and generative field engine for ori-pop.
//!
//! ## Drawing API
//! Import [`prelude`] and write Processing-style sketches:
//! `size()`, `background()`, `stroke()`, `line()`, `ellipse()`, `run()`, etc.
//!
//! ## Geometry primitives
//! [`Point`], [`Line`], [`Bezier`] — 2D primitives used by the field system.
//! (Future: promoted to 3D types in `oripop-math`.)
//!
//! ## Field engine
//! [`field`] — scalar field evaluation ([`field::Force`], [`field::eval_force`],
//! [`field::field_at`]) and dot distribution ([`field::generate_dots`]).

pub mod bezier;
pub mod draw;
pub mod field;
pub mod line;
pub mod point;
pub mod prelude;

pub use bezier::Bezier;
pub use field::{
    Canvas, Distribution, Dot, Field, Force, Params, Render, Singularity,
    density_at, eval_force, field_at, generate_dots,
};
pub use line::Line;
pub use point::Point;
