# AGENTS.md — working on ORI-POP

This file orients coding agents and contributors to the **current** repository layout, boundaries, and commands. Vision, aesthetics, and long-range architecture live in [`PHILOSOPHY.md`](PHILOSOPHY.md) and [`ROADMAP.md`](ROADMAP.md). The narrative guide is built from [`guide/`](guide/) with mdBook.

---

## What this project is

ORI-POP is a **personal** Rust workspace for generative art and creative coding: a small set of library crates plus runnable **sketches**. It is intentionally **not** a general-purpose framework for external users. Prefer minimal, coherent changes that match existing patterns over speculative APIs or large refactors unless explicitly requested.

Published guide: https://mmansion.github.io/ori-pop/

---

## Workspace layout (as built)

| Path | Crate / role |
|------|----------------|
| `crates/oripop-math` | Shared math and design-tree types. **Must stay free of GPU and windowing** (`wgpu`, `winit`, `egui` do not belong here). Uses `glam`, `serde`, `serde_json`, `ron`. Intended to remain headless and easy to test. |
| `crates/oripop-canvas` | Creative engine kernel: Processing-style 2D drawing API, scalar fields, stipples (includes `wgpu` / `winit` for the sketch API). |
| `crates/oripop-3d` | Real-time 3D: `wgpu`, `egui`, windowing, scene and camera. Depends on `oripop-canvas` and `oripop-math`. |
| `sketches/` | Binary demos and experiments. Depends on `oripop-canvas` and `oripop-3d`. Appropriate place for one-off exploration. |
| `guide/` | mdBook source for the ORI-POP Guide. |
| `presets/` | JSON presets (for example `presets/default.json`). |

Root [`Cargo.toml`](Cargo.toml) is the workspace manifest (Rust 2021, `resolver = "2"`).

### Planned vs present

[`PHILOSOPHY.md`](PHILOSOPHY.md) describes a **future** stack (for example fabrication and evolution crates). Those crates are **not** in this workspace until they appear under `crates/` and in `[workspace.members]`. When adding or referencing crates, treat the root `Cargo.toml` and the `crates/` directory as the source of truth.

---

## Coordinates and space

The codebase standard is **Z-up, right-handed** world space (X right, Y forward, Z up), aligned with CAD, robotics, and common exchange formats. See module docs in `oripop-math` and `oripop-3d` (for example `camera.rs`, `scene.rs`) for the authoritative wording.

---

## Commands

Build the whole workspace:

```bash
cargo build --workspace
```

Run unit tests (present under `oripop-math` and `oripop-canvas`):

```bash
cargo test --workspace
```

Run a sketch (package `sketches`):

```bash
cargo run -p sketches --bin hello-ori-pop
```

Other sketch binaries are declared in [`sketches/Cargo.toml`](sketches/Cargo.toml), for example: `primitives-demo`, `transform-demo`, `alpha-demo`, `forces-demo`, `interactive-demo`, `curves-demo`, `curves-3d-demo`, `textured-3d-demo`, `lsystem-3d`.

Build the guide locally (requires [mdBook](https://rust-lang.github.io/mdBook/)):

```bash
mdbook build guide
```

Output is written to `guide/book/`.

---

## Continuous integration

GitHub Actions currently builds and deploys the guide from [`/.github/workflows/book.yml`](.github/workflows/book.yml) on pushes to `main`. There is no workspace-wide `cargo clippy` / `cargo fmt` gate in CI yet; if you change Rust code, run `cargo fmt` and `cargo clippy` locally when practical.

---

## Where to read next

| Document | Use when |
|----------|-----------|
| [`README.md`](README.md) | One-line pitch and quickest entry command. |
| [`PHILOSOPHY.md`](PHILOSOPHY.md) | Intent, output spectrum, interface vision, UV-first generative ideas. |
| [`ROADMAP.md`](ROADMAP.md) | Status of major directions and technical decisions. |
| [`guide/src/`](guide/src/) | Long-form guide chapters alongside the published site. |

When behavior or file paths conflict between this `AGENTS.md` and older prose, prefer **this file and the live tree** for “what exists now,” and update `AGENTS.md` when the workspace changes in a durable way.
