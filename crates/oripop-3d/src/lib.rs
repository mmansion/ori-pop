//! # oripop-3d
//!
//! A 3D rendering layer for ori-pop, built on top of `oripop-core`.
//!
//! ## Coordinate convention
//!
//! Everything lives in **Z-up right-handed** world space — the standard
//! used by CAD tools, robotics (ROS), 3D printing slicers, and fabrication
//! machines.  X = right, Y = depth/forward, Z = up, XY = ground plane.
//!
//! ## What it provides
//!
//! - GPU-driven 3D pipeline via wgpu + WGSL.
//! - Generative textures computed entirely on the GPU (domain-warped FBM,
//!   animated with time).
//! - 3D render pass with depth testing, Lambertian lighting, and rim shading.
//! - Seamless 2D overlay: any `oripop-core` drawing call inside the draw
//!   callback is composited on top of the 3D scene in the same frame.
//! - Live **egui inspector panel** — shows camera, light, texture-gen params,
//!   and scene objects.  Toggle visibility with the **Tab** key.
//!
//! ## Quickstart
//!
//! ```no_run
//! use oripop_3d::prelude::*;
//!
//! fn main() {
//!     size(960, 640);
//!     title("generative 3d");
//!     smooth(4);
//!     run3d(draw);
//! }
//!
//! fn draw(scene: &mut Scene3D) {
//!     background(6, 4, 14);
//!
//!     // Camera orbits the origin.
//!     let t = scene.time;
//!     scene.camera.eye = Vec3::new(4.0 * t.sin(), -4.0 * t.cos(), 3.0);
//!
//!     scene.clear();
//!     scene.add(MeshKind::Sphere, Mat4::IDENTITY);
//!
//!     // 2D HUD drawn on top.
//!     stroke(200, 200, 255);
//!     line(20.0, 20.0, 300.0, 20.0);
//! }
//! ```

pub mod camera;
pub mod mesh;
pub mod scene;
mod renderer;

pub use camera::Camera;
pub use mesh::MeshKind;
pub use scene::{ObjectId, Scene3D, TextureGenParams, Object3D};

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowId},
};

// ── Inspector UI ─────────────────────────────────────────────────────────────

fn build_inspector(ui: &mut egui::Ui, scene: &mut Scene3D) {
    use egui::{DragValue, Grid, Slider};

    // ── Scene stats ────────────────────────────────────────────────────────
    ui.heading("Scene");
    Grid::new("scene_grid").num_columns(2).show(ui, |ui| {
        ui.label("Time");
        ui.label(format!("{:.2} s", scene.time));
        ui.end_row();
        ui.label("Objects");
        ui.label(scene.objects.len().to_string());
        ui.end_row();
    });
    ui.separator();

    // ── Camera ─────────────────────────────────────────────────────────────
    ui.collapsing("Camera", |ui| {
        Grid::new("cam_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Eye X");
            ui.add(DragValue::new(&mut scene.camera.eye.x).speed(0.05));
            ui.end_row();
            ui.label("Eye Y");
            ui.add(DragValue::new(&mut scene.camera.eye.y).speed(0.05));
            ui.end_row();
            ui.label("Eye Z");
            ui.add(DragValue::new(&mut scene.camera.eye.z).speed(0.05));
            ui.end_row();
            ui.label("Target X");
            ui.add(DragValue::new(&mut scene.camera.target.x).speed(0.05));
            ui.end_row();
            ui.label("Target Y");
            ui.add(DragValue::new(&mut scene.camera.target.y).speed(0.05));
            ui.end_row();
            ui.label("Target Z");
            ui.add(DragValue::new(&mut scene.camera.target.z).speed(0.05));
            ui.end_row();
            ui.label("FOV (°)");
            let mut fov_deg = scene.camera.fov_y.to_degrees();
            if ui.add(Slider::new(&mut fov_deg, 10.0..=120.0)).changed() {
                scene.camera.fov_y = fov_deg.to_radians();
            }
            ui.end_row();
        });
    });
    ui.separator();

    // ── Lighting ───────────────────────────────────────────────────────────
    ui.collapsing("Lighting", |ui| {
        Grid::new("light_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Direction X");
            ui.add(DragValue::new(&mut scene.light_dir.x).speed(0.05));
            ui.end_row();
            ui.label("Direction Y");
            ui.add(DragValue::new(&mut scene.light_dir.y).speed(0.05));
            ui.end_row();
            ui.label("Direction Z");
            ui.add(DragValue::new(&mut scene.light_dir.z).speed(0.05));
            ui.end_row();
        });
    });
    ui.separator();

    // ── Texture generation ─────────────────────────────────────────────────
    ui.collapsing("Texture Generation", |ui| {
        Grid::new("tex_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            ui.label("Frequency");
            ui.add(Slider::new(&mut scene.gen.frequency, 0.1_f32..=10.0));
            ui.end_row();
            ui.label("Octaves");
            ui.add(Slider::new(&mut scene.gen.octaves, 1_u32..=8));
            ui.end_row();
            ui.label("Warp Strength");
            ui.add(Slider::new(&mut scene.gen.warp_strength, 0.0_f32..=4.0));
            ui.end_row();
            ui.label("Seed");
            ui.add(DragValue::new(&mut scene.gen.seed).speed(0.01));
            ui.end_row();
        });
    });
    ui.separator();

    // ── Scene objects ──────────────────────────────────────────────────────
    ui.collapsing("Objects", |ui| {
        for obj in &scene.objects {
            let label = obj.label.as_deref().unwrap_or("(unnamed)");
            let kind  = format!("{:?}", obj.mesh);
            ui.label(format!("• {} — {}", label, kind));
        }
        if scene.objects.is_empty() {
            ui.label("No objects added this frame.");
        }
    });
}

