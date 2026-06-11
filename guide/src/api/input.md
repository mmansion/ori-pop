# Input, Events & Time

## Polled state

Query input anywhere inside `draw()`:

```rust
mouse_x()  /  mouse_y()        // current position (logical pixels)
pmouse_x() /  pmouse_y()       // position at the previous frame
mouse_pressed()                // any button held?
mouse_button()                 // Some(MouseButton::Left | Right | Center)
mouse_wheel()                  // scroll lines during the last frame
key_pressed()                  // any key held?
key()                          // most recent character ('\0' if none)
```

## Event handlers

The classic creative-coding event functions. Rust cannot discover magic
global functions like Processing's `mousePressed()`, so handlers are
registered explicitly in `main()` before `run()`:

```rust
fn main() {
    size(800, 600);
    on_mouse_pressed(pressed);
    on_mouse_dragged(dragged);
    on_mouse_wheel(wheel);
    on_key_pressed(keydown);
    run(draw);
}

fn pressed() { /* mouse_x()/mouse_y()/mouse_button() are current */ }
fn dragged() { /* moved with a button held */ }
fn wheel(delta: f32) { /* scroll lines */ }
fn keydown() { if key() == 's' { save_frame("shot-####.png"); } }
```

Available registrations: `on_mouse_pressed`, `on_mouse_released`,
`on_mouse_moved`, `on_mouse_dragged`, `on_mouse_wheel`, `on_key_pressed`,
`on_key_released`. Handlers fire between frames, like Processing.

## Time & frame control

```rust
frame_count()                  // frames since run() (starts at 1)
millis()                       // ms since the sketch started
frame_rate(30.0)               // cap the loop (default: display vsync)
redraw_continuous(false)       // only redraw on input (low-power sketches)
```
