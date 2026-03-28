//! # oripop-3d
//!
//! A 3D rendering layer for ori-pop, built on top of `oripop-core`.
//!
//! Provides a GPU-driven 3D pipeline (wgpu + WGSL) with:
//! - Generative textures computed entirely on the GPU via a compute shader
//!   (domain-warped FBM, animated with time).
//! - A 3D render pass with depth testing, Lambertian lighting, and rim shading.
//! - Seamless 2D overlay: all `oripop-core` drawing calls (`line`, `rect`,
//!   `ellipse`, …) made inside the draw callback are composited on top of the
//!   3D content in the same frame.
//!
//! ## Quickstart
//!
//! ```no_run
//! use oripop_3d::prelude::*;
//!
//! fn main() {
//!     size(900, 700);
//!     title("generative 3d");
//!     smooth(4);
//!     run3d(draw);
//! }
//!
//! fn draw(scene: &mut Scene3D) {
//!     let t = scene.time;
//!
//!     background(8, 5, 18);                      // 3D clear colour
//!     scene.camera.eye = Vec3::new(0.0, 2.0, 5.0);
//!
//!     scene.clear();
//!     scene.add(MeshKind::Sphere, Mat4::from_rotation_y(t * 0.4));
//!
//!     // 2D overlay drawn on top of the 3D scene
//!     stroke(200, 200, 255);
//!     line(20.0, 20.0, 200.0, 20.0);
//! }
//! ```

pub mod camera;
pub mod mesh;
pub mod scene;
mod renderer;

pub use camera::Camera;
pub use mesh::MeshKind;
pub use scene::{Scene3D, TextureGenParams, Object3D};

use std::sync::Arc;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

// ── run3d ────────────────────────────────────────────────────────────────────

/// Open a window and start the combined 2D + 3D draw loop.
///
/// Configure the window with [`size`], [`title`], and [`smooth`] from
/// `oripop_core` **before** calling `run3d`.
///
/// The `draw_fn` callback is called once per frame.  Inside it you can:
/// - Mutate [`Scene3D`] (camera, objects, texture-gen params).
/// - Call any `oripop_core` 2D drawing function; those shapes will be
///   rendered as a depth-free overlay on top of the 3D scene.
///
/// This function blocks until the window is closed.
pub fn run3d(draw_fn: fn(&mut Scene3D)) {
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "user32")]
        extern "system" {
            fn SetProcessDpiAwarenessContext(value: isize) -> i32;
        }
        SetProcessDpiAwarenessContext(-2);
    }

    let (width, height, win_title, msaa) = oripop_core::draw::settings();

    let event_loop = EventLoop::new().expect("create event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&win_title)
            .with_inner_size(LogicalSize::new(width as f64, height as f64))
            .build(&event_loop)
            .expect("create window"),
    );

    let phys = window.inner_size();
    let mut renderer = pollster::block_on(renderer::Renderer::init(
        Arc::clone(&window),
        phys.width, phys.height,
        width, height,
        msaa,
    ));

    let mut scene = Scene3D::new(width as f32, height as f32);
    let     start = std::time::Instant::now();

    event_loop.set_control_flow(ControlFlow::Poll);
    window.request_redraw();

    event_loop
        .run(move |event, target| {
            if let Event::WindowEvent { event, .. } = event {
                match event {
                    WindowEvent::CloseRequested => target.exit(),

                    WindowEvent::Resized(sz) => {
                        renderer.resize(sz.width, sz.height);
                        let lw = (sz.width  as f64 / renderer.scale_factor) as f32;
                        let lh = (sz.height as f64 / renderer.scale_factor) as f32;
                        renderer.update_2d_resolution(lw, lh);
                        scene.width  = lw;
                        scene.height = lh;
                        window.request_redraw();
                    }

                    WindowEvent::RedrawRequested => {
                        scene.time = start.elapsed().as_secs_f32();

                        oripop_core::draw::begin_frame();
                        draw_fn(&mut scene);
                        let (bg, vertices_2d) = oripop_core::draw::take_2d_vertices();

                        match renderer.render(&scene, bg, &vertices_2d) {
                            Ok(())                              => {}
                            Err(wgpu::SurfaceError::Lost)       => {
                                renderer.reconfigure();
                            }
                            Err(wgpu::SurfaceError::Outdated
                              | wgpu::SurfaceError::Timeout)    => {}
                            Err(e) => log::error!("render error: {e}"),
                        }
                        window.request_redraw();
                    }

                    WindowEvent::CursorMoved { position, .. } => {
                        let sf = renderer.scale_factor as f32;
                        oripop_core::draw::set_mouse(
                            position.x as f32 / sf,
                            position.y as f32 / sf,
                            false, // preserve pressed state via MouseInput
                        );
                        // Re-read current pressed state to avoid clearing it
                        let pressed = oripop_core::draw::mouse_pressed();
                        oripop_core::draw::set_mouse(
                            position.x as f32 / sf,
                            position.y as f32 / sf,
                            pressed,
                        );
                    }

                    WindowEvent::MouseInput { state, .. } => {
                        let pressed = state == ElementState::Pressed;
                        let x = oripop_core::draw::mouse_x();
                        let y = oripop_core::draw::mouse_y();
                        oripop_core::draw::set_mouse(x, y, pressed);
                    }

                    WindowEvent::KeyboardInput { event: key_event, .. } => {
                        let pressed = key_event.state == ElementState::Pressed;
                        let code = if pressed {
                            if let winit::keyboard::Key::Character(ref s) = key_event.logical_key {
                                s.chars().next().unwrap_or('\0')
                            } else { '\0' }
                        } else { '\0' };
                        oripop_core::draw::set_key(pressed, code);
                    }

                    _ => {}
                }
            }
        })
        .expect("event loop error");
}

// ── prelude ───────────────────────────────────────────────────────────────────

/// Convenience re-export: import everything needed for a 3D sketch.
///
/// ```ignore
/// use oripop_3d::prelude::*;
/// ```
///
/// Includes the full `oripop_core` 2D drawing API plus all 3D types.
pub mod prelude {
    pub use oripop_core::prelude::*;

    pub use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

    pub use crate::{
        run3d,
        Camera,
        MeshKind,
        Object3D,
        Scene3D,
        TextureGenParams,
    };
}
