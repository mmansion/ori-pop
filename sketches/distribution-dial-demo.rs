//! Prototype mutator UI for [`oripop_canvas::field`] distributions (see `ROADMAP.md` §0b).
//!
//! - Wireframe circle: constraint region (dots are filtered inside it).
//! - **East handle:** drag to resize radius.
//! - **Dial handle** on the perimeter: drag around the circle; angle sets a **weight** mixing
//!   directional gradient vs softer isotropic density.
//! - **Center offset dot:** drag from the hub to set **gradient direction** (toward brighter density).
//! - **Bottom slider:** drag to change target dot count (sparse default; range about 200–6.5k).
//! - **`H` key:** toggle UI handles, wireframe, and slider on/off (dots stay visible).
//!
//! ```bash
//! cargo run -p sketches --bin distribution-dial-demo
//! ```

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use oripop_canvas::prelude::*;
use oripop_canvas::{generate_dots, Dot, Params, Singularity};

const W: f32 = 800.0;
const H: f32 = 800.0;
const SLIDER_Y: f32 = 762.0;
const SLIDER_X0: f32 = 140.0;
const SLIDER_X1: f32 = 660.0;
/// Sparse by default; slider ceiling is ~10× the previous cap for dense stipple at the focal point.
const DOT_MIN: u32 = 200;
const DOT_MAX: u32 = 65_000;
/// Minimum delay between dot regenerations while a drag is active. ~30 Hz keeps the
/// visible field following the cursor without generating a full distribution on every mouse event.
const DRAG_REGEN_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Drag {
    Dial,
    Radius,
    GradientTip,
    Slider,
}

struct AppState {
    last_mouse_pressed: bool,
    drag:             Option<Drag>,
    cx:               f32,
    cy:               f32,
    radius:           f32,
    /// Angle (rad) of the dial knob on the circle; `rem_euclid(TAU) / TAU` → mix weight.
    dial_angle:       f32,
    /// Gradient tip offset from `(cx, cy)` in pixels.
    gtip_x:           f32,
    gtip_y:           f32,
    dot_count:        u32,
    /// Fingerprint of the last inputs we ran `generate_dots` for. When unchanged, we reuse
    /// [`AppState::cached_dots`] so redraws are just vertex emission.
    cache_key:        Option<[u32; 7]>,
    cached_dots:      Vec<Dot>,
    /// Timestamp of the last successful `generate_dots`. Used to throttle regeneration during drag.
    last_gen:         Option<Instant>,
    /// True on the frame the user released the mouse — forces a final full-rate regen to match the slider exactly.
    needs_final:      bool,
    /// Visibility of the mutator UI (handles, wireframe circle, slider). Toggled with `H`.
    ui_visible:       bool,
    /// Previous-frame `key_pressed()` so we only toggle on the rising edge.
    last_key_pressed: bool,
}

fn initial_state() -> AppState {
    AppState {
        last_mouse_pressed: false,
        drag:       None,
        cx:         W * 0.5,
        cy:         H * 0.5 - 24.0,
        radius:     190.0,
        dial_angle: std::f32::consts::FRAC_PI_2,
        gtip_x:     95.0,
        gtip_y:     -18.0,
        dot_count:  1_400,
        cache_key:    None,
        cached_dots:  Vec::new(),
        last_gen:         None,
        needs_final:      false,
        ui_visible:       true,
        last_key_pressed: false,
    }
}

fn state_key(s: &AppState) -> [u32; 7] {
    [
        s.cx.to_bits(),
        s.cy.to_bits(),
        s.radius.to_bits(),
        s.dial_angle.to_bits(),
        s.gtip_x.to_bits(),
        s.gtip_y.to_bits(),
        s.dot_count,
    ]
}

fn app() -> &'static Mutex<AppState> {
    static S: OnceLock<Mutex<AppState>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(initial_state()))
}

fn dist(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    (dx * dx + dy * dy).sqrt()
}

fn hit(mx: f32, my: f32, s: &AppState) -> Option<Drag> {
    if my >= SLIDER_Y - 22.0
        && my <= SLIDER_Y + 30.0
        && mx >= SLIDER_X0 - 12.0
        && mx <= SLIDER_X1 + 12.0
    {
        return Some(Drag::Slider);
    }

    let tcx = s.cx + s.gtip_x;
    let tcy = s.cy + s.gtip_y;
    if dist(mx, my, tcx, tcy) < 15.0 {
        return Some(Drag::GradientTip);
    }

    let dhx = s.cx + s.radius * s.dial_angle.cos();
    let dhy = s.cy + s.radius * s.dial_angle.sin();
    if dist(mx, my, dhx, dhy) < 17.0 {
        return Some(Drag::Dial);
    }

    let rhx = s.cx + s.radius;
    let rhy = s.cy;
    if dist(mx, my, rhx, rhy) < 17.0 {
        return Some(Drag::Radius);
    }

    None
}

