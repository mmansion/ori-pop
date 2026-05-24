# Studio Architecture

**Status: locked (2026-05-24)**

This document defines the structural architecture of Ori Pop Studio: layers,
canvas model, generative core, assembly previz, and the continuity strategies
the framework keeps available.

---

## 1. Product shape

Ori Pop Studio is a **desktop** texture-authoring environment built on the
ori-pop engine (`oripop-runtime` → `oripop-3d` + `oripop-canvas`).

It is **not** a general game engine or a replacement for Rhino. It is the place
where generative textures are authored, iterated, baked, and previewed on
surfaces — ultimately for fabrication (fabric print, scored sheet, 3D material
preview, etc.).

The studio exposes only APIs present in a **pinned engine version**. User
designs compile and run inside the editor (Unity-style **Play**). The repo
`sketches/` crate remains separate: framework development, not user projects.

---

## 2. System layers

```text
┌─────────────────────────────────────────────────────────────────┐
│  oripop-studio                                                  │
│  Library browser · Project browser · Atlas editor · Bake UI     │
│  Code editor · Play orchestration · Assembly previz             │
└────────────────────────────┬────────────────────────────────────┘
                             │ depends on
┌────────────────────────────▼────────────────────────────────────┐
│  oripop-runtime (Play / playback API — versioned contract)       │
└────────────────────────────┬────────────────────────────────────┘
                             │
        ┌────────────────────┼────────────────────┐
        ▼                    ▼                    ▼
  oripop-canvas        oripop-3d           oripop-math
  fields · stipples    wgpu · scene · UV   CpuMesh · Surface
  raster · 2D API      texture binding    import · layout types
```

**Dependency rule (unchanged from engine roadmap):**  
`oripop-studio` → `oripop-runtime` → GPU host. The studio orchestrates; it
does not own inner GPU pass implementations.

---

## 3. Three populations of “sketches”

| Name | Where | Role |
|------|-------|------|
| **Framework sketch** | Repo `sketches/` | Prove APIs, prototype UI, stress-test engine; may use any public crate surface |
| **Library design** | User `texture-library/` | Reusable generative recipes; exploration before a garment project exists |
| **Project design** | Studio project `designs/` | Production work bound to an assembly atlas and/or library references |

Framework sketches **build** the engine. Library and project designs **consume**
the runtime API under editor constraints.

---

## 4. Canvas model

A **canvas** is the 2D domain where generative logic runs before results are
bound to 3D or baked for print. Canvas binding is a **stage**, not a prerequisite
for starting work.

### 4.1 Canvas kinds

| Kind | Description | Typical use |
|------|-------------|-------------|
| **`none` / provisional** | Normalized buffer (e.g. unit square or fixed pixel rect); no GH panels | Open exploration; library designs |
| **`primitive_uv`** | UV parameterization of built-in mesh (`plane`, `sphere`, `cylinder`, …) | Texture-first studies; quick 3D previz on framework shapes |
| **`atlas`** | One raster with **n panel regions** + **cut lines** | Production; surface-first workflow |

Designs declare canvas kind in metadata. The generative core (fields, stipples,
future WGSL) writes to a buffer; the **canvas adapter** supplies coordinate
transforms, masks, and constraints.

### 4.2 Canvas lifecycle

```text
Library exploration (provisional / primitive)
        │
        ├─► stay in library (shared across projects)
        │
        └─► referenced by project ──► bind to atlas regions
                                          │
                                          ├─► live preview on assembly
                                          └─► bake → print / previz assets
```

A design may begin with **no target canvas**, live in the **shared texture
library**, and later be **referenced** by a project when GH panels exist.

---

## 5. Atlas (primary production canvas)

**v0 strategy: atlas-first.** Multiple panels are packed into **one raster** for
a single generative pass. Panel boundaries and **cut lines** are first-class
geometry — not just export metadata.

### 5.1 Layout modes

Two layout concepts coexist:

| Layout | Source | Purpose |
|--------|--------|---------|
| **Fabrication layout** | Imported from Rhino/GH | Canonical for print: pixel ↔ physical mm; cut lines match unroll |
| **Authoring layout** | Optional studio override | Reposition panels on the atlas for generative convenience |

**v0 default:** authoring layout **equals** fabrication layout (no warp step).

**Later:** authoring layout may diverge; bake-for-print resolves through
fabrication layout (direct or mapped from authoring space).

### 5.2 Cut lines — dual role

1. **Fabrication** — cut, score, sew boundaries; travel with GH export.
2. **Generative constraints** — boundaries for simulations and fields (e.g.
   boids cannot cross a cut line; diffusion respects panel edges; stipple
   exclusion zones).

The generative API should expose **region masks** and **cut-line distance
fields** so designs do not re-parse SVG per sketch.

### 5.3 One design → n flats

A single **project design** targets **one or many panel regions** on one atlas
(`n ≥ 1`). Seamless appearance across the full object is achieved by one or
more **continuity strategies** (§7) — not by forcing one panel per design file.

Export may produce **one atlas image** or **n per-panel images** depending on
print workflow; that is a **bake setting**, not a design-structure fork.

---

## 6. External geometry (Rhino / Grasshopper)

### 6.1 Division of responsibility

| Task | Owner (v0) | Notes |
|------|------------|-------|
| Surface modeling | Rhino / GH | Authoritative |
| Unroll → flats / panels | Rhino / GH | Authoritative for fabrication |
| Cut lines | Rhino / GH | Exported with atlas |
| 3D assembly mesh + UV | Export → glTF/GLB | Previz rig |
| Generative texture | ORI-POP Studio | Atlas canvas |
| Assembly previz | ORI-POP (`oripop-3d`) | Baked or live texture on imported mesh |
| In-framework unroll | **Future** | Simple developables in Rust; complex panels via **Rhino.Compute** when needed |

