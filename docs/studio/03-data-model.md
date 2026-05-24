# Studio Data Model

**Status: locked (2026-05-24)**

On-disk layout, reference formats, and sidecar schemas for the texture library,
studio projects, atlas imports, and bakes. Schema **`format_version`** fields
allow migration as the studio evolves.

JSON is used for interchange (agents, GH plugins, web tools). RON may mirror
`DesignTree` conventions where appropriate.

---

## 1. Scope hierarchy

```text
texture-library/          ← user-level; shared across all projects
    designs · presets · bakes

MyGarment/                ← studio project
    assembly · atlas · designs · baked · .oripop/build/
```

Projects **reference** library entries; they do not embed copies unless the user
explicitly forks.

---

## 2. Texture library

Default location: user-configurable; convention `~/ori-pop-library/` or
`texture-library/` adjacent to projects.

```text
texture-library/
├── library.oripop
├── designs/
│   └── coral-field-v2/
│       ├── design.oripop
│       ├── main.rs
│       └── params.json
├── presets/
│   └── dense-focal.json
└── bakes/
    └── coral-field-v2/
        ├── 2026-05-24T14-12-00.png
        └── 2026-05-24T14-12-00.bake.json
```

### 2.1 `library.oripop`

```json
{
  "format_version": 1,
  "engine_version": "0.1.0",
  "title": "MANSION texture library",
  "designs": [
    { "id": "coral-field-v2", "path": "designs/coral-field-v2" }
  ],
  "presets": [
    { "id": "dense-focal", "path": "presets/dense-focal.json" }
  ]
}
```

### 2.2 `design.oripop` (library or project)

```json
{
  "format_version": 1,
  "id": "coral-field-v2",
  "title": "Coral field study",
  "engine_version": "0.1.0",
  "canvas": {
    "kind": "provisional",
    "width": 1024,
    "height": 1024
  },
  "tags": ["organic", "stipple"],
  "params": "params.json",
  "entry": "main.rs"
}
```

**`canvas.kind` values:**

| Value | Meaning |
|-------|---------|
| `provisional` | Fixed pixel buffer; no panels |
| `primitive_uv` | `{ "kind": "primitive_uv", "mesh": "sphere" }` |
| `atlas` | `{ "kind": "atlas", "atlas_ref": "atlas/atlas.oripop", "panels": ["sleeve-left", "sleeve-right"] }` |

Project-local designs add optional **`library_ref`** when forked from library:

```json
{
  "library_ref": {
    "uri": "library://designs/coral-field-v2",
    "params_override": "params.override.json"
  }
}
```

### 2.3 `params.json`

Aligns with `oripop_canvas` field/distribution structs (see repo
`presets/default.json`). Studio inspector reads/writes this file.

### 2.4 Library URI scheme

| Form | Example |
|------|---------|
| `library://designs/<id>` | Design entry |
| `library://presets/<id>` | Preset only |
| `library://bakes/<design-id>/<filename>` | Specific bake |

Studio resolves against configured library root path.

---

## 3. Studio project

```text
MyGarment/
├── project.oripop
├── assembly/
│   └── garment.glb
├── atlas/
│   ├── atlas.oripop
│   ├── fabrication_layout.json
│   ├── authoring_layout.json       ← optional; defaults to fabrication
│   ├── cut_lines.json              ← or cut_lines.svg
│   └── panels/
│       ├── sleeve-left.panel.json
│       └── sleeve-right.panel.json
├── designs/
│   └── field-span-v1/
│       ├── design.oripop
│       ├── main.rs
│       ├── params.json
│       └── params.override.json    ← optional
├── baked/
│   └── field-span-v1/
│       ├── 2026-05-24T14-12-00.png
│       └── 2026-05-24T14-12-00.bake.json
└── .oripop/
    └── build/                      ← generated Cargo; studio-managed
```

### 3.1 `project.oripop`

```json
{
  "format_version": 1,
  "engine_version": "0.1.0",
  "title": "Spring garment",
  "created": "2026-05-24T12:00:00Z",
  "default_design": "field-span-v1",
  "assembly": "assembly/garment.glb",
  "atlas": "atlas/atlas.oripop",
  "designs": [
    { "id": "field-span-v1", "path": "designs/field-span-v1" }
  ],
  "library_refs": [
    { "design_id": "field-span-v1", "uri": "library://designs/coral-field-v2" }
  ]
}
```

---

## 4. Atlas

### 4.1 `atlas.oripop`

```json
{
  "format_version": 1,
  "width_px": 8192,
  "height_px": 4096,
  "dpi": 300,
  "physical_width_mm": 700.0,
  "physical_height_mm": 350.0,
  "fabrication_layout": "fabrication_layout.json",
  "authoring_layout": "authoring_layout.json",
  "cut_lines": "cut_lines.json",
  "panels_dir": "panels/"
}
```

**v0:** If `authoring_layout` is omitted, it equals `fabrication_layout`.

### 4.2 `fabrication_layout.json`

Panel rectangles in **atlas pixel space** (origin top-left, matching generative
canvas convention unless documented otherwise):

