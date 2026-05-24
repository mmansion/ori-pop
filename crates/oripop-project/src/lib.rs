//! # oripop-project
//!
//! Headless types for Ori Pop Studio: texture library, project, atlas, and bake
//! manifests. See [`docs/studio/03-data-model.md`](../../docs/studio/03-data-model.md).
//!
//! No GPU or windowing dependencies.

mod atlas;
mod bake;
mod build_gen;
mod canvas;
mod design;
mod io;
mod layout;
mod library;
mod project;

pub use atlas::{AtlasManifest, CutLinePath, CutLinesManifest, PanelLink, PanelManifest};
pub use bake::{BakeLock, BakeManifest};
pub use build_gen::{generate_library_build, GeneratedBuild};
pub use canvas::{CanvasKind, PrimitiveMesh};
pub use design::{DesignManifest, LibraryRef};
pub use io::TextureLibrary;
pub use layout::{FabricationLayout, PanelRect};
pub use library::LibraryManifest;
pub use project::{DesignEntry, LibraryRefEntry, ProjectManifest};

/// Current schema version for all studio manifest files.
pub const FORMAT_VERSION: u32 = 1;