// ── run3d ApplicationHandler ──────────────────────────────────────────────────

struct Runner3D {
    draw_fn:          fn(&mut Scene3D),
    window_attrs:     winit::window::WindowAttributes,
    msaa:             u32,

    window:           Option<Arc<Window>>,
    renderer:         Option<renderer::Renderer>,
    egui_renderer:    Option<egui_wgpu::Renderer>,
    scene:            Scene3D,
    start:            std::time::Instant,

    egui_ctx:         egui::Context,
    egui_winit:       Option<egui_winit::State>,
}

impl Runner3D {
    fn new(draw_fn: fn(&mut Scene3D), window_attrs: winit::window::WindowAttributes, msaa: u32) -> Self {
        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());

        Self {
            draw_fn,
            window_attrs,
            msaa,
            window:        None,
            renderer:      None,
            egui_renderer: None,
            scene:         Scene3D::new(0.0, 0.0),
            start:         std::time::Instant::now(),
            egui_ctx,
            egui_winit:    None,
        }
    }
}

impl ApplicationHandler for Runner3D {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(self.window_attrs.clone())
                .expect("create window"),
        );

        let phys = window.inner_size();
        let (w, h, _, _) = oripop_core::draw::settings();

        let renderer = pollster::block_on(renderer::Renderer::init(
            Arc::clone(&window),
            phys.width, phys.height,
            w, h,
            self.msaa,
        ));

        let scale_factor = renderer.scale_factor;

        // egui-wgpu renderer — owned separately to avoid self-borrow issues.
        let egui_renderer = egui_wgpu::Renderer::new(
            &renderer.device,
            renderer.surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        // egui-winit state — initialised after the window exists.
        let egui_winit = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(scale_factor as f32),
            None,
            Some(renderer.device.limits().max_texture_dimension_2d as usize),
        );

        self.scene.width  = (phys.width  as f64 / scale_factor) as f32;
        self.scene.height = (phys.height as f64 / scale_factor) as f32;

        self.egui_renderer = Some(egui_renderer);
        self.renderer      = Some(renderer);
        self.egui_winit    = Some(egui_winit);
        self.window        = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: WindowId,
        event: WindowEvent,
    ) {
        // Feed the event to egui-winit first; if egui consumes it skip app logic.
        if let (Some(egui_winit), Some(window)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
            let response = egui_winit.on_window_event(window, &event);
            if response.consumed {
                return;
            }
        }

        let (Some(window), Some(renderer)) =
            (self.window.as_ref(), self.renderer.as_mut()) else { return };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(sz) => {
                renderer.resize(sz.width, sz.height);
                let lw = (sz.width  as f64 / renderer.scale_factor) as f32;
                let lh = (sz.height as f64 / renderer.scale_factor) as f32;
                renderer.update_2d_resolution(lw, lh);
                self.scene.width  = lw;
                self.scene.height = lh;
                window.request_redraw();
            }

            WindowEvent::RedrawRequested => {
                self.scene.time = self.start.elapsed().as_secs_f32();

                // ── Run draw callback ──────────────────────────────────────
                oripop_core::draw::begin_frame();
                (self.draw_fn)(&mut self.scene);
                let (bg, vertices_2d) = oripop_core::draw::take_2d_vertices();

                // ── Build egui frame ───────────────────────────────────────
                let egui_output = if let Some(egui_winit) = self.egui_winit.as_mut() {
                    let raw_input   = egui_winit.take_egui_input(window);
                    let scene       = &mut self.scene;
                    let full_output = self.egui_ctx.run(raw_input, |ctx| {
                        if scene.show_inspector {
                            egui::SidePanel::right("inspector")
                                .min_width(220.0)
                                .resizable(true)
                                .show(ctx, |ui| {
                                    ui.heading("Inspector");
                                    ui.small("Tab — toggle  │  drag values to edit");
                                    ui.separator();
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        build_inspector(ui, scene);
                                    });
                                });
                        }
                    });
                    egui_winit.handle_platform_output(window, full_output.platform_output.clone());
                    Some(full_output)
                } else {
                    None
                };

                // ── Main render (compute + 3D + 2D overlay) ────────────────
                // render() returns the SurfaceTexture without presenting so we
                // can composite egui on the same texture before the single present.
                let output = match renderer.render(&self.scene, bg, &vertices_2d) {
                    Ok(o)                                => o,
                    Err(wgpu::SurfaceError::Lost)        => { renderer.reconfigure(); return; }
                    Err(wgpu::SurfaceError::Outdated
                      | wgpu::SurfaceError::Timeout)     => { return; }
                    Err(e) => { log::error!("render error: {e}"); return; }
                };

                // ── egui overlay on the same surface texture ────────────────
                if let (Some(full_output), Some(egui_renderer)) =
                    (egui_output, self.egui_renderer.as_mut())
                {
                    let phys       = renderer.phys_size();
                    let ppp        = window.scale_factor() as f32;
                    let paint_jobs = self.egui_ctx.tessellate(
                        full_output.shapes,
                        full_output.pixels_per_point,
                    );
                    let screen = egui_wgpu::ScreenDescriptor {
                        size_in_pixels:   phys,
                        pixels_per_point: ppp,
                    };
                    renderer.render_egui(
                        &output,
                        egui_renderer,
                        paint_jobs,
                        full_output.textures_delta,
                        screen,
                    );
                }

                // ── Single present for the whole frame ──────────────────────
                output.present();
            }

            WindowEvent::CursorMoved { position, .. } => {
                let sf = renderer.scale_factor as f32;
                let x  = position.x as f32 / sf;
                let y  = position.y as f32 / sf;
                let pressed = oripop_core::draw::mouse_pressed();
                oripop_core::draw::set_mouse(x, y, pressed);
            }

            WindowEvent::MouseInput { state, .. } => {
                let pressed = state == ElementState::Pressed;
                let x = oripop_core::draw::mouse_x();
                let y = oripop_core::draw::mouse_y();
                oripop_core::draw::set_mouse(x, y, pressed);
            }

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let pressed = key_event.state == ElementState::Pressed;

                // Toggle inspector on Tab.
                if pressed {
                    if let winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab) =
                        key_event.logical_key
                    {
                        self.scene.show_inspector = !self.scene.show_inspector;
                    }
                }

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

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

// ── run3d ─────────────────────────────────────────────────────────────────────

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
/// Press **Tab** to toggle the live inspector panel.
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

    let window_attrs = Window::default_attributes()
        .with_title(win_title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64));

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = Runner3D::new(draw_fn, window_attrs, msaa);
    event_loop.run_app(&mut app).expect("event loop error");
}

// ── prelude ───────────────────────────────────────────────────────────────────

/// Convenience re-export — import everything needed for a 3D sketch.
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
        ObjectId,
        Scene3D,
        TextureGenParams,
    };
}
