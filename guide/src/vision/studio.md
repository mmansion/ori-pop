# Ori Pop Studio

**Status: locked (2026-05-24)**

Ori Pop Studio is the desktop editor for **generative textures** that map onto
surfaces — for 3D assembly preview and physical output such as fabric printing.

The full specification lives in the repository under
[`docs/studio/`](https://github.com/mmansion/ori-pop/tree/main/docs/studio):

| Document | Topic |
|----------|-------|
| [Architecture](https://github.com/mmansion/ori-pop/blob/main/docs/studio/01-architecture.md) | Layers, canvas model, atlas, previz |
| [Workflows](https://github.com/mmansion/ori-pop/blob/main/docs/studio/02-workflows.md) | Exploration, primitives, GH production |
| [Data model](https://github.com/mmansion/ori-pop/blob/main/docs/studio/03-data-model.md) | Library, project, atlas, bake formats |
| [Implementation roadmap](https://github.com/mmansion/ori-pop/blob/main/docs/studio/04-roadmap.md) | Phased build order |

---

## Three kinds of sketches

| | Repo `sketches/` | Texture library | Studio project |
|--|------------------|-----------------|----------------|
| Purpose | Build the framework | Shared exploration | Production on a garment |
| API | Full crate surface | Runtime prelude | Runtime prelude |

---

## Workflows

**Library exploration** — develop textures before any Rhino panels exist
(provisional canvas or built-in sphere/plane).

**Surface-first** — import assembly mesh + atlas from Grasshopper; generative
work on panel layout; preview on the full 3D object; bake for print.

**Library → project** — reference a library design and bind it to atlas regions
when panels exist.

---

## Core rules

- **Desktop** editor; compile and **Play** through `oripop-runtime` (Unity-style).
- **Atlas-first** for production: one raster, many panels; cut lines are
  fabrication boundaries and generative constraints.
- **Rhino/GH** owns unrolling at v0; ORI-POP imports flats and previzes.
- **Bake is additive** — like Grasshopper bake; source code and params stay live.
- **Shared texture library** — designs and bakes reusable across projects.

---

## What we are building first

**Phase 1:** texture library on disk, provisional canvas, Play, and bake to PNG
with a reproducibility manifest. See the
[implementation roadmap](https://github.com/mmansion/ori-pop/blob/main/docs/studio/04-roadmap.md)
for the task list.
