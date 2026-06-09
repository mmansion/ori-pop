# Studio Data Model

**Status: refreshed for cartridge migration (2026-05-25)**

On-disk layout, reference formats, and sidecar schemas for studio projects,
textures, atlas imports, and bakes. Schema **`format_version`** fields allow
migration as the studio evolves.

JSON is used for interchange (agents, GH plugins, web tools).

---

## 1. Scope hierarchy

```text
projects/
└── example-project/                ← user-level container ("project")
    ├── project.oripop
    ├── textures/                   ← each subfolder is one texture cartridge
    │   ├── coral-stipple/
    │   ├── lsystem-tree/
    │   └── flowfield-ink/
    ├── (future) assembly/          ← garment / object mesh
    ├── (future) atlas/             ← panel layout
    └── (future) bakes/             ← rendered PNGs + manifests
```

A **texture** is one self-contained generative artwork. A **project** is a
Unity-like container that may hold many textures plus, in later phases, an
assembly mesh, an atlas, baked outputs, and other assets.

The example project lives in `projects/example-project/` at the workspace
root. End-user projects can live anywhere on disk that the studio can
navigate to.

---

## 2. Texture cartridge

Each texture is a self-contained Cargo crate that builds **two** targets:

| Target | Purpose |
|--------|---------|
| `cdylib` | Loaded by `oripop-studio` at runtime via `libloading` |
| `bin`    | Standalone player: `cargo run -p <texture-id>` |

```text
textures/coral-stipple/
├── Cargo.toml        ← crate-type = ["cdylib", "rlib"] + [[bin]]
├── texture.oripop    ← manifest
├── params.json
└── src/
    ├── lib.rs        ← oripop_texture_render + draw()
    └── bin.rs        ← standalone runtime entry
```

