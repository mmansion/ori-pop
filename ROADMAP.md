# ORI-POP Roadmap

This document captures the intended long-term direction of the ori-pop framework.
It is a living record of architectural decisions made early so that patterns stay
consistent as the codebase grows.

The guiding ambition: a **GPU-first, engine-first** creative-coding and
generative-design platform — one host runtime (window, passes, cameras), with
**two-dimensional work as a *view mode*** (orthographic camera, window-aligned
canvas) exactly like engine-native 2D in Unity — that serves both art-making and
real fabrication: robotic, CNC, and 3D-printed output from the same generative
models used to produce visuals.

---

## 0. Engine Host, `oripop-canvas`, and 2D as a Mode

**Status: in progress (conceptual alignment; implementation evolving)**

**Product shape:** Ori Pop is **not** modeled as a separate “2D app” beside a “3D
app.” Sketches and future editor tooling target **one engine window** and **one
frame graph** (`oripop-3d`). “Processing-like” sketches are **orthographic views**
of scene content (often a plane or overlay) where the **canvas maps cleanly to
the window**, not a second platform abstraction.

**Kernel to grow first — `oripop-canvas`:** Treat **`oripop-canvas` as the creative
engine kernel** — canvas contracts, scalar fields, stipple distribution, and the
Processing-style drawing API. This is the **first runtime foundation** to harden:
everything else (surface binding, fabrication, agents) **consumes** patterns and
parameters authored here or serializes them through the `DesignTree` in
`oripop-math`.

**Mathematical substrate — `oripop-math`:** Remains the **GPU-free mathematical
kernel** — `Surface`, `DesignTree`, `CpuMesh`, frames — serializable and
headlessly testable. It is **not** the sketch API; it is the **geometry and design
object** layer the engine and agents share.

**Relationship:** `oripop-3d` depends on **both** `oripop-math` and `oripop-canvas`.
The `sketches` crate depends on **`oripop-canvas` and `oripop-3d`** (and therefore
transitively on `oripop-math`). Long term, a dedicated
**Ori Pop editor** binary would sit alongside `sketches`, reuse the same crates,
and encapsulate project UI, inspectors, and coding tools inside the host.

---

## 1. Coordinate System — Z-Up Right-Handed

**Status: done**

Adopt the CAD / robotics / 3D-printing standard: Z is up, XY is the build plane,
right-handed orientation (ISO 80000-2, ROS, STEP, STL, Rhino, Grasshopper,
FreeCAD).

The workspace has **completed** the migration away from Y-up defaults: cameras,
mesh primitives, lighting, and documentation assume **Z-up right-handed** space
(see `oripop-math` / `oripop-3d` module docs and scene conventions).

---

## 1a. egui Inspector Panel

**Status: done**

A live inspector panel (Tab key toggles) built on egui 0.33 renders as a fourth GPU
pass on top of every 3D frame.  Shows scene time, camera (eye, target, FOV),
light direction, texture-gen parameters, and a list of named scene objects.
All values are editable with drag inputs and sliders in real time.

---

## 2. `oripop-math` — GPU-Free Geometry Kernel

**Status: in progress**

A new crate with **no GPU dependency** (`wgpu`, `winit` not in `Cargo.toml`).
Every **planned** downstream geometry crate — physics, geo, evolution, fabrication
— depends on this crate for shared types. **`oripop-3d`** already depends on it.
**`oripop-canvas`** remains the creative engine kernel and does **not** yet list
`oripop-math` as a dependency. Sketches combine **`oripop-canvas` + `oripop-3d`**
(and thus pull in `oripop-math` through `oripop-3d`). `DesignTree` wiring will
tighten the link between canvas fields and `Surface` data over time. Because
`oripop-math` carries no GPU weight, it can be tested headlessly on any machine,
including CI.

### The Design Tree

The foundational data structure of ori-pop. A `DesignTree` is the complete
mathematical description of a design — every surface, every generative field,
every fabrication intent — as a directed acyclic graph of typed, named, serializable
nodes.

