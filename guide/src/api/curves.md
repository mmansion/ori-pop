# Curves

## Drawing curves

```rust
bezier(x1, y1, cx1, cy1, cx2, cy2, x2, y2);  // cubic bezier (stroked)
curve(cx1, cy1, x1, y1, x2, y2, cx2, cy2);   // Catmull-Rom through x1..x2
```

`bezier` draws from the first to the last point, pulled by two control
points. `curve` draws the Catmull-Rom segment between its two *middle*
points; the outer points shape the curve as neighbors.

For filled curved shapes, use the custom shape builder
(`begin_shape` + `bezier_vertex` / `curve_vertex` — see
[Shapes & Primitives](./shapes.md)).

## Evaluating curves

Pure math, one coordinate at a time — useful for placing things *along* a
curve (and, later, for converting drawn curves into fabrication toolpaths):

```rust
let x = bezier_point(x1, cx1, cx2, x2, t);   // position at t in [0, 1]
let dx = bezier_tangent(x1, cx1, cx2, x2, t); // derivative at t
let x = curve_point(a, b, c, d, t);          // Catmull-Rom between b and c
let dx = curve_tangent(a, b, c, d, t);
```

Example — dots marching along a bezier:

```rust
for i in 0..=20 {
    let t = i as f32 / 20.0;
    let x = bezier_point(100.0, 250.0, 550.0, 700.0, t);
    let y = bezier_point(500.0, 100.0, 100.0, 500.0, t);
    circle(x, y, 6.0);
}
```
