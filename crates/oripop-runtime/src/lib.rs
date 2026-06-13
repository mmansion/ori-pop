//! Sketch playback and frame orchestration for Ori Pop.
//!
//! This crate is the **intended home** for the highly optimized runtime loop
//! (timing, input dispatch, GPU pass scheduling) shared by **Play** in the
//! studio and **standalone player** builds. Today it is a **thin facade** over
//! `oripop_3d`: types and entrypoints re-export so the `oripop-studio` binary and
//! future standalone **player** binaries depend on **runtime**, not on the GPU
//! implementation crate directly.

pub use oripop_3d::{run3d, run_sketch};

/// Alias for [`run_sketch`] — Processing-style entry point.
pub use oripop_3d::run_sketch as run;

/// Re-exports for sketch and player code: canvas drawing + 3D scene API.
pub mod prelude {
    pub use oripop_3d::prelude::*;
    pub use crate::{run, run3d, run_sketch};
}