A design in ori-pop is a **mathematical object**: fully defined, portable,
deterministic. Given the same `DesignTree`, you get the same object. Every time.
On any machine. In any material. The material, scale, and fabrication method are
transformations applied to that object — not properties stored in it.

Key properties:
- **Serializable** — RON (native) and JSON (agent/interop). Round-trips losslessly.
- **Diffable** — meaningful git diffs between design iterations.
- **Agent-readable** — named nodes, typed ports, documented parameters. An AI
  agent reads and writes the tree; it never touches the renderer directly.
- **Evaluable** — each node is a pure function. Same inputs → same output. No
  hidden state.

Core node types: `Surface`, `UvField`, `Material`, `Mesh`, `Toolpath`, `Output`.

### Core geometry types:
- `DesignTree` — the complete parametric design as a serializable DAG.
- `Node` / `Edge` / `Port` — typed graph primitives.
- `Value` / `Param` — typed, named, documented parameter atoms.
- `Surface` trait — parametric surface: `(u,v) → Vec3`, `normal`, `curvature`.
  Implementations: `UvSphere`, `Plane`, `Cylinder`, `Torus`, `RuledSurface`.
- `PrincipalCurvatures` — k1, k2 and their directions. Determines developability.
- `Frame` — coordinate frame (origin + orthonormal basis, Z-up). Represents robot
  end-effector poses, print-bed orientation, workpiece datums, joint frames.
- `CpuMesh` — vertex/normal/UV/index data. The canonical geometric representation.
  Separate from the GPU-side `GpuMesh` in `oripop-3d`, which is a cached upload.
- `BoundingBox` — AABB in Z-up space.
- `Ray` — for picking, intersection queries, and SDF ray-marching.
- `Sdf` — see item 5 below.

The existing `Point`, `Line`, and `Bezier` types in `oripop-canvas` will migrate
here and be promoted to full 3D types.

---

## 3. `oripop-geo` — Computational Geometry

**Status: planned**

Geometric algorithms, a mix of CPU and GPU compute implementations.

| Operation | Method |
|---|---|
| Mesh boolean (CSG) | SDF-based: `union = min(a,b)`, `subtract = max(a,-b)` |
| Convex hull (3D) | QuickHull, parallelizable on compute |
| Delaunay / Voronoi | Jump flooding algorithm — GPU native |
| Marching cubes | GPU compute — extracts mesh from SDF volume |
| Surface sampling | Poisson disk sampling via compute |
| Normal recomputation | Per-face then per-vertex reduction pass |
| Laplacian smoothing | Iterative compute pass |
| Curvature estimation | Principal curvatures from the Hessian of the SDF |
| Arc length reparameterization | Lookup table precomputed on CPU |

Depends on: `oripop-math`.

---

## 4. Surface-Aware Texture Generation

**Status: planned**

Generative 2D textures that are parameterized in the same `(u, v)` coordinate
system as the 3D surface they are applied to, ensuring mathematical alignment
with no projection distortion.

### The Core Idea

A parametric surface is a function `surface(u, v) → Vec3`. If the texture is also
generated as a function of `(u, v)` — sharing the same coordinate system — the
alignment is mathematically guaranteed. The texture is not projected onto the
surface from outside; it grows from the same parameterization that defines the
geometry.

### Levels of Alignment

**Level 1 — UV sampling (current state).** The compute shader (or CPU stipple
raster) produces a **flat 2D image** in parameter space; the mesh samples it by
UV inside the **same engine frame** as any 3D content. Works for abstract
patterns; distortion follows from the UV layout. This is consistent with
**2D-as-a-mode**: the pattern is authored in `(u, v)` / canvas space, then **read
through** surface UVs in the 3D pass.

**Level 2 — Surface-parameterized generation.** The compute shader receives
surface-specific uniforms: profile curve, principal curvatures, arc length
reparameterization. The noise or pattern is generated as a function of surface
parameters, not flat UV. The texture and geometry share a coordinate system.

**Level 3 — Simulation on the surface.** PDEs such as Gray-Scott
reaction-diffusion (Turing patterns, branching, spots) run directly in UV space
with isotropic diffusion in surface coordinates. This is how biological surface
patterns actually form — they are governed by geometry.

