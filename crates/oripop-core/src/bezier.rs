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
