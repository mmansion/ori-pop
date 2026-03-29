//! Organic 3D lattice — axis-aligned grid that grows from the centre of a
//! cube using a randomised BFS.  Because each step has a random duration,
//! some tendrils race ahead while others lag, giving the growth an organic,
//! generative character while remaining strictly axis-aligned (X/Y/Z only).
//!
//! Edges are coloured by their axis:  X = blue  |  Y = teal  |  Z = green
//! Brightness falls off from the centre outward.
//! The growth loops automatically with a smooth fade-out.
//!
//! Controls:
//!   Right-drag — orbit camera
//!   Scroll     — zoom
//!   Space      — toggle inspector panel
//!
//! Run with:
//!   cargo run --bin lsystem-3d

use oripop_3d::prelude::*;
use rand::{SeedableRng, Rng};
use rand::rngs::SmallRng;
use std::collections::{HashMap, VecDeque};
use std::sync::OnceLock;

// ── Parameters ────────────────────────────────────────────────────────────────

const STEP:      f32 = 0.14; // grid spacing (3 × 0.14 = 0.42, fits inside ±0.5)
const HALF:      i32 = 3;    // half-extent in cells
const GROW_RATE: f32 = 2.2;  // algorithm-time units revealed per real second
const FADE_IN:   f32 = 1.2;  // edge/node fade-in window (algorithm time)
const HOLD_SECS: f32 = 3.5;  // pause after full growth before looping

fn main() {
    size(960, 640);
    title("3D Lattice — organic growth");
    smooth(4);
    run3d(draw);
}

// ── Lattice data ──────────────────────────────────────────────────────────────

struct LatticeEdge {
    a:    Vec3,
    b:    Vec3,
    /// Algorithm-time at which this edge is fully revealed.
    t:    f32,
    /// Axis direction — 0 = +X, 1 = +Y, 2 = +Z.
    axis: u8,
    /// Manhattan distance of the nearer endpoint from origin.
    dist: f32,
}

struct LatticeNode {
    pos:  Vec3,
    /// Algorithm-time at which this node is first discovered.
    t:    f32,
    dist: f32,
}

static LATTICE: OnceLock<(Vec<LatticeEdge>, Vec<LatticeNode>, f32)> = OnceLock::new();

fn build_lattice() -> (Vec<LatticeEdge>, Vec<LatticeNode>, f32) {
    let mut node_t: HashMap<(i32,i32,i32), f32> = HashMap::new();
    let mut queue:  VecDeque<(i32,i32,i32)>     = VecDeque::new();
    let mut rng = SmallRng::seed_from_u64(31337);

    // Randomised BFS from the centre.
    // Each step has a random duration in [1, 4], so some paths advance
    // three times faster than others — giving organic, uneven growth.
    node_t.insert((0, 0, 0), 0.0);
    queue.push_back((0, 0, 0));

    while let Some((x, y, z)) = queue.pop_front() {
        let t = node_t[&(x, y, z)];

        // Shuffle the six axis-aligned directions so the BFS explores
        // in a different random order at every node.
        let mut dirs = [(-1i32,0i32,0i32),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1),(0,0,1)];
        for i in (1..6usize).rev() {
            let j = rng.random_range(0..=i);
            dirs.swap(i, j);
        }

        for (dx, dy, dz) in dirs {
            let (nx, ny, nz) = (x + dx, y + dy, z + dz);
            if nx.abs() > HALF || ny.abs() > HALF || nz.abs() > HALF { continue; }
            if node_t.contains_key(&(nx, ny, nz)) { continue; }

            let step: f32 = 1.0 + rng.random::<f32>() * 3.0;
            node_t.insert((nx, ny, nz), t + step);
            queue.push_back((nx, ny, nz));
        }
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────
    let mut nodes: Vec<LatticeNode> = Vec::new();
    for z in -HALF..=HALF {
        for y in -HALF..=HALF {
            for x in -HALF..=HALF {
                if let Some(&t) = node_t.get(&(x, y, z)) {
                    nodes.push(LatticeNode {
                        pos:  Vec3::new(x as f32 * STEP, y as f32 * STEP, z as f32 * STEP),
                        t,
                        dist: (x.abs() + y.abs() + z.abs()) as f32,
                    });
                }
            }
        }
    }

    // ── Edges (each emitted once, toward the +X / +Y / +Z neighbour) ──────────
    let mut edges: Vec<LatticeEdge> = Vec::new();
    for z in -HALF..=HALF {
        for y in -HALF..=HALF {
            for x in -HALF..=HALF {
                let ta = *node_t.get(&(x, y, z)).unwrap_or(&f32::MAX);
                for (dx, dy, dz, axis) in [(1i32,0i32,0i32,0u8),(0,1,0,1),(0,0,1,2)] {
                    let (nx, ny, nz) = (x + dx, y + dy, z + dz);
                    if nx.abs() > HALF || ny.abs() > HALF || nz.abs() > HALF { continue; }
                    let tb = *node_t.get(&(nx, ny, nz)).unwrap_or(&f32::MAX);
                    let dist = (x.abs().min(nx.abs())
                              + y.abs().min(ny.abs())
                              + z.abs().min(nz.abs())) as f32;
                    edges.push(LatticeEdge {
                        a:    Vec3::new(x  as f32 * STEP, y  as f32 * STEP, z  as f32 * STEP),
                        b:    Vec3::new(nx as f32 * STEP, ny as f32 * STEP, nz as f32 * STEP),
                        t:    ta.max(tb), // revealed when the later endpoint is discovered
                        axis,
                        dist,
                    });
                }
            }
        }
    }

    edges.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    let max_t = edges.last().map(|e| e.t).unwrap_or(1.0);
    (edges, nodes, max_t)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t.clamp(0.0, 1.0) }

