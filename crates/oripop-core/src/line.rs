use serde::{Deserialize, Serialize};

use crate::point::Point;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Line {
    pub a: Point,
    pub b: Point,
}

impl Line {
    pub fn new(a: Point, b: Point) -> Self {
        Self { a, b }
    }

    pub fn length(&self) -> f32 {
        self.a.dist(&self.b)
    }

    pub fn midpoint(&self) -> Point {
        self.a.lerp(&self.b, 0.5)
    }

    pub fn lerp(&self, t: f32) -> Point {
        self.a.lerp(&self.b, t)
    }

    pub fn nearest(&self, p: &Point) -> Point {
        let dx = self.b.x - self.a.x;
        let dy = self.b.y - self.a.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 1e-10 {
            return self.a;
        }
        let t = ((p.x - self.a.x) * dx + (p.y - self.a.y) * dy) / len_sq;
        let t = t.clamp(0.0, 1.0);
        Point::new(self.a.x + dx * t, self.a.y + dy * t)
    }

    pub fn distance(&self, p: &Point) -> f32 {
        p.dist(&self.nearest(p))
    }
}
