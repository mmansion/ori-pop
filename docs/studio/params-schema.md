# params.json schema

**Status:** v0 (aligns with `oripop_canvas::Params` and [`presets/default.json`](../../presets/default.json))

Library and project designs store tunable generative parameters in **`params.json`**
next to `main.rs`. The studio inspector (Phase 3) will edit this file; **bake**
reads it headlessly for stipple field output.

## Top-level fields

| Field | Type | Purpose |
|-------|------|---------|
| `seed` | u64 | RNG seed for dot placement |
| `canvas` | object | Logical canvas size (`width`, `height` in design units) |
| `field` | object | Scalar field: singularity, warp, forces |
| `distribution` | object | Dot count, radii, jitter, density curve |
| `render` | object | Invert / threshold (future raster options) |

For **provisional** canvases, set `canvas.width` / `canvas.height` to match
`design.oripop` pixel size (e.g. `1024`).

## Example

See [`examples/texture-library/designs/coral-stipple/params.json`](../../examples/texture-library/designs/coral-stipple/params.json).

## Serde

Types are defined in `oripop_canvas::field::Params`. Round-trip via:

```rust
let params: Params = serde_json::from_str(&text)?;
```

Bake manifests store a **`params_snapshot`** at bake time for reproducibility.
