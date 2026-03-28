# ORI-POP — Philosophy

*A living document. Updated as the system evolves and the vision clarifies.*

---

## What It Is

ORI-POP is a personal computational design system for generative sculpture.

It is not a general-purpose tool. It is not a framework for other people.
It is a creative instrument — authored by one person, accumulating aesthetic
decisions over time, producing work across every output modality from the same
generative core.

Like openFrameworks was to a generation of new-media artists: opinionated,
personal, and shaped by a specific way of seeing.

---

## The Central Idea

**Surface and pattern share a coordinate system.**

A generative texture is not projected onto a form from outside. It grows from
the same parameterization that defines the geometry. The pattern *knows* the
surface it inhabits — its curvature, its arc length, its principal directions.
This mathematical alignment is what makes the output physically meaningful, not
just visually compelling.

When the UV field drives a robot painting arm, the brush follows the surface.
When it drives a paper fold pattern, the crease lines are native to the form.
When it drives 3D print infill, the density gradient follows the geometry's
stress logic.

One generative system. Many material outputs.

---

## The Output Spectrum

The same design intent — the same UV field, the same surface definition — can
produce:

| Output | Material | Scale |
|--------|----------|-------|
| Screen animation | Digital | Any |
| Fine art print | Paper / archival | A4 → large format |
| Plotter / SVG drawing | Ink on paper | Tabletop → wall |
| Scored / cut paper model | Developable sheet | Hand-scale |
| Robot-painted sculpture | Paint on form | Any |
| 3D printed object | Polymer / metal | Desktop → architectural |
| Kinetic robotic installation | Mixed | Room-scale |
| XR environment | Digital / spatial | Immersive |

The framework does not privilege any of these. They are all first-class outputs
of the same upstream computation.

---

## The Stack

```
Heavy computation (external libraries)
truck / parry / rapier / nalgebra
  ↓
oripop-math          — thin integration types: Surface, UVMap, Frame, Mesh
  ↓
oripop-3d            — GPU generative pipeline, real-time render, egui inspector
  ↓
oripop-fab           — fabrication bridge: toolpath / strips / STL / G-code
  ↓
oripop-evo           — genetic / evolutionary optimization of design parameters
  ↓
Agentic layer        — AI models that read, modify, and evaluate the graph
```

**ORI-POP does not reimplement the geometry kernel.** It wraps proven Rust
libraries (truck for NURBS/B-rep, rapier for physics, nalgebra for math) and
provides a creative coding interface on top of them.

What ORI-POP owns is the **creative and agentic interface** — the expressive
layer where generative logic is authored, previewed, and connected to physical
output.

---

## The Interface Vision

The ideal interface is a **visual directed graph with embedded scripting** —
the sensibility of Rhino's Grasshopper and TouchDesigner, with the precision
of a code editor and the intelligence of an agentic layer.

**Visual graph:** Parametric nodes with typed inputs and outputs. Data flows
downstream. Change an upstream parameter — a surface radius, a noise frequency,
a force field strength — and everything depending on it recomputes. The graph
is the design. It is serializable, diffable, and legible to an AI agent.

**Scripting windows:** Each node can be a WGSL compute shader, a Rust closure,
or a high-level built-in. The code editor is first-class — not hidden behind
the visual interface but integrated with it. A node's source is inspectable
and editable inline.

**Agentic editor:** An AI agent can read the graph, understand the design
intent, propose parameter changes, write new node implementations, and evaluate
fabrication constraints. The agent observes through the same real-time render
that the human sees. The graph's serializability is what makes this possible —
the agent modifies data, not pixels.

**Code editor:** The scripting layer is Rust + WGSL. Rust for orchestration
and geometry logic. WGSL for GPU compute — UV-space generative functions,
physics integration, SDF evaluation. The language choice is intentional:
performance, correctness, and a clear boundary between CPU and GPU work.

**Inspector (egui):** The current Tab-toggled inspector is the beginning.
It grows into a full properties panel, timeline scrubber, output configurator,
and fabrication status monitor — all within the same window as the 3D view.

---

## The Generative Core

The creative coding layer operates primarily in **UV space** — the parametric
coordinate system of a surface.

A UV field is a function `f(u, v, t) → value` where:
- `(u, v) ∈ [0,1]²` is a point on the surface
- `t` is time (for animation and kinetic work)
- `value` encodes pattern information: direction, density, color, mask