The cdylib must export the C-ABI symbol:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn oripop_texture_render(
    t:          f32,
    params_ptr: *const u8,
    params_len: usize,
    emit:       EmitFn,
    emit_ctx:   *mut c_void,
);
```

`EmitFn` is the host-supplied callback (`oripop_canvas::cartridge::EmitFn`)
that copies the texture's background color and tessellated vertex bytes back
into a host-owned buffer. The texture's draw closure runs inside
[`oripop_canvas::cartridge::dispatch`], which resets the canvas
thread-local, runs the closure, and emits the resulting frame.

The cdylib must also export the ABI version symbol via
`oripop_canvas::export_cartridge_abi!();` (one invocation in `lib.rs`). The
host refuses to load a cartridge whose version differs from its own
`oripop_canvas::cartridge::CARTRIDGE_ABI_VERSION`.

**Vertex layout (ABI v2):** each vertex is 36 bytes
(`oripop_canvas::draw::VERTEX_2D_STRIDE`): `[f32; 2]` position at offset 0,
`[f32; 4]` RGBA at offset 8, `[f32; 2]` UV at offset 24, `f32` texture slot at
offset 32 (0.0 = solid color, 1.0 = sample the bound 2D texture). ABI v1
(24-byte position + color vertices) is no longer accepted.

### 2.1 `texture.oripop`

```json
{
  "format_version": 1,
  "id": "coral-stipple",
  "title": "Coral stipple field",
  "engine_version": "0.1.0",
  "canvas": {
    "kind": "provisional",
    "width": 1024,
    "height": 1024
  },
  "tags": ["stipple", "field", "example"],
  "params": "params.json"
}
```

**`canvas.kind` values:**

| Value | Meaning |
|-------|---------|
| `provisional` | Fixed pixel buffer; no panels |
| `primitive_uv` | `{ "kind": "primitive_uv", "mesh": "sphere" }` |
| `atlas` | `{ "kind": "atlas", "atlas_ref": "atlas/atlas.oripop", "panels": ["sleeve-left", "sleeve-right"] }` |

### 2.2 `params.json`

The shape is texture-defined: each texture's `src/lib.rs` declares its own
params struct (often a thin wrapper around `oripop_canvas::Params` for
stipple-style textures). The studio passes the file's bytes through to the
cdylib unmodified; the cdylib does the JSON decoding.

---

## 3. Project

### 3.1 Folder layout

```text
projects/example-project/
├── project.oripop
├── textures/
│   ├── coral-stipple/
│   ├── lsystem-tree/
│   └── flowfield-ink/
├── (future) assembly/garment.glb
├── (future) atlas/atlas.oripop
└── (future) bakes/
```

The studio discovers textures by scanning `<project>/textures/` for
subdirectories that contain a `texture.oripop` file. The folder name is the
texture id.

### 3.2 `project.oripop`

```json
{
  "format_version": 1,
  "engine_version": "0.1.0",
  "title": "Ori Pop example project",
  "created": "2026-05-25T00:00:00Z",
  "default_texture": "coral-stipple"
}
```

Atlas, assembly, and baked-output fields will be added back as those Phase 2
features come online.

---

## 4. Atlas (Phase 2, unchanged schema)

The atlas, fabrication layout, cut lines, and panel manifests in the original
spec are still planned. See git history of this file for the detailed schema
that will be reinstated once atlas authoring is implemented.

---

## 5. Bake manifest (`*.bake.json`)

Written alongside each baked PNG. Bake is **additive**; manifests enable
regeneration when possible.

```json
{
  "format_version": 1,
  "texture_id": "coral-stipple",
  "project_id": "example-project",
  "created": "2026-05-25T14:12:00Z",
  "image": "bake-1779711641103.png",
  "width_px": 1024,
  "height_px": 1024,
  "layout": "fabrication",
  "canvas_kind": "provisional",
  "panels": [],
  "lock": {
    "frame": 847,
    "time": 14.12,
    "seed": null
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
| `params_snapshot` | Inline copy of the params JSON at bake time |
| `reproducible` | `false` when unseeded random or nondeterministic GPU |

When `reproducible` is `false`, the PNG is authoritative; studio UI states this
clearly.

---

## 6. Build cache (`target/debug/.oripop/`)

The studio shells out to `cargo build -p <texture-id> --lib` to compile the
selected texture's cdylib, then copies the resulting library to a
versioned path under `target/debug/.oripop/<crate-name>-<timestamp>-<seq>.<ext>`
so that subsequent reloads after a save get a fresh `HMODULE` (Windows holds
file locks on already-loaded DLLs).

This directory is studio-managed and safe to wipe.

---

## 7. Reference vs copy semantics

Textures live entirely inside their owning project; there is no separate
shared library in the current model. Forking a texture is a directory copy.
Phase 2 may reintroduce a cross-project texture registry; if so the manifest
will gain a `library_ref` field similar to the old design model.

---

## 8. Mapping to engine types

| Data model | Rust home |
|------------|-----------|
| `ProjectManifest`, `TextureManifest`, `Project` | `oripop-project` |
| `CanvasKind`, `PrimitiveMesh` | `oripop-project` |
| `BakeManifest`, `BakeLock` | `oripop-project` |
| `Cartridge`, `Frame`, emit callback host side | `oripop-studio::cartridge` |
| `cartridge::dispatch`, `EmitFn`, drawing primitives | `oripop-canvas` |
| Standalone player entry (`bin.rs`) | `oripop-runtime` consumer |

---

## 9. Versioning and migration

- Every top-level manifest includes **`format_version`** and **`engine_version`**.
- Studio refuses to load a texture whose `engine_version` exceeds the installed
  runtime (planned check; currently advisory).
- Older `library.oripop` + `design.oripop` files from pre-cartridge versions
  are no longer supported; migration tools are TBD.

---

## 10. Related documents

- Architecture: [01-architecture.md](./01-architecture.md)
- Workflows: [02-workflows.md](./02-workflows.md)
- Build phases: [04-roadmap.md](./04-roadmap.md)