### Seam Handling

- **Seamless 4D noise** — for closed surfaces (spheres, tori), map `(u, v)` to a
  point on a 4D torus `(cos 2πu, sin 2πu, cos 2πv, sin 2πv)` and evaluate 4D
  noise. Periodic in both directions, no seam.
- **Triplanar mapping** — three world-axis projections blended by surface normal.
  No UV needed. Useful fallback for irregular geometry.

### Fabrication Applications

- Material gradient in UV space → multi-material print color zoning.
- Reaction-diffusion pattern → sparse vs. dense infill tied to stress lines.
- Surface toolpath pattern → CNC cut directions aligned to dominant surface
  feature, encoded in UV.
- Voronoi cells in UV → surface holes after boolean subtraction, forming
  latticed shells with patterns native to the surface form.

Depends on: `oripop-math` (parametric surface types), `oripop-geo` (curvature
computation, arc length reparameterization).

---

## 5. `Sdf` — Signed Distance Field Primitives

**Status: planned (part of `oripop-math`)**

An SDF is a function `f(point) → f32` where the sign indicates inside (negative),
surface (zero), or outside (positive). Boolean CSG reduces to `min`/`max`
arithmetic on two distance values. No polygon clipping, no mesh repair, no
degenerate triangles.

```
union(a, b)     = min(a, b)
subtract(a, b)  = max(a, -b)
intersect(a, b) = max(a, b)
```

The `Sdf` enum in `oripop-math` is a pure data structure (no GPU dependency).
It describes a shape tree that can be serialized, logged, and passed between
the genetic optimizer and the geometry kernel.

Primitives:
- `Sphere`, `Box`, `Cylinder`, `Torus`, `Cone`
- `Gyroid { scale, thickness }` — triply periodic minimal surface; the dominant
  FDM/SLA infill structure. Load-bearing in all directions, first-class here.

Combinators:
- `Union`, `Subtract`, `Intersect`

Modifiers:
- `Shell(sdf, wall_thickness)` — hollow with uniform wall
- `Offset(sdf, amount)` — morphological expand / contract (fit tolerances)
- `Twist(sdf, rate)`
- `Bend(sdf, rate)`

Evaluators (added in later crates):
- CPU evaluator in `oripop-geo`: `sdf.distance(point: Vec3) → f32`
- GPU evaluator in `oripop-3d`: compiles the tree to a WGSL compute shader
  dispatch; marching cubes extracts a renderable mesh.

---

## 6. `ComputeStage` — Extensible GPU Pipeline

**Status: planned (in `oripop-3d`)**

The current three-pass frame (compute → 3D → 2D overlay) has the passes baked
into `renderer.rs`. Adding physics, SDF evaluation, or evolutionary fitness
evaluation would require rewriting the renderer each time.

A `ComputeStage` trait lets each system register its own GPU passes:

```rust
pub trait ComputeStage: Send {
    fn init(&mut self, device: &wgpu::Device, queue: &wgpu::Queue);
    fn encode(&self, encoder: &mut wgpu::CommandEncoder, queue: &wgpu::Queue);
}
```

The renderer holds `Vec<Box<dyn ComputeStage>>` and encodes them in order before
the 3D pass. The texture generation that already exists becomes the first stage.
Physics integration, SDF evaluation, and evolutionary fitness evaluation each
become additional stages, isolated and composable.

From a sketch:
```rust
fn main() {
    size(960, 640);
    run3d_with(draw)
        .add_stage(TextureGen::default())
        .add_stage(PhysicsIntegrator::new(1000))
        .start();
}
```

---

## 7. `oripop-physics` — GPU Physics Simulation

**Status: planned**

Two complementary physics backends:

**Rigid body** — [Rapier](https://rapier.rs/) integration (pure Rust, CPU).
Handles collision detection, joints, and constraints correctly. Visualization
passes results to `oripop-3d` each frame.

**Particles / soft body / cloth / fluid** — Position-Based Dynamics (PBD) on GPU
compute. Each PBD iteration is a compute dispatch; the output position buffer
binds directly as a vertex buffer for rendering with no CPU readback.

Fluid: SPH (Smoothed Particle Hydrodynamics) or LBM (Lattice-Boltzmann), both
GPU-native.

Gravity is `(0, 0, -9.81)` — Z-down, consistent with the Z-up workspace.

Depends on: `oripop-math`, `oripop-3d` (for `ComputeStage`).

---

## 8. `oripop-evo` — Genetic / Evolutionary Optimization

**Status: planned**

GPU-parallel evaluation of entire populations in a single compute dispatch.
The genome is a flat `f32` array encoding SDF parameters, force field strengths,
structural lattice parameters, or any other continuous design variable.

Frame loop:
1. Population buffer lives on GPU between generations.
2. Compute pass: evaluate fitness for all N individuals in parallel.
3. CPU readback: read `fitness[N]`, run selection, crossover, mutation.
4. Write new population to GPU buffer.
5. Repeat.

The fitness function is a WGSL compute shader. For fabrication objectives:
minimize material volume, maximize stiffness-to-weight, enforce minimum wall
thickness, penalize overhangs beyond a threshold angle — all evaluable per-voxel
in the SDF volume.

Depends on: `oripop-math`, `oripop-geo`.

---

## 9. `oripop-fab` — Fabrication Output

**Status: planned**

Bridges generative models to physical manufacturing.

- **Mesh extraction** — marching cubes / dual contouring on GPU; produces
  watertight meshes from SDF volumes.
- **Printability analysis** — overhang angle heatmap (surface normal vs. Z-up
  build direction) computed on GPU, visualized as a texture overlay.
- **Slicing** — plane-mesh intersection per layer, one compute dispatch per layer.
- **Toolpath generation** — robot painting arm strokes derived from the UV field.
  Stroke direction, density, and spacing encoded in UV space, following the
  surface curvature.
- **Developable strip unrolling** — for sheet material (paper, leather, metal):
  unroll ruled/developable surfaces to flat cut patterns with fold/score lines
  derived from the UV field. Export as SVG or DXF.
- **Export formats:**
  - **glTF / GLB** — primary portable format. Geometry + materials + full
    `DesignTree` in `extras`. Round-trip lossless. Compatible with Blender,
    Houdini, Rhino, three.js, visionOS, NVIDIA Omniverse.
  - **STL** — triangle soup for 3D printing slicers.
  - **3MF** — materials, colors, lattice metadata (Bambu, Prusa, Cura).
  - **SVG / DXF** — 2D cut patterns, plotter output, fold lines.
  - **PNG / EXR** — high-resolution raster of the UV field (fine art print).
  - **G-code** — CPU-side, from slice data.
  - **USD** — future target when Rust bindings mature (NVIDIA Omniverse native).

Depends on: `oripop-math`, `oripop-geo`.

---

## Dependency Graph

**Current workspace (crates that exist today):**

```
oripop-math        — GPU-free math / DesignTree / Surface / CpuMesh
oripop-canvas        — creative engine kernel: canvas, fields, stipples, 2D API
oripop-3d          — depends on oripop-math + oripop-canvas (window, wgpu, scene)
sketches           — depends on oripop-canvas + oripop-3d (binaries / experiments)
```

**Planned expansion (from items elsewhere in this roadmap):**

```
oripop-math   (no GPU — pure Rust geometry)
    ├── oripop-geo      (computational geometry, some GPU compute)
    │       ├── oripop-evo   (genetic optimization)
    │       └── oripop-fab   (fabrication output)
    ├── oripop-physics  (simulation)
    └── oripop-3d       (rendering / visualization; already depends on canvas + math)
```

`oripop-canvas` is the **authoring-facing engine kernel**; `oripop-math` is the
**geometry / design-object kernel**. `oripop-3d` pulls both into the GPU host.
Future editor and fabrication crates depend on `oripop-math` and may depend on
`oripop-canvas` and `oripop-3d` as needed; they do not all need to depend on each
other.
