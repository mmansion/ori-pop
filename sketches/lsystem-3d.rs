//! Boid-driven toolpath sketch.
//!
//! Each agent has a position, velocity, and a growing trail.  Forces applied
//! every frame:
//!   • Separation from other agents (repulsion within SEP_RADIUS)
//!   • Own-trail avoidance (repels from its own recent path → prevents loops)
//!   • Soft boundary repulsion (stays inside the ±BOUND cube)
//!   • Random perturbation (organic variation)
//!
//! The trail is drawn as a continuous extruding line — the tip grows each
//! frame exactly like a toolpath or pen being dragged across the surface.
//!
//! Controls:
//!   Right-drag — orbit   |   Scroll — zoom   |   Space — inspector
//!
//! Run with:
//!   cargo run --bin lsystem-3d

use oripop_3d::prelude::*;
use rand::{SeedableRng, Rng};
use rand::rngs::SmallRng;
use std::collections::VecDeque;
use std::cell::RefCell;

// ── Simulation parameters ─────────────────────────────────────────────────────

const NUM_AGENTS: usize = 10;
/// Maximum trail length per agent (older points are removed from the front).
const MAX_TRAIL:  usize = 2500;
const MAX_SPEED:  f32   = 0.30;
const MIN_SPEED:  f32   = 0.06;
/// Distance within which agents repel each other.
const SEP_RADIUS: f32   = 0.24;
const SEP_FORCE:  f32   = 0.10;
/// Soft wall distance from cube centre.
const BOUND:      f32   = 0.42;
const WALL_FORCE: f32   = 0.55;
/// Damping applied to velocity each frame (< 1 → gradually slows if no force).
const DAMPING:    f32   = 0.992;
/// Amplitude of the random perturbation added each frame.
const NOISE_AMP:  f32   = 0.022;

// ── Agent colours ─────────────────────────────────────────────────────────────

#[rustfmt::skip]
const COLORS: &[(u8, u8, u8)] = &[
    (100, 160, 255), // blue
    ( 80, 225, 175), // teal
    (110, 235, 110), // green
    (255, 215,  80), // yellow
    (255, 130,  55), // orange
    (255,  75, 120), // pink
    (175,  75, 255), // violet
    ( 75, 205, 255), // sky
    (255, 155,  80), // amber
    (130, 255, 180), // mint
];

// ── Agent ─────────────────────────────────────────────────────────────────────

struct Agent {
    pos:   Vec3,
    vel:   Vec3,
    trail: VecDeque<Vec3>,
    color: (u8, u8, u8),
}

impl Agent {
    fn new(pos: Vec3, vel: Vec3, color: (u8, u8, u8)) -> Self {
        Self { pos, vel, trail: VecDeque::new(), color }
    }
}

// ── Simulation ────────────────────────────────────────────────────────────────

struct Sim {
    agents:    Vec<Agent>,
    rng:       SmallRng,
    last_time: f32,
}

impl Sim {
    fn new() -> Self {
        let rng = SmallRng::seed_from_u64(99991);

        // Spread initial directions evenly using the golden angle on a sphere.
        let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        let agents = (0..NUM_AGENTS).map(|i| {
            let theta = i as f32 * golden;
            let phi   = std::f32::consts::FRAC_PI_2
                        * (1.0 - 2.0 * i as f32 / NUM_AGENTS as f32);
            let dir   = Vec3::new(phi.cos() * theta.cos(),
                                  phi.cos() * theta.sin(),
                                  phi.sin());
            let vel   = dir * 0.20;
            let pos   = dir * 0.04; // small offset from centre
            Agent::new(pos, vel, COLORS[i % COLORS.len()])
        }).collect();

        Self { agents, rng, last_time: 0.0 }
    }

