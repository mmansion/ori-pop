# Shapes & Primitives

All shapes respect the current fill and stroke settings. Geometry is
tessellated through [lyon](https://github.com/nical/lyon), so strokes have
proper caps and joins at any weight.

## Basic primitives

```rust
point(x, y);                  // round dot, diameter = stroke_weight
line(x1, y1, x2, y2);
rect(x, y, w, h);             // top-left corner (see rect_mode)
square(x, y, s);
ellipse(cx, cy, w, h);        // centered (see ellipse_mode)
circle(cx, cy, d);
triangle(x1, y1, x2, y2, x3, y3);
quad(x1, y1, x2, y2, x3, y3, x4, y4);
```

> **Processing note:** `ellipse` uses CENTER mode by default and `rect` uses
> CORNER mode, exactly like Processing.

## Arcs

```rust
arc(cx, cy, w, h, start, stop);                      // radians
arc_with_mode(cx, cy, w, h, start, stop, ArcMode::Pie);
```

Angles are in radians; 0 points along +x and angles increase clockwise on
screen (y grows down). The default rendering matches Processing: an open
stroke with a pie-shaped fill. `ArcMode::Chord` closes across the chord,
`ArcMode::Pie` closes through the center.

## Shape modes

```rust
rect_mode(ShapeMode::Center);     // Corner (default) | Corners | Center | Radius
ellipse_mode(ShapeMode::Corner);  // Center (default) | Corner | Corners | Radius
```

## Stroke caps & joins

```rust
stroke_cap(StrokeCap::Round);     // Round (default) | Square | Project
stroke_join(StrokeJoin::Miter);   // Miter (default) | Bevel | Round
```

Processing naming applies: `Square` is a flat cap at the endpoint, `Project`
extends past it by half the stroke weight.

## Custom shapes

Build arbitrary polygons and curved outlines vertex by vertex:

```rust
begin_shape();
vertex(x, y);                           // straight segment
bezier_vertex(cx1, cy1, cx2, cy2, x, y); // cubic segment
quadratic_vertex(cx, cy, x, y);          // quadratic segment
curve_vertex(x, y);                      // Catmull-Rom (first/last = controls)
end_shape();                             // open outline
end_shape_close();                       // closed outline
```

Cut holes with contours — even-odd filling makes them read as true holes:

```rust
begin_shape();
vertex(0.0, 0.0); vertex(100.0, 0.0); vertex(100.0, 100.0); vertex(0.0, 100.0);
begin_contour();
vertex(25.0, 25.0); vertex(75.0, 25.0); vertex(75.0, 75.0); vertex(25.0, 75.0);
end_contour();
end_shape_close();
```
