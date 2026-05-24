# Studio Implementation Roadmap

**Status: locked (2026-05-24)**

Phased build order for Ori Pop Studio and the runtime support it requires.
This extends [`ROADMAP.md`](../../ROADMAP.md) §0a–§0c. **Do not reorder phases**
without updating this document and the lock date.

Engine crate work that blocks studio phases is noted inline.

---

## Phase 0 — Foundation (current → near term)

**Goal:** Stable playback boundary and texture primitives the studio will expose.

| Item | Status | Notes |
|------|--------|-------|
| `oripop-project` manifest types | Done | `DesignManifest`, atlas, bake, library, project |
| `oripop-runtime` re-exports `run3d` + prelude | Done | |
| Repo sketches as R&D (`10`, `11`, `7`, …) | In progress | Stipple + field + gen paths |
| `presets/default.json` ↔ `Params` serde | Partial | Formalize schema |
| `ObjectTexture::StippleCanvas` + raster upload | Done | |
| `ObjectTexture::Gen` + compute pass | Done | |
| `oripop-studio` stub binary | Done | |
| Studio architecture docs | Done | This folder |

**Exit criteria:** Can run stipple and gen texture demos via runtime; params
JSON round-trips; docs locked.

---

## Phase 1 — Provisional canvas + library skeleton

**Goal:** W1 workflow — explore textures in a **shared library** without GH or
projects.

### Deliverables

1. **`texture-library/` conventions** — `library.oripop`, `design.oripop` parser
   (JSON, `format_version: 1`).
2. **`oripop-project` crate** — headless manifest types (`DesignManifest`,
   `CanvasKind`, `BakeManifest`); unit tests, no GPU. **Done.**
3. **Provisional canvas adapter** — fixed-size RGBA buffer; stipple/field raster
   at configurable resolution (not only `STIPPLE_CANVAS_SIZE`).
4. **Studio minimal shell** — egui window: library browser, open design, external
   or embedded editor hook, **Play** invokes generated build for one design.
5. **Generated `.oripop/build/` Cargo** — one binary per library design.
6. **Bake to PNG** — raster buffer at canvas resolution + write
   `*.bake.json` (frame, seed, params, `reproducible`).
7. **Primitive previz (optional in P1)** — `primitive_uv` sphere/plane preview
   using existing `run3d` path.

### First implementation tasks (start here)

```
[x] oripop-project crate: manifest structs + serde
[x] CLI or studio stub: load library.oripop, list designs
[x] Generate .oripop/build/Cargo.toml from library design
[x] Play: cargo build + run design binary through oripop-runtime
[x] Bake command: write PNG + bake manifest from stipple buffer
[x] Document params.json schema alongside presets/default.json
```

**Exit criteria:** Create library design → Play → Bake PNG with manifest;
no project or atlas required.

---

## Phase 2 — Project + atlas import + assembly previz

**Goal:** W3 surface-first workflow at minimal fidelity.

### Deliverables

1. **`project.oripop`** loader + project folder template.
2. **Atlas types** — `atlas.oripop`, `fabrication_layout.json`, `panels/*.panel.json`.
3. **Cut lines** — load `cut_lines.json`; expose segment list / SDF to designs.
4. **Atlas canvas adapter** — generative buffer sized to atlas; panel region masks.
5. **glTF assembly import** — `CpuMesh` → scene object(s) for previz.
6. **UV binding** — map baked/live atlas texture to assembly mesh (material slot
   or UV island per panel metadata).
7. **Studio views** — Atlas / Assembly / Split; active texture: Live or selected bake.
8. **Library reference in project** — `library_ref` + optional `params.override.json`.
9. **Manual import path v0** — hand-authored JSON + glb until GH exporter exists.

### Engine tasks

- `ObjectTexture::Baked(Path)` or equivalent static texture binding.
- Mesh import from glTF (minimal: positions, normals, UVs, indices).
- High-resolution bake (atlas pixel size, not only 1024²).

**Exit criteria:** Import sample assembly + atlas JSON; run design on atlas;
preview on 3D mesh; bake at atlas resolution.

---

## Phase 3 — Studio UX + parametric control

**Goal:** Inspector-driven params; bake variants; reproducibility UX.

### Deliverables

1. **Inspector panels** for `params.json` (promote patterns from
   `11-distribution-dial-demo` into studio chrome).
2. **Lock** — capture frame/seed into manifest from UI.
3. **Bake variant strip** — multiple bakes per design; pick active for assembly.
4. **Hot param reload** where feasible without full recompile.
5. **Design templates** — `provisional`, `primitive_uv`, `atlas` scaffolds.

**Exit criteria:** Tune stipple field from inspector; bake three variants; swap
active bake on assembly previz.

---

## Phase 4 — Constraints + continuity

**Goal:** Cut lines as generative boundaries; multi-panel designs; continuity
helpers.

### Deliverables

1. **Cut-line constraint API** for designs (distance to boundary, inside-panel
   mask).
2. **Multi-panel designs** — one design targets `n` panel ids on atlas.
3. **Shared object-space field** sampler (optional 3D field evaluated per UV).
4. **Authoring layout override** — edit `authoring_layout.json`; map to
   fabrication on bake (if needed).

**Exit criteria:** Flocking or stipple demo respects cut-line boundary; seamless
field spans two panels on sample atlas.

---

## Phase 5 — GH export + fabrication package

**Goal:** Tighten Rhino/GH handoff; export print-ready packages.

### Deliverables

1. **GH export script/plugin spec** implemented on GH side (targets schemas in
   [03-data-model.md](./03-data-model.md)).
2. **Bake export bundle** — PNG + manifest + layout metadata for fabric printer.
3. **Optional per-panel PNG export** from same bake (bake setting).
4. **Rhino.Compute** spike for in-framework unroll (research; not blocking).

**Exit criteria:** One real GH export → import → generative pass → bake →
previz without hand-editing layout JSON.

---

## Phase 6 — Advanced (post-lock backlog)

Not part of initial studio MVP; tracked here for alignment:

| Item | Notes |
|------|-------|
| Per-region different recipes on one atlas | Mixed designs |
| Edge matching constraints | Shared panel edges |
| WGSL custom compute in studio designs | Runtime API extension |
| `DesignTree` as project source | Graph editor era |
| Agent-readable project + library index | AI layer |
| In-framework developable unroll | `oripop-fab` |

---

## Crate ownership (target)

```text
oripop-math (or oripop-project)   manifest + atlas types; headless tests
oripop-canvas                     generative kernels; constraint hooks
oripop-3d                         import mesh; Baked texture; assembly scene
oripop-runtime                    Play API; canvas adapters surface here over time
oripop-studio                     egui shell; library/project browsers; bake UI
```

---

## Dependency on repo sketches

| Sketch | Phase consumed |
|--------|----------------|
| `1-hello-ori-pop` | P1 templates |
| `7-textured-3d-demo` | P1 primitive previz |
| `10-curves-3d-demo` | P2 atlas stipple path |
| `11-distribution-dial-demo` | P3 inspector |

Repo sketches remain R&D until patterns are promoted; studio does not duplicate
their prototype UI in user designs.

---

## Lock statement

This roadmap is **locked** as of **2026-05-24**. Implementation begins at
**Phase 1 first tasks**. Changes require explicit revision to this file and
[`README.md`](./README.md) lock date.

---

## Related documents

- [README.md](./README.md) — index
- [01-architecture.md](./01-architecture.md)
- [02-workflows.md](./02-workflows.md)
- [03-data-model.md](./03-data-model.md)
- [`ROADMAP.md`](../../ROADMAP.md) — engine-wide roadmap
