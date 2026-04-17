# Roadmap

ori-pop began as a minimal creative-coding workspace. The longer ambition is a
GPU-first generative design framework that serves both art-making and real
fabrication — producing robotic toolpaths, 3D-printed forms, and CNC programs
from the same generative models used to create visuals.

This page records the architectural direction being built toward, so that
patterns established early stay consistent as the codebase grows.

---

## Coordinate System — Z-Up Right-Handed ✓

The framework uses the **CAD / robotics / 3D-printing standard**: Z is up, XY
is the build plane, right-handed orientation (ISO 80000-2, ROS, STEP, STL,
Rhino, Grasshopper, FreeCAD, most fabrication toolchains).

X = right, Y = depth/forward, Z = up, XY = ground plane.  The camera's default
`up` vector is `Vec3::Z`.  All mesh primitives (sphere, cube, plane) are
generated in this convention.

## Live Inspector Panel ✓

A real-time egui inspector panel renders as a fourth GPU pass on top of every 3D
frame.  Press **Tab** to toggle it.  Shows and edits camera, lighting,
texture-generation parameters, and the current frame's named scene objects.

---

## Sketch-first evolution

The **core** stays a thin Processing-like toolkit. Richer mutators, editors, and
distribution experiments land in **sketches** first; successful patterns are
**promoted** into `oripop-canvas` (or future crates) with tests. See
[`ROADMAP.md`](https://github.com/mmansion/ori-pop/blob/main/ROADMAP.md) §0b for
the full wording.

---

## `oripop-math` — Geometry Without GPU

A new crate with no GPU dependency — no `wgpu`, no `winit`. It is the
foundational layer that every other crate depends on for shared geometric types:
`Frame` (coordinate frames for robot poses and workpiece datums), `Mesh`
(CPU-side vertex and index data), `Ray`, `BoundingBox`, `Plane`, and the `Sdf`
primitive tree described below.

Because it carries no GPU weight it can be tested headlessly on any machine,
including CI, with ordinary Rust unit tests.

---

## Signed Distance Fields

An SDF is a function that takes a point in space and returns a single number:
how far that point is from the surface of a shape, with a sign that tells you
whether you are inside or outside.

```
positive → outside the shape
zero     → exactly on the surface
negative → inside the shape
```

Boolean operations — union, subtraction, intersection — reduce to arithmetic on
two distance values (`min`, `max`). No polygon clipping, no mesh repair, no
degenerate triangles. The shape is described as a tree of primitives and
combinators, evaluated per point.

Built-in primitives include `Sphere`, `Box`, `Cylinder`, `Torus`, and
`Gyroid` — a triply periodic minimal surface that is the dominant infill
structure in modern FDM and SLA printing, load-bearing in all directions.

Modifiers include `Shell` (hollow with a uniform wall thickness), `Offset`
(expand or contract the surface — useful for fit tolerances), `Twist`, and
`Bend`.

---

## Surface-Aware Texture Generation

The current generative texture pipeline produces a flat 2D image that is applied
to a mesh by UV coordinates. The next step is texture generation that is
*parameterized in the same `(u, v)` coordinate system as the surface itself*.

A parametric surface is a function `surface(u, v) → point in 3D`. If the
texture is generated as a function of the same `(u, v)` — sharing the surface's
own coordinate system — the alignment is mathematically guaranteed. The texture
is not projected onto the surface from outside; it grows from the same
parameterization that defines the geometry.

Three levels of alignment:

**UV sampling** (current). The compute shader generates a flat image without
knowledge of the 3D shape. Distortion follows from the UV layout.

**Surface-parameterized generation**. The compute shader receives surface-specific
data: profile curve, arc length reparameterization, principal curvatures. The
pattern is generated as a function of surface parameters, not flat image space.

**Simulation on the surface**. Reaction-diffusion equations (Turing patterns,
branching, cellular spots) run directly in UV space with diffusion that is
isotropic in surface coordinates — the way biological surface patterns actually
form.

For fabrication, the texture is not decoration. It encodes material or process
information: a gradient that maps to multi-material print zones, a pattern that
defines sparse versus dense infill tied to structural stress, a field that
encodes CNC cut directions aligned to the dominant surface feature.

---

## Physics Simulation

Two complementary backends:

**Rigid body** — [Rapier](https://rapier.rs/) (pure Rust, CPU) for collision
detection, joints, and constraints. Results visualized via `oripop-3d` each
frame.

**Particles, soft body, cloth, fluid** — Position-Based Dynamics on GPU compute.
Each iteration is a compute dispatch; the output position buffer binds directly
as a vertex buffer for rendering with no CPU readback.

---

## Genetic and Evolutionary Optimization

The genome is a flat array of floats encoding SDF parameters, force field
strengths, structural lattice parameters — any continuous design variable. The
entire population is evaluated in a single GPU compute dispatch, one invocation
per individual. The fitness function is a WGSL shader.

For fabrication objectives: minimize material volume, maximize
stiffness-to-weight, enforce minimum wall thickness, penalize overhangs beyond
the printable threshold — all evaluable per-voxel in the SDF volume.

---

## Fabrication Output

- Watertight mesh extraction from SDF volumes via marching cubes on GPU.
- Overhang angle analysis — a printability heatmap computed from surface normals
  versus the Z-up build direction, visualized as a texture overlay.
- Layer slicing — plane-mesh intersection per layer.
- Export to STL, 3MF (preferred by Bambu, Prusa, Cura — supports materials,
  colors, and lattice metadata), and OBJ.
- G-code generation from slice data.

---

## Crate Structure

```
oripop-math       no GPU — pure Rust geometry kernel
    ├── oripop-geo        computational geometry, some GPU compute
    │       ├── oripop-evo    genetic / evolutionary optimization
    │       └── oripop-fab    fabrication output
    ├── oripop-physics    simulation
    └── oripop-3d         rendering and visualization
            └── oripop-canvas   2D drawing API
```

`oripop-3d` depends on both `oripop-canvas` and `oripop-math`. `oripop-canvas` does
not depend on `oripop-math` yet. Higher-level crates depend on `oripop-math` but
not necessarily on each other.