    fn step(&mut self, now: f32) {
        let dt = (now - self.last_time).clamp(0.0, 0.05);
        self.last_time = now;
        if dt <= 0.0 { return; }

        // Snapshot positions so all agents see the state from the same instant.
        let positions: Vec<Vec3> = self.agents.iter().map(|a| a.pos).collect();

        for (i, agent) in self.agents.iter_mut().enumerate() {
            let mut acc = Vec3::ZERO;

            // ── Separation from other agents ──────────────────────────────
            for (j, &other) in positions.iter().enumerate() {
                if j == i { continue; }
                let diff = agent.pos - other;
                let dist = diff.length();
                if dist < SEP_RADIUS && dist > 0.001 {
                    let strength = SEP_FORCE * (1.0 - dist / SEP_RADIUS);
                    acc += diff.normalize() * strength;
                }
            }

            // ── Own-trail avoidance (recent 60 points, skip 5 newest) ─────
            // This makes agents curve away from their own wake — preventing
            // tight re-tracing loops and encouraging space-filling behaviour.
            let trail_len = agent.trail.len();
            let check = 60.min(trail_len);
            if check > 5 {
                for &past in agent.trail.iter().rev().take(check).skip(5) {
                    let diff = agent.pos - past;
                    let dist = diff.length();
                    if dist < 0.11 && dist > 0.001 {
                        acc += diff.normalize() * 0.045 / dist.max(0.02);
                    }
                }
            }

            // ── Soft boundary repulsion ───────────────────────────────────
            for dim in 0..3usize {
                let p = agent.pos[dim];
                if p > BOUND {
                    acc[dim] -= WALL_FORCE * (p - BOUND);
                } else if p < -BOUND {
                    acc[dim] += WALL_FORCE * (-BOUND - p);
                }
            }

            // ── Random perturbation ───────────────────────────────────────
            acc += Vec3::new(
                self.rng.random::<f32>() * 2.0 - 1.0,
                self.rng.random::<f32>() * 2.0 - 1.0,
                self.rng.random::<f32>() * 2.0 - 1.0,
            ) * NOISE_AMP;

            // ── Maintain minimum speed ────────────────────────────────────
            if agent.vel.length() < MIN_SPEED {
                let rand_dir = Vec3::new(
                    self.rng.random::<f32>() * 2.0 - 1.0,
                    self.rng.random::<f32>() * 2.0 - 1.0,
                    self.rng.random::<f32>() * 2.0 - 1.0,
                ).normalize_or(Vec3::X);
                acc += rand_dir * 0.12;
            }

            // ── Integrate ─────────────────────────────────────────────────
            agent.vel = (agent.vel + acc * dt) * DAMPING;
            agent.vel  = agent.vel.clamp_length_max(MAX_SPEED);
            agent.pos += agent.vel * dt;

            // Hard clamp so agents never escape the cube.
            agent.pos = agent.pos.clamp(Vec3::splat(-0.45), Vec3::splat(0.45));

            // Grow trail: append current position.
            agent.trail.push_back(agent.pos);
            if agent.trail.len() > MAX_TRAIL {
                agent.trail.pop_front(); // remove oldest
            }
        }
    }

    fn render(&self, mvp: Mat4, w: f32, h: f32) {
        no_fill();

        for agent in &self.agents {
            let n = agent.trail.len();
            if n < 2 { continue; }

            let (cr, cg, cb) = agent.color;
            let mut prev_screen: Option<(f32, f32)> = None;

            for (idx, &pt) in agent.trail.iter().enumerate() {
                let screen = project(pt, mvp, w, h);
                if let (Some(p0), Some(p1)) = (prev_screen, screen) {
                    // freshness: 0.0 = oldest, 1.0 = newest
                    let freshness = idx as f32 / n as f32;
                    let alpha_t   = freshness.powf(1.6); // non-linear fade

                    let alpha  = (alpha_t * 210.0) as u8;
                    let bright = 0.25 + 0.75 * alpha_t;
                    let weight = 0.35 + alpha_t * 1.6;

                    stroke_weight(weight);
                    stroke_a(
                        (cr as f32 * bright) as u8,
                        (cg as f32 * bright) as u8,
                        (cb as f32 * bright) as u8,
                        alpha,
                    );
                    line(p0.0, p0.1, p1.0, p1.1);
                }
                prev_screen = screen;
            }

            // Bright dot at the agent's current tip.
            no_stroke();
            fill_a(cr, cg, cb, 255);
            if let Some((sx, sy)) = project(agent.pos, mvp, w, h) {
                ellipse(sx - 2.5, sy - 2.5, 5.0, 5.0);
            }
            no_fill();
        }
    }
}

// ── Thread-local sim state ────────────────────────────────────────────────────

thread_local! {
    static SIM: RefCell<Option<Sim>> = RefCell::new(None);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn project(p: Vec3, mvp: Mat4, w: f32, h: f32) -> Option<(f32, f32)> {
    let c = mvp * p.extend(1.0);
    if c.w < 0.001 { return None; }
    let (nx, ny) = (c.x / c.w, c.y / c.w);
    if nx.abs() > 2.5 || ny.abs() > 2.5 { return None; }
    Some(((nx + 1.0) * 0.5 * w, (1.0 - ny) * 0.5 * h))
}

// ── Draw loop ─────────────────────────────────────────────────────────────────

fn main() {
    size(960, 640);
    title("Boid toolpaths — extruding trails with force & avoidance");
    smooth(4);
    run3d(draw);
}

fn draw(scene: &mut Scene3D) {
    let w = scene.width;
    let h = scene.height;
    let t = scene.time;

    background(2, 1, 6);
    scene.orbit_enabled = true;
    scene.auto_spin     = true;
    scene.spin_speed    = 0.22;
    scene.camera.fov_y  = std::f32::consts::FRAC_PI_4;
    scene.clear();

    let mvp = scene.camera.view_proj(scene.aspect());

    // Advance simulation and render trails.
    SIM.with(|cell| {
        let mut opt = cell.borrow_mut();
        let sim = opt.get_or_insert_with(Sim::new);
        sim.step(t);
        sim.render(mvp, w, h);
    });

    // ── Bounding cube ─────────────────────────────────────────────────────────
    let s = 0.5_f32;
    no_fill();
    stroke_weight(0.5);
    stroke_a(38, 32, 70, 55);

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