fn dial_mix(s: &AppState) -> f32 {
    (s.dial_angle.rem_euclid(std::f32::consts::TAU)) / std::f32::consts::TAU
}

fn clamp_gradient_tip(s: &mut AppState) {
    let max_d = (s.radius - 28.0).max(24.0);
    let mut vx = s.gtip_x;
    let mut vy = s.gtip_y;
    let d = (vx * vx + vy * vy).sqrt();
    if d > max_d && d > 1e-6 {
        vx *= max_d / d;
        vy *= max_d / d;
    }
    s.gtip_x = vx;
    s.gtip_y = vy;
}

fn update_input(s: &mut AppState) {
    let mx = mouse_x();
    let my = mouse_y();
    let pressed = mouse_pressed();

    if pressed && !s.last_mouse_pressed {
        s.drag = hit(mx, my, s);
    }

    if let Some(kind) = s.drag {
        if pressed {
            match kind {
                Drag::Dial => {
                    s.dial_angle = (my - s.cy).atan2(mx - s.cx);
                }
                Drag::Radius => {
                    let r = dist(mx, my, s.cx, s.cy).clamp(48.0, f32::min(W, H) * 0.46);
                    s.radius = r;
                    clamp_gradient_tip(s);
                }
                Drag::GradientTip => {
                    let mut vx = mx - s.cx;
                    let mut vy = my - s.cy;
                    let max_d = (s.radius - 28.0).max(24.0);
                    let d = (vx * vx + vy * vy).sqrt();
                    if d > max_d && d > 1e-6 {
                        vx *= max_d / d;
                        vy *= max_d / d;
                    }
                    s.gtip_x = vx;
                    s.gtip_y = vy;
                }
                Drag::Slider => {
                    let t = ((mx - SLIDER_X0) / (SLIDER_X1 - SLIDER_X0).max(1.0)).clamp(0.0, 1.0);
                    s.dot_count = (DOT_MIN as f32 + t * (DOT_MAX - DOT_MIN) as f32)
                        .round()
                        .clamp(DOT_MIN as f32, DOT_MAX as f32) as u32;
                }
            }
        } else {
            s.drag = None;
        }
    } else if !pressed {
        s.drag = None;
    }

    let released = s.last_mouse_pressed && !pressed;
    if released {
        s.needs_final = true;
    }
    s.last_mouse_pressed = pressed;

    let key_down = key_pressed();
    if key_down && !s.last_key_pressed {
        let k = key();
        if k == 'h' || k == 'H' {
            s.ui_visible = !s.ui_visible;
        }
    }
    s.last_key_pressed = key_down;
}

fn build_params(s: &AppState) -> Params {
    let mix = dial_mix(s);
    let ntx = (s.cx + s.gtip_x) / W;
    let nty = (s.cy + s.gtip_y) / H;
    let norm_radius = (s.radius / W).clamp(0.04, 0.5);

    // Falloff dominates the clustering shape: higher value → tighter cluster at the focal dot.
    // Normalized so the peak stays inside the visual circle regardless of radius. Dial mix tightens it further.
    let base_falloff = 1.2 / (norm_radius * norm_radius);
    let falloff = base_falloff * (1.0 + 1.5 * mix);

    let mut params = Params::default();
    params.canvas.width = W;
    params.canvas.height = H;
    params.distribution.dot_count = s.dot_count.clamp(DOT_MIN, DOT_MAX);
    params.distribution.min_radius = 0.00055;
    params.distribution.max_radius = 0.00115;
    // Sharper acceptance → density falls off aggressively away from the focal dot.
    params.distribution.density_pow = 2.4;
    // No independent center singularity; the focal attractor is the only density peak.
    params.field.singularity = Singularity {
        cx:       ntx.clamp(0.002, 0.998),
        cy:       nty.clamp(0.002, 0.998),
        falloff,
        strength: 1.0,
    };
    params.field.warp_amount = 0.015 * (1.0 - mix);
    params.field.warp_frequency = 6.0;
    params.field.forces = Vec::new();

    params
}

