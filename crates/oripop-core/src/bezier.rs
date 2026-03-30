use serde::{Deserialize, Serialize};

use crate::point::Point;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Bezier {
    pub p0: Point,
    pub p1: Point,
    pub p2: Point,
    pub p3: Point,
}

impl Bezier {
    pub fn new(p0: Point, p1: Point, p2: Point, p3: Point) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// Parameter \(t \in [0,1]\) on the curve that is closest to `p`.
    ///
    /// Coarse samples are refined with a few Gauss–Newton style steps using the analytic tangent.
    pub fn closest_param(&self, p: &Point, search_steps: u32) -> f32 {
        let n = search_steps.clamp(8, 4096);
        let mut best_t = 0.0f32;
        let mut best_d = f32::MAX;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let d = p.dist_sq(&self.eval(t));
            if d < best_d {
                best_d = d;
                best_t = t;
            }
        }

        let mut t = best_t;
        for _ in 0..8 {
            let q = self.eval(t);
            let tang = self.tangent(t);
            let tang_len_sq = tang.x * tang.x + tang.y * tang.y;
            if tang_len_sq < 1e-12 {
                break;
            }
            let vx = q.x - p.x;
            let vy = q.y - p.y;
            let dt = (tang.x * vx + tang.y * vy) / tang_len_sq;
            t = (t - dt).clamp(0.0, 1.0);
        }
        t
    }

    pub fn eval(&self, t: f32) -> Point {
        let u = 1.0 - t;
        let u2 = u * u;
        let t2 = t * t;
        Point::new(
            u2 * u * self.p0.x + 3.0 * u2 * t * self.p1.x + 3.0 * u * t2 * self.p2.x + t2 * t * self.p3.x,
            u2 * u * self.p0.y + 3.0 * u2 * t * self.p1.y + 3.0 * u * t2 * self.p2.y + t2 * t * self.p3.y,
        )
    }

    pub fn tangent(&self, t: f32) -> Point {
        let u = 1.0 - t;
        Point::new(
            3.0 * u * u * (self.p1.x - self.p0.x) + 6.0 * u * t * (self.p2.x - self.p1.x) + 3.0 * t * t * (self.p3.x - self.p2.x),
            3.0 * u * u * (self.p1.y - self.p0.y) + 6.0 * u * t * (self.p2.y - self.p1.y) + 3.0 * t * t * (self.p3.y - self.p2.y),
        )
    }

    pub fn nearest(&self, p: &Point, steps: u32) -> Point {
        let mut best = self.p0;
        let mut best_d = f32::MAX;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let q = self.eval(t);
            let d = p.dist_sq(&q);
            if d < best_d {
                best_d = d;
                best = q;
            }
        }
        best
    }

    pub fn distance(&self, p: &Point, steps: u32) -> f32 {
        p.dist(&self.nearest(p, steps))
    }
}

fn default_density_t1() -> f32 {
    1.0 / 3.0
}
fn default_density_t2() -> f32 {
    2.0 / 3.0
}

/// Piecewise-linear density response along a path parameter \(t \in [0,1]\).
///
/// Knots at `t = 0`, `t1`, `t2`, `1` with multipliers `y0`…`y3`. Moving `t1` and `t2` shifts where
/// transitions occur along the curve (see [`DensityProfile::multiplier_at`]).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DensityProfile {
    pub y0: f32,
    pub y1: f32,
    pub y2: f32,
    pub y3: f32,
    #[serde(default = "default_density_t1")]
    pub t1: f32,
    #[serde(default = "default_density_t2")]
    pub t2: f32,
}

impl Default for DensityProfile {
    fn default() -> Self {
        Self {
            y0: 0.1,
            y1: 0.45,
            y2: 0.75,
            y3: 0.2,
            t1: default_density_t1(),
            t2: default_density_t2(),
        }
    }
}

impl DensityProfile {
    /// Multiplier in \([0,1]\): piecewise linear through \((0,y0)\), \((t_1,y1)\), \((t_2,y2)\), \((1,y3)\).
    pub fn multiplier_at(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let y0 = self.y0.clamp(0.0, 1.0);
        let y1 = self.y1.clamp(0.0, 1.0);
        let y2 = self.y2.clamp(0.0, 1.0);
        let y3 = self.y3.clamp(0.0, 1.0);
        let gap = (1.0_f32 / 64.0_f32).max(1e-4_f32);
        let mut t1 = self.t1.clamp(gap, 1.0 - 2.0 * gap);
        let mut t2 = self.t2.clamp(t1 + gap, 1.0 - gap);
        if t2 < t1 + gap {
            t2 = (t1 + gap).min(1.0 - gap);
        }
        if t1 > t2 - gap {
            t1 = (t2 - gap).max(gap);
        }

        let m = if t <= t1 {
            if t1 <= 1e-8 {
                y0
            } else {
                y0 + (y1 - y0) * (t / t1)
            }
        } else if t <= t2 {
            let span = (t2 - t1).max(1e-8);
            y1 + (y2 - y1) * ((t - t1) / span)
        } else {
            let span = (1.0 - t2).max(1e-8);
            y2 + (y3 - y2) * ((t - t2) / span)
        };
        m.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closest_param_endpoints() {
        let b = Bezier::new(
            Point::new(0.0, 0.0),
            Point::new(0.3, 0.0),
            Point::new(0.7, 0.0),
            Point::new(1.0, 0.0),
        );
        assert!(b.closest_param(&Point::new(0.0, 0.0), 64) < 1e-3);
        assert!((b.closest_param(&Point::new(1.0, 0.0), 64) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn density_profile_endpoints() {
        let p = DensityProfile {
            y0: 0.25,
            y1: 0.5,
            y2: 0.5,
            y3: 0.9,
            t1: 0.25,
            t2: 0.75,
        };
        assert!((p.multiplier_at(0.0) - 0.25).abs() < 1e-5);
        assert!((p.multiplier_at(1.0) - 0.9).abs() < 1e-5);
    }

    #[test]
    fn density_sliding_t1_changes_midpoint() {
        let a = DensityProfile {
            y0: 0.0,
            y1: 1.0,
            y2: 0.0,
            y3: 1.0,
            t1: 0.25,
            t2: 0.75,
        };
        let mut b = a;
        b.t1 = 0.55;
        assert!(b.multiplier_at(0.4) > a.multiplier_at(0.4));
    }
}
