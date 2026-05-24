# Studio Workflows

**Status: locked (2026-05-24)**

This document describes the primary user workflows Ori Pop Studio supports.
All workflows share the same generative core, bake semantics, and runtime Play
path; they differ in **where geometry comes from** and **when canvas binding
happens**.

---

## Workflow overview

```text
                    ┌──────────────────────┐
                    │  Texture Library      │
                    │  (shared, cross-      │
                    │   project)            │
                    └──────────┬───────────┘
                               │
         Texture-first         │         Surface-first
         (no GH yet)           │         (GH upstream)
              │                │                │
              ▼                ▼                ▼
     provisional /      reference library    import assembly
     primitive canvas     design into         + atlas from GH
              │           project                  │
              │                │                │
              └────────────────┴────────────────┘
                               │
                               ▼
                    generative design (Play)
                               │
                    ┌──────────┴──────────┐
                    ▼                     ▼
              lock (optional)      parametric tune
                    │                     │
                    └──────────┬──────────┘
                               ▼
                    bake variants (additive)
                               │
                               ▼
                    assembly previz / print export
```

---

## W1 — Library exploration (no target canvas)

**Goal:** Develop generative texture ideas before any garment or GH panels exist.

**Canvas:** `none` / **provisional** — fixed-resolution buffer or normalized
`[0,1]²` space; optional **primitive** 3D preview (sphere, plane).

**Location:** User **texture library** (`texture-library/designs/…`), not inside
a project.

**Steps:**

1. Create library design from template (`provisional` or `primitive_uv`).
2. Edit `main.rs` + `params.json`; **Play** in studio.
3. Iterate — parametric sliders and/or time-based simulation.
4. **Lock** optional stopping point (frame, seed) when reproducibility matters.
5. **Bake** one or more variants into `texture-library/bakes/…`.
6. Tag and organize for later retrieval.

**Output:** Reusable **library design** + optional **baked variants**. No
fabrication layout yet.

**Later:** Reference this design from a production project; bind to atlas regions
when GH export arrives.

---

## W2 — Texture-first (framework primitives)

**Goal:** Explore how a pattern reads on curved or simple 3D form before
committing to real panels — or discover texture ideas that **inform** form design
later.

**Canvas:** **`primitive_uv`** — built-in `MeshKind` (sphere, plane, cylinder, …).

**Location:** Library or project.

**Steps:**

1. Pick primitive + resolution.
2. **Play** generative design; orbit in **Assembly** view (primitive mesh).
3. Bake variants for comparison.
4. Optionally **promote to library** if started inside a project.

**Output:** Design + bakes portable to atlas via **retarget** (same params/logic,
new canvas adapter).

**Relation to W1:** W2 adds meaningful **3D previz on framework geometry**; W1
may stay purely 2D provisional.

---

## W3 — Surface-first (Rhino / Grasshopper production)

**Goal:** Author generative textures on **real unrolled panels** and preview on
the **full assembly** for fabrication (e.g. fabric print).

**Upstream (GH):**

1. Model surfaces / garment panels.
2. Unroll → panel flats with physical dimensions.
3. Export **assembly mesh**, **atlas layout**, **cut lines**.

**Studio:**

1. Create or open **project**; import assembly + atlas.
2. Create **project design** (or **reference library design**) bound to atlas
   regions (`n` panels).
3. Generative logic respects **panel masks** and **cut-line constraints**.
4. **Play** — edit in **Atlas** view; check seams in **Split** / **Assembly**
   view.
5. **Bake** at print resolution (fabrication layout); accumulate variants for
   evolving patterns.
6. Select active bake for assembly previz; export print package.

**Output:** Project bakes tied to `fabrication_layout`; assembly previz; print
assets.

**Unrolling:** Rhino/GH authoritative at v0. Future: Rhino.Compute or Rust
developable unroll for selected cases.

---

## W4 — Library → project (retarget)

**Goal:** Reuse exploration work on a production garment.

**Steps:**

1. In project, **Insert from library** — reference `library://designs/…`.
2. Studio binds design to **atlas panel regions** + cut lines.
3. Optional **`params.override.json`** at project level (garment-specific tweaks
   without forking library source).
4. **Play** / **bake** in project context; bakes stored under **project**
   `baked/`, not library.

**Copy vs reference:**

| Action | When |
|--------|------|
| **Reference** | Default — library remains source of truth |
| **Copy into project** | User forks code for garment-specific changes |

---

## W5 — Continuous bake evaluation (evolving patterns)

**Goal:** Patterns that keep changing over time (simulation, noise animation)
need **many snapshots** for comparison — without destroying the live source.

**Steps:**

1. Run design live (no lock required).
2. **Bake** repeatedly → `baked/run-0847.png`, `baked/run-1203.png`, …
3. Assembly view **cycles active bake** or shows comparison strip.
4. Optional **lock** + manifest when one variant is chosen for reproduction.

Compatible with W1–W3. Bake remains **additive**; source always runnable.

---

## Workflow comparison

| | W1 Explore | W2 Primitive | W3 Surface-first | W4 Retarget |
|--|------------|--------------|------------------|-------------|
| Canvas | Provisional | Primitive UV | Atlas | Atlas |
| GH required | No | No | Yes | At project step |
| Library | Primary home | Optional | Optional source | Library → project |
| 3D previz | Optional | Built-in mesh | Imported assembly | Imported assembly |
| Print-ready bake | No | No | Yes | Yes |

---

## Studio UI modes (conceptual)

| Mode | Primary affordances |
|------|---------------------|
| **Library** | Global design browser; tags; new exploration; no project open |
| **Project** | Assembly + atlas import; design list; library refs used here |
| **Play** | Compile + run active design through runtime |
| **Atlas editor** | Panel outlines, cut lines, layout (fabrication vs authoring later) |
| **Assembly** | 3D previz; live vs baked texture selector |
| **Bake** | Export raster + manifest; variant history |

---

## Generative control patterns

### Parametric (inspector-driven)

- `params.json` mirrors `oripop_canvas::Params` and related structs (see repo
  `presets/default.json`).
- Studio inspector edits params; hot-reload where possible without recompile.
- Bake when visual result is acceptable.

### Iterative (time-driven)

- Draw loop uses `scene.time` / `frame_count`.
- User **locks** at frame *N* when reproducible.
- Bake captures locked state in manifest.

### Constrained (atlas-aware)

- Design receives **region mask** + **cut-line SDF** from atlas adapter.
- Use for flocking boundaries, exclusion zones, diffusion walls.

---

## What GH / Rhino users need to provide (W3 checklist)

- [ ] Assembly mesh export (glTF/GLB) with consistent UV islands per panel
- [ ] Atlas image dimensions + DPI / physical scale
- [ ] Panel rectangles in atlas space (`fabrication_layout.json`)
- [ ] Cut line geometry in atlas space
- [ ] Stable panel ids linking layout ↔ mesh UV ↔ metadata

Exact schema: [03-data-model.md](./03-data-model.md).

---

## Next

Implementation order: [04-roadmap.md](./04-roadmap.md).