/// Filter samples into the circle. With the focal attractor inside the disk, rejection sampling
/// already concentrates dots near the peak, so we start close to `target` and grow the request
/// only if too many fell outside the circle.
fn dots_in_circle(params: &Params, t: f32, cx: f32, cy: f32, r: f32, target: usize) -> Vec<Dot> {
    let r2 = r * r;
    let gen_cap = ((target as f32) * 3.0).ceil() as u32;
    let mut n_gen = ((target as f32) * 1.15).ceil() as u32;
    n_gen = n_gen.clamp(target as u32 + 32, gen_cap);

    let mut out = Vec::with_capacity(target);
    for attempt in 0..6 {
        let mut p = params.clone();
        p.distribution.dot_count = n_gen;
        let batch = generate_dots(&p, t + attempt as f32 * 1e-4);
        out.clear();
        for d in batch {
            let dx = d.x - cx;
            let dy = d.y - cy;
            if dx * dx + dy * dy <= r2 && out.len() < target {
                out.push(d);
            }
        }
        if out.len() >= target {
            break;
        }
        let bump = ((target - out.len()) as u32).saturating_mul(2).max(256);
        n_gen = (n_gen + bump).min(gen_cap);
    }
    out.truncate(target);
    out
}

fn main() {
    size(800, 800);
    title("distribution dial");
    smooth(4);
    redraw_continuous(false);
    run(draw);
}

fn draw() {
    background(0, 0, 0);

    let (cx, cy, r) = {
        let mut s = app().lock().unwrap();
        update_input(&mut *s);

        let key = state_key(&s);
        let keys_differ = s.cache_key != Some(key);
        let dragging = s.drag.is_some();
        let throttled = dragging
            && !s.needs_final
            && s.last_gen
                .map(|t| t.elapsed() < DRAG_REGEN_INTERVAL)
                .unwrap_or(false);

        if keys_differ && !throttled {
            let params = build_params(&s);
            let cx = s.cx;
            let cy = s.cy;
            let radius = s.radius;
            let target = s.dot_count as usize;
            s.cached_dots = dots_in_circle(&params, 0.0, cx, cy, radius, target);
            s.cache_key = Some(key);
            s.last_gen = Some(Instant::now());
            s.needs_final = false;
        }
        (s.cx, s.cy, s.radius)
    };

    let s = app().lock().unwrap();

    no_stroke();
    for dot in &s.cached_dots {
        let v = (dot.w * 200.0 + 55.0).min(255.0) as u8;
        fill(v, v, v);
        let sz = dot.r * W * 1.25;
        rect(dot.x - sz * 0.5, dot.y - sz * 0.5, sz, sz);
    }

    if !s.ui_visible {
        return;
    }

    // Wireframe circle (mutator boundary)
    no_fill();
    stroke(255, 255, 255);
    stroke_weight(2.0);
    ellipse(cx - r, cy - r, r * 2.0, r * 2.0);

    // Hub → focal-dot guide line
    stroke_a(255, 255, 255, 110);
    stroke_weight(1.5);
    line(cx, cy, cx + s.gtip_x, cy + s.gtip_y);

    // Radius handle (east) — hollow so it reads as a control without adding a tone
    no_fill();
    stroke(255, 255, 255);
    stroke_weight(2.0);
    let rhx = cx + r;
    let rhy = cy;
    ellipse(rhx - 9.0, rhy - 9.0, 18.0, 18.0);

    // Dial handle — filled white disc
    no_stroke();
    fill(255, 255, 255);
    let dhx = cx + r * s.dial_angle.cos();
    let dhy = cy + r * s.dial_angle.sin();
    ellipse(dhx - 10.0, dhy - 10.0, 20.0, 20.0);

    // Focal direction dot — filled white, slightly larger
    fill(255, 255, 255);
    let tcx = cx + s.gtip_x;
    let tcy = cy + s.gtip_y;
    ellipse(tcx - 11.0, tcy - 11.0, 22.0, 22.0);

    // Slider track
    no_fill();
    stroke(160, 160, 160);
    stroke_weight(3.0);
    rect(SLIDER_X0, SLIDER_Y, SLIDER_X1 - SLIDER_X0, 6.0);

    let t = (s.dot_count.saturating_sub(DOT_MIN)) as f32 / (DOT_MAX - DOT_MIN).max(1) as f32;
    let thumb_x = SLIDER_X0 + t.clamp(0.0, 1.0) * (SLIDER_X1 - SLIDER_X0);
    no_stroke();
    fill(255, 255, 255);
    ellipse(thumb_x - 8.0, SLIDER_Y - 2.0, 16.0, 16.0);
}