```json
{
  "format_version": 1,
  "panels": [
    {
      "id": "sleeve-left",
      "x": 120,
      "y": 80,
      "width": 2048,
      "height": 3072,
      "rotation_deg": 0
    }
  ]
}
```

### 4.3 `authoring_layout.json`

Same schema as fabrication layout. When present and different, generative
sketches read constraints from **authoring** space; **bake-for-print** uses
**fabrication** space (v0: identical).

### 4.4 `panels/<id>.panel.json`

Links panel to mesh previz and physical fabrication:

```json
{
  "format_version": 1,
  "id": "sleeve-left",
  "title": "Left sleeve",
  "physical_width_mm": 450.0,
  "physical_height_mm": 680.0,
  "mesh": {
    "material_slot": "SleeveLeft",
    "uv_island_index": 0
  },
  "import_source": {
    "tool": "rhino",
    "note": "GH unroll v3"
  }
}
```

### 4.5 `cut_lines.json`

Polylines in atlas pixel space for fabrication and generative constraints:

```json
{
  "format_version": 1,
  "paths": [
    {
      "id": "seam-armhole-left",
      "closed": false,
      "points": [[120, 80], [2168, 80], [2168, 3152]]
    }
  ]
}
```

Studio builds **distance fields** or segment lists for the runtime constraint
API from this file.

---

## 5. Bake manifest (`*.bake.json`)

Written alongside each baked PNG. Bake is **additive**; manifests enable
regeneration when possible.

```json
{
  "format_version": 1,
  "design_id": "field-span-v1",
  "project_id": "MyGarment",
  "created": "2026-05-24T14:12:00Z",
  "image": "2026-05-24T14-12-00.png",
  "width_px": 8192,
  "height_px": 4096,
  "layout": "fabrication",
  "canvas_kind": "atlas",
  "panels": ["sleeve-left", "sleeve-right"],
  "lock": {
    "frame": 847,
    "time": 14.12,
    "seed": 985734123
  },
  "params_snapshot": { },
  "reproducible": true,
  "notes": ""
}
```

| Field | Purpose |
|-------|---------|
| `lock.frame` / `lock.time` | Stopping point for iterative sims |
| `lock.seed` | RNG seed when used |
| `params_snapshot` | Inline copy or path to frozen params |
| `reproducible` | `false` when unseeded random or nondeterministic GPU |

When `reproducible` is `false`, the PNG is authoritative; studio UI states this
clearly.

---

## 6. Generated build (`/.oripop/build/`)

Studio generates per-project Cargo manifest — **never hand-edited**:

```text
.oripop/build/
├── Cargo.toml          ← one [[bin]] per project design
├── src/
│   └── field-span-v1.rs   ← copied or symlinked from designs/.../main.rs
└── target/             ← optional; may live in global target dir
```

Dependencies pin to **`engine_version`** from `project.oripop`. Play invokes:

```bash
cargo build --manifest-path .oripop/build/Cargo.toml --bin <design-id>
```

---

## 7. Reference vs copy semantics

| Operation | Library | Project |
|-----------|---------|---------|
| Edit `main.rs` | Affects all reference consumers unless forked | Local only |
| Edit `params.json` | Library default | Project override file preferred |
| Bake | Exploration variants | Print / assembly previz variants |
| Delete design | Studio warns if referenced | Project-local only |

**Fork:** Copy library design into `project/designs/`; clear `library_ref`; user
owns the fork.

---

## 8. Import from Rhino / Grasshopper (v0 contract)

Minimum deliverables per import:

1. `garment.glb` — mesh + UVs + material slot names
2. `fabrication_layout.json` — panel rects on atlas
3. `cut_lines.json` — paths in atlas space
4. `panels/*.panel.json` — physical mm + mesh UV linkage

Optional: blank outline PNG per panel for registration overlay in atlas editor.

GH-side export plugin/script is **out of scope** for ori-pop v0 but should
target this schema.

---

## 9. Mapping to engine types (implementation notes)

| Data model | Rust home (target) |
|------------|-------------------|
| Panel rect, atlas size | New `oripop-math` or `oripop-studio` types |
| `cut_lines.json` | Constraint geometry → canvas adapter |
| `garment.glb` | `CpuMesh` import → `oripop-3d` GPU mesh |
| Baked PNG | `ObjectTexture::Baked` (new variant) or external path binding |
| `params.json` | `oripop_canvas::Params` (+ serde) |

Exact crate placement is decided during Phase 1 implementation; headless parsing
and tests belong in GPU-free code where possible.

---

## 10. Versioning and migration

- Every top-level manifest includes **`format_version`** and **`engine_version`**.
- Studio refuses Play when project `engine_version` exceeds installed runtime
  (with clear upgrade message).
- Library designs remain readable; migration tools come later.

---

## 11. Related documents

- Architecture: [01-architecture.md](./01-architecture.md)
- Workflows: [02-workflows.md](./02-workflows.md)
- Build phases: [04-roadmap.md](./04-roadmap.md)