fn project(p: Vec3, mvp: Mat4, w: f32, h: f32) -> Option<(f32, f32)> {
    let c = mvp * p.extend(1.0);
    if c.w < 0.001 { return None; }
    let (nx, ny) = (c.x / c.w, c.y / c.w);
    if nx.abs() > 2.5 || ny.abs() > 2.5 { return None; }
    Some(((nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h))
}

// ── Draw ──────────────────────────────────────────────────────────────────────

fn draw(scene: &mut Scene3D) {
    let t = scene.time;
    let w = scene.width;
    let h = scene.height;

    background(2, 1, 6);
    scene.orbit_enabled = true;
    scene.camera.fov_y  = std::f32::consts::FRAC_PI_4;
    scene.clear();

    let mvp = scene.camera.view_proj(scene.aspect());
    let (edges, nodes, max_t) = LATTICE.get_or_init(build_lattice);

    // Loop timing: grow → hold → fade out → restart.
    let grow_dur = max_t / GROW_RATE;
    let cycle    = grow_dur + HOLD_SECS;
    let t_local  = t % cycle;
    let reveal   = (t_local * GROW_RATE).min(max_t + FADE_IN);

    // Smooth global fade-out in the last 1.5 s of each cycle.
    let global_fade = if t_local > cycle - 1.5 {
        ((cycle - t_local) / 1.5).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let max_dist = (HALF * 3) as f32;

    // ── Edges ─────────────────────────────────────────────────────────────────
    no_fill();

    for edge in edges.iter() {
        // Edges are sorted by reveal time — safe to break early.
        if edge.t > reveal + FADE_IN { break; }

        // Per-edge fade: smoothly appears over FADE_IN algorithm-time units.
        let local_fade = ((reveal - edge.t + FADE_IN) / FADE_IN).clamp(0.0, 1.0);
        let fade = local_fade * global_fade;
        if fade < 0.01 { continue; }

        let centre = 1.0 - edge.dist / max_dist;
        let bright  = lerp(18.0, 230.0, centre * fade);
        let alpha   = lerp( 8.0, 195.0, centre * fade);

        stroke_weight(lerp(0.3, 1.6, centre));

        // Direction-coded colour.
        let (r, g, b): (f32, f32, f32) = match edge.axis {
            0 => (bright * 0.28, bright * 0.52, bright         ), // X → blue
            1 => (bright * 0.22, bright,         bright * 0.70 ), // Y → teal
            _ => (bright * 0.18, bright,         bright * 0.40 ), // Z → green
        };

        stroke_a(r as u8, g as u8, b as u8, alpha as u8);

        if let (Some(p0), Some(p1)) = (project(edge.a, mvp, w, h), project(edge.b, mvp, w, h)) {
            line(p0.0, p0.1, p1.0, p1.1);
        }
    }

    // ── Nodes ─────────────────────────────────────────────────────────────────
    no_stroke();

    for node in nodes.iter() {
        let local_fade = ((reveal - node.t + FADE_IN) / FADE_IN).clamp(0.0, 1.0);
        let fade = local_fade * global_fade;
        if fade < 0.01 { continue; }

        let centre = 1.0 - node.dist / max_dist;
        let bright  = lerp(25.0, 255.0, centre * fade) as u8;
        let alpha   = lerp(15.0, 215.0, centre * fade) as u8;
        let radius  = lerp(0.6, 2.6, centre);

        fill_a(bright, (bright as f32 * 1.02).min(255.0) as u8, bright, alpha);

        if let Some((sx, sy)) = project(node.pos, mvp, w, h) {
            ellipse(sx - radius, sy - radius, radius * 2.0, radius * 2.0);
        }
    }

    // ── Bounding cube ─────────────────────────────────────────────────────────
    let s = 0.5_f32;
    no_fill();
    stroke_weight(0.5);
    stroke_a(38, 32, 70, (60.0 * global_fade) as u8);

    let cube_edges: &[(Vec3, Vec3)] = &[
        (Vec3::new(-s,-s,-s), Vec3::new( s,-s,-s)),
        (Vec3::new( s,-s,-s), Vec3::new( s, s,-s)),
        (Vec3::new( s, s,-s), Vec3::new(-s, s,-s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s,-s,-s)),
        (Vec3::new(-s,-s, s), Vec3::new( s,-s, s)),
        (Vec3::new( s,-s, s), Vec3::new( s, s, s)),
        (Vec3::new( s, s, s), Vec3::new(-s, s, s)),
        (Vec3::new(-s, s, s), Vec3::new(-s,-s, s)),
        (Vec3::new(-s,-s,-s), Vec3::new(-s,-s, s)),
        (Vec3::new( s,-s,-s), Vec3::new( s,-s, s)),
        (Vec3::new( s, s,-s), Vec3::new( s, s, s)),
        (Vec3::new(-s, s,-s), Vec3::new(-s, s, s)),
    ];
    for (a, b) in cube_edges {
        if let (Some(p0), Some(p1)) = (project(*a, mvp, w, h), project(*b, mvp, w, h)) {
            line(p0.0, p0.1, p1.0, p1.1);
        }
    }
}
