//! # oripop-project
//!
//! Headless types for Ori Pop Studio: project, texture, atlas, and bake
//! manifests. See [`docs/studio/03-data-model.md`](../../docs/studio/03-data-model.md).
//!
//! No GPU or windowing dependencies.

mod atlas;
mod bake;
mod canvas;
mod layout;
mod project;
mod texture;

pub use atlas::{AtlasManifest, CutLinePath, CutLinesManifest, PanelLink, PanelManifest};
pub use bake::{BakeLock, BakeManifest};
pub use canvas::{CanvasKind, PrimitiveMesh};
pub use layout::{FabricationLayout, PanelRect};
pub use project::{Project, ProjectManifest, TextureEntry};
pub use texture::TextureManifest;

/// Current schema version for all studio manifest files.
pub const FORMAT_VERSION: u32 = 1;
