# Shapes & Primitives

All shapes respect the current fill and stroke settings.

## Line

```rust
line(x1, y1, x2, y2);
```

Drawn with the current stroke color and weight. Ignored when stroke is disabled.

## Point

```rust
point(x, y);
```

A small filled square at the given position, sized by `stroke_weight`.

## Rectangle

```rust
rect(x, y, width, height);
```

Top-left corner at (`x`, `y`). Filled and/or stroked based on current settings.

## Ellipse

```rust
ellipse(x, y, width, height);
```

Bounded by the rectangle at (`x`, `y`) with given dimensions. Use equal width and height for a circle.

## Triangle

```rust
triangle(x1, y1, x2, y2, x3, y3);
```

Three vertices. Filled and/or stroked based on current settings.