The field is computed on the GPU as a WGSL compute shader. Built-in generators:

- **Domain-warped FBM** — layered noise with swirling warp, animated
- **Reaction-diffusion** — Gray-Scott, Turing patterns, biological surface logic
- **Voronoi / distance fields** — cell structure, edges, gradients
- **Force fields** — attractors, gradients, compression zones (from oripop-core)
- **Custom WGSL** — the user writes the shader directly

Surface-aware generation means the compute shader receives surface-specific
data — curvature, arc-length parameterization, principal directions — so the
pattern responds to the geometry it inhabits rather than being blindly mapped
onto it.

---

## Coordinate Convention

**Z-up right-handed** throughout. X = right, Y = depth/forward, Z = up.
XY is the ground / build plane.

This is the standard of CAD tools (Rhino, FreeCAD, STEP, STL), robotics (ROS),
and 3D printing slicers (Bambu, Prusa, Cura). Adopting it early means geometry
travels cleanly from ori-pop to fabrication without coordinate remapping.

Gravity is `(0, 0, -9.81)`.

---

## On Developable Surfaces

A developable surface can be unrolled flat without distortion — cones,
cylinders, ruled surfaces, tangent developables. The UV map *is* the flat
pattern. What is designed in UV space is literally what gets cut, scored,
and folded from sheet material.

Developable surfaces are a first-class concern because they bridge digital
generative logic and physical sheet fabrication (paper models, sheet metal,
leather, fabric) with mathematical precision. The UV field authored on a
developable surface is the cut pattern.

---

## On AI and Agency

The graph structure of the parametric pipeline is what enables meaningful AI
assistance:

- The graph is **serializable** — an agent can read and write it
- The nodes have **typed interfaces** — an agent knows what each connection means
- The fabrication constraints are **evaluable** — overhang angle, wall thickness,
  printability are computable per-voxel
- The real-time render is the agent's **visual feedback loop**

An agent in this system is not generating images. It is modifying a parametric
design graph and observing the downstream consequences — visual, physical, and
structural. This is a meaningful design collaboration, not style transfer.

The agentic layer is not a future concern to be added later. The graph
architecture should be designed from the beginning with agent-readability in
mind: named nodes, typed ports, serializable parameters, observable outputs.

---

## Interchange Format — glTF

**glTF (GL Transmission Format)** is the interchange format for ori-pop designs.
Developed by the Khronos Group, it is the universal 3D transmission format —
readable by Blender, Houdini, Rhino, three.js, visionOS, NVIDIA Omniverse, and
every modern WebGPU/WebGL renderer. Often called "the JPEG of 3D."

ori-pop exports `.glb` (binary glTF) as its primary portable format. The file
carries:

- **Geometry** — tessellated mesh, normals, UVs
- **Materials** — PBR material with the baked generative texture
- **Scene graph** — named node hierarchy with transforms
- **DesignTree** — the full parametric description, embedded in each node's
  `extras.oripop` field as JSON

The `extras` embedding is the key detail. A standard glTF viewer shows the
baked geometry and texture. An ori-pop reader reconstructs the complete
`DesignTree` — every parameter, every generative function, every fabrication
intent — and restores the live, editable, recomputable design.

**The round-trip is lossless.** Open any `.glb` in ori-pop and get the full
mathematical object back, not just a baked snapshot.

This makes every ori-pop export simultaneously:
- A preview asset for any 3D tool or web viewer
- A complete, recoverable parametric design
- An input to NVIDIA Omniverse, AI generative tools, and spatial computing (XR)

USD (Universal Scene Description, NVIDIA Omniverse's format) is a future export
target. The Rust ecosystem for USD is not yet mature; glTF provides equivalent
interoperability today with excellent tooling.

---

## What It Is Not

- Not a game engine (though it borrows real-time rendering from one)
- Not a general-purpose 3D modeler
- Not a replacement for Rhino, Blender, or Houdini
- Not a framework for other people's aesthetics
- Not finished

---

## The Work

The framework is inseparable from the work it produces. The aesthetic
accumulates in the API choices — which primitives are first-class, which
operations are effortless, which outputs are supported. Every addition to
the framework is also a statement about what the work is.

The goal is not feature completeness. The goal is expressive precision —
a system that makes the specific things you want to make feel inevitable.

---

*Last updated: 2026-03-28*
