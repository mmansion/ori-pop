# ORI-POP Studio — Architecture & Plan

**Status: locked (2026-05-24)**

This folder is the authoritative specification for **Ori Pop Studio**: the desktop
texture-authoring environment, its project and library model, and the phased
implementation plan. It extends [`ROADMAP.md`](../../ROADMAP.md) §0a–§0c and
[`PHILOSOPHY.md`](../../PHILOSOPHY.md).

When studio behavior or on-disk formats are discussed, prefer these documents
over informal notes or older prose.

---

## Documents

| Document | Contents |
|----------|----------|
| [01-architecture.md](./01-architecture.md) | System layers, canvas model, generative core, previz, continuity strategies |
| [02-workflows.md](./02-workflows.md) | Surface-first, texture-first, library exploration, bake & preview loops |
| [03-data-model.md](./03-data-model.md) | Library vs project layout, atlas, panels, references, bake manifests |
| [04-roadmap.md](./04-roadmap.md) | **Locked build order** — phases, deliverables, first implementation tasks |
| [params-schema.md](./params-schema.md) | `params.json` field reference (v0) |

---

## Summary

**Ori Pop Studio** is a desktop editor (not browser-based) for generative
textures that map onto surfaces — for screen previz and physical fabrication
(fabric print, cut patterns, etc.).

Three distinct code populations coexist:

| Population | Location | Purpose |
|------------|----------|---------|
| **Framework sketches** | Repo `sketches/` | R&D: grow `oripop-canvas`, `oripop-3d`, runtime APIs |
| **Library designs** | User `texture-library/` | Shared explorations, presets, baked variants — pulled into any project |
| **Project designs** | Studio project folder | Production: bound to GH atlas + assembly, project-specific bakes |

Generative logic is **Rust** targeting a **version-pinned runtime API**
(`oripop-runtime`). The studio compiles and **Play**s designs in-process
(Unity-style). **Bake** is additive (Grasshopper-style): code and params are
never replaced; bakes are output artifacts for previz and print.

**Rhino / Grasshopper** remains the authoritative unroller for fabrication flats
(v0). ORI-POP imports the **3D assembly** and **atlas** (panels + cut lines),
previews the finished object in `oripop-3d`, and runs generative work on an
**atlas canvas** where cut lines are both fabrication metadata and generative
constraints.

---

## Relationship to the engine roadmap

| Engine item | Studio dependency |
|-------------|-------------------|
| `oripop-canvas` — fields, stipples, raster | Texture generative core |
| `oripop-3d` — mesh, UV, texture binding | Assembly previz |
| `oripop-math` — `CpuMesh`, `Surface`, import | Atlas ↔ mesh mapping |
| `oripop-runtime` — Play boundary | All studio designs compile against this |
| `oripop-studio` — egui shell, browser, bake UI | Implementation target |
| Repo `sketches/` | Prototypes promoted into runtime or studio chrome |

See [04-roadmap.md](./04-roadmap.md) for what to build first.
