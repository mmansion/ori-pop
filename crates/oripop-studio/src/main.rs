//! Ori Pop Studio — interactive control surface (Unity-style inspector chrome, project UI).
//!
//! **Stub:** no window yet. The studio process stays separate from *player*
//! concerns at the **crate** level: [`oripop_runtime`] is the shared playback
//! boundary; this binary will grow egui panels and Play orchestration without
//! owning GPU pass implementations (those stay in `oripop-3d` until split).

fn main() {
    eprintln!(
        "oripop-studio {} — stub shell (editor UI and Play embedding not wired yet).",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!("Sketches: cargo run -p sketches --bin <name>");
    eprintln!("Runtime API crate: oripop-runtime (re-exports oripop-3d prelude today).");
}
