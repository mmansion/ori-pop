//! # oripop-canvas
//!
//! The 2D drawing API and generative field engine for ori-pop.
//!
//! ## Drawing API
//! Import [`prelude`] and write Processing-style sketches. The core surface
//! covers shapes/arcs/curves/custom shapes with contours, transforms and
//! style stacks, RGB/HSB color with [`draw::Color`] + `lerp_color`, math /
//! seeded random / Perlin noise ([`math`]), polled input plus registered
//! event handlers, a persistent canvas (`background()` clears, otherwise
//! content accumulates; translucent washes fade trails), offscreen
//! [`graphics::Graphics`] canvases, and high-res PNG snapshots
//! (`pixel_density` + `save_frame`).
//!
//! ## Geometry primitives
//! [`Point`], [`Line`], [`Bezier`] — 2D primitives used by the field system.
//!
//! ## Field engine
//! [`field`] — scalar field evaluation ([`field::Force`], [`field::eval_force`],
//! [`field::field_at`]) and dot distribution ([`field::generate_dots`]).
//!
//! ## Dynamic linking
//! Built as both an `rlib` (for standalone bins) and a `dylib` (so the studio
//! and dynamically-loaded texture cartridges share the same thread-local draw
//! state at runtime). When Cargo builds an executable that links a `dylib`,
//! the resulting binary requires the same shared library at runtime.

pub mod bezier;
pub mod cartridge;
pub mod draw;
pub mod field;
pub mod graphics;
pub mod math;
pub mod line;
pub mod point;
pub mod prelude;

pub use bezier::{Bezier, DensityProfile};
pub use field::{
    density_at, eval_force, field_at, generate_dots,
    Canvas, Distribution, Dot, Field, Force, Params, Render, Singularity,
};
pub use draw::{CanvasFormat, DrawFrame, ResolvedCanvasFormat};
pub use line::Line;
pub use point::Point;