ORI-POP does **not** need to flatten panels correctly at v0 to be useful. It
needs a reliable **import contract** (§6.2) and **UV ↔ panel mapping** for
previz and bake.

### 6.2 Import contract (conceptual)

Each project assembly import provides:

- **`assembly.glb`** — tessellated mesh, UVs, optional per-panel material slots
- **`fabrication_layout.json`** — panel rectangles on atlas (pixel or normalized)
- **`cut_lines`** — paths in atlas space (SVG or JSON)
- **Per-panel metadata** — id, physical width/height (mm), link to mesh UV island

Studio validates that panel regions tile the atlas and reference valid mesh
regions for previz.

---

## 7. Continuity strategies (all retained)

Multiple approaches remain available; a design declares which apply:

| Strategy | Mechanism | Best for |
|----------|-----------|----------|
| **Shared object-space field** | Sample `f(x,y,z,t)` in world space; each panel reads via surface UV | Seamless spans-format; exploration before flats exist |
| **Atlas + regions** | Single raster; generative pass aware of panel rects | v0 production default |
| **Cut-line constraints** | Hard/soft boundaries in generative logic | Flocking, diffusion, masks |
| **Per-region recipes** | Different params or sub-designs per panel on same atlas | Mixed designs on one garment (later) |
| **Edge matching** | Explicit continuity at shared edges | When layout splits adjacent panels (later) |

These are composable. v0 implements **atlas + regions + cut-line constraints**;
others follow as the generative API matures.

---

## 8. Generative core (engine-facing)

User designs call the **runtime prelude** — today centered on `oripop-canvas`:

- Scalar **fields** and **forces** (`Params`, `generate_dots`, …)
- **Stipple raster** → RGBA buffer (`StippleCanvas` path in `oripop-3d`)
- GPU procedural textures (`ObjectTexture::Gen`, `TextureGenParams`)
- Future: WGSL compute stages, surface-aware uniforms (roadmap Level 2–3)

The studio adds **canvas adapters** (provisional, primitive, atlas) and **bake**
at target resolution — not a second generative engine.

### 8.1 Two paths to a stopping point

| Mode | Control | Lock / bake |
|------|---------|-------------|
| **Iterative** | Time / simulation steps in draw loop | Lock at frame *N*; bake snapshot |
| **Parametric** | Sliders / inspector on `params.json` | Bake for frame; bake when visually correct |

Both produce **additive bakes** (§9).

---

## 9. Bake semantics

**Bake is additive output — not a mode switch** (Grasshopper-style).

| Artifact | After bake |
|----------|------------|
| `main.rs` |  `params.json` | Unchanged; still editable |
| Live preview | Still runs from source |
| New bake file | PNG/TIFF (+ sidecar manifest) appended to `baked/` |

For **evolving** patterns, the user may **bake repeatedly** to accumulate
variants (`variant-a.png`, `variant-b.png`, …). Assembly previz can display
**Live**, **Bake v1**, **Bake v2**, etc., for side-by-side evaluation.

Bake manifests record reproducibility when possible (see
[03-data-model.md](./03-data-model.md)).

---

## 10. Assembly previz

The finished object is visualized by applying textures to the **imported 3D
assembly** in `oripop-3d`:

```text
assembly.glb  +  active texture (live buffer or selected bake)  +  UV mapping
        ──►  orbit / inspect in studio Assembly view
```

**Views:**

| View | Shows |
|------|-------|
| **Atlas** | 2D generative canvas, panel outlines, cut lines |
| **Assembly** | Textured 3D mesh |
| **Split** | Both (recommended for seam checking) |

Previz does not require Rust to unroll panels. It requires correct **UV ↔ panel
↔ atlas region** linkage from import.

---

## 11. Shared texture library

Exploration often happens **before** a garment project exists. Reusable work
lives in a **user-level texture library** (outside any project):

- **Designs** — `main.rs` + `params.json` + metadata
- **Presets** — params-only snippets
- **Bakes** — exploration snapshots + manifests

Projects **reference** library entries (with optional param overrides) rather
than copying by default. See [03-data-model.md](./03-data-model.md).

---

## 12. Repo sketches vs studio (permanent split)

| | Repo `sketches/` | Studio designs |
|--|------------------|----------------|
| Cargo | Hand-maintained in workspace | Generated under `.oripop/build/` per project |
| API | Full crate surface for R&D | Runtime prelude only |
| Audience | Framework authors | Texture authors |
| Promotion | Patterns lift into crates or studio UI | N/A |

Examples of current repo prototypes and their studio destiny:

| Repo sketch | Promotes to |
|-------------|-------------|
| `11-distribution-dial-demo` | Inspector / param UI in studio |
| `10-curves-3d-demo` | Stipple → atlas raster pipeline |
| `7-textured-3d-demo` | Primitive UV + `Gen` texture template |
| `presets/default.json` | Preset / params schema for library + projects |

---

## 13. Coordinate convention

Unchanged: **Z-up, right-handed** throughout engine and imports. Assembly previz
and future fabrication exports assume the same convention as Rhino/GH when
exports are configured accordingly.

---

## 14. Document map

- Workflows: [02-workflows.md](./02-workflows.md)
- On-disk formats: [03-data-model.md](./03-data-model.md)
- Build phases: [04-roadmap.md](./04-roadmap.md)
