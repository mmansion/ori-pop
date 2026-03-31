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
//!   and scene objects. Toggle visibility with the **Space** key.
//!
//! ## Orbit camera (`Scene3D::orbit_enabled`)
//!
//! Enabled by default on [`Scene3D`]. Sketches that animate [`Camera::eye`] themselves should set
//! [`Scene3D::orbit_enabled`] to `false` each frame. When enabled, viewport navigation matches common
//! 3D apps (Blender-style):
//! - **Right mouse** drag — orbit (tumble) around the look target.
//! - **Middle mouse** drag — pan (slide the look target in the view plane, like many CAD / DCC tools).
//! - **Scroll wheel** — zoom. In **perspective**, dolly by changing eye distance; in **orthographic**,
//!   adjust [`Camera::ortho_half_height`] (distance alone does not change scale). The wheel zooms the
//!   3D view only when the pointer is **not** over an egui panel; over the Inspector, the wheel scrolls UI.
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
pub mod capture;
pub mod inspector;
pub mod mesh;
pub mod scene;
mod renderer;

pub use camera::{Camera, Projection};
pub use mesh::MeshKind;
pub use scene::{
    ObjectId, ObjectTexture, Scene3D, TextureGenParams, Object3D, STIPPLE_CANVAS_SIZE,
};

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowId},
};

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

    // ── Orbit camera ──────────────────────────────────────────────────────────
    /// Accumulated azimuth angle (longitude in XY, radians).
    orbit_az:     f32,
    /// Accumulated elevation angle (latitude from XY plane, radians).
    orbit_el:     f32,
    /// Distance from orbit target.
    orbit_r:      f32,
    /// World-space point the camera orbits around.
    orbit_target: glam::Vec3,
    /// Whether orbit state has been engaged (right-drag or scroll used).
    orbit_on:     bool,
    /// Right mouse button is currently held (orbit drag).
    orbit_rdown:  bool,
    /// Middle mouse button is currently held (pan drag in the view plane).
    orbit_mdown:  bool,
    /// Last known mouse position (logical pixels) for delta computation.
    cur_x:        f32,
    cur_y:        f32,
    /// scene.time at the previous frame — used to compute dt for auto-spin.
    prev_time:    f32,

    /// Screenshot / recording state.
    capture:      capture::CaptureState,
}

impl Runner3D {
    fn new(draw_fn: fn(&mut Scene3D), window_attrs: winit::window::WindowAttributes, msaa: u32) -> Self {
        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());

        // Default orbit position — closer than Camera::default() so sketches
        // that use orbit_enabled start with a tighter, more intimate framing.
        let orbit_r  = 3.0_f32;
        let orbit_el = 0.42_f32;  // ~24° above the XY plane
        let orbit_az = -0.79_f32; // ~45° into the -X/-Y quadrant

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
            orbit_az,
            orbit_el,
            orbit_r,
            orbit_target: glam::Vec3::ZERO,
            orbit_on:     false,
            orbit_rdown:  false,
            orbit_mdown:  false,
            cur_x:        0.0,
            cur_y:        0.0,
            prev_time:    0.0,
            capture:      capture::CaptureState::default(),
        }
    }

    /// Copy eye–target into spherical orbit state (call before first drag / zoom after external camera edits).
    fn sync_orbit_from_camera(&mut self) {
        let eye = self.scene.camera.eye;
        let target = self.scene.camera.target;
        let dir = eye - target;
        let r = dir.length().max(0.01);
        self.orbit_r = r;
        self.orbit_target = target;
        let zn = (dir.z / r).clamp(-1.0, 1.0);
        self.orbit_el = zn.asin();
        self.orbit_az = dir.y.atan2(dir.x);
    }

    /// Scroll-wheel zoom: perspective dollies [`orbit_r`]; orthographic scales [`Camera::ortho_half_height`].
    fn apply_orbit_zoom(&mut self, scroll: f32) {
        if !self.orbit_on {
            self.sync_orbit_from_camera();
        }
        let factor = 1.0 - scroll * 0.12;
        let factor = factor.clamp(0.85, 1.15);
        match self.scene.camera.projection {
            Projection::Perspective => {
                self.orbit_r = (self.orbit_r * factor).clamp(0.05, 500.0);
            }
            Projection::Orthographic => {
                let hh = self.scene.camera.ortho_half_height * factor;
                self.scene.camera.ortho_half_height = hh.clamp(0.02, 500.0);
            }
        }
        self.orbit_on = true;
    }

    /// Pan the orbit target in the camera view plane (screen-space drag).
    fn apply_orbit_pan(&mut self, dx: f32, dy: f32) {
        let eye = self.scene.camera.eye;
        let target = self.orbit_target;
        let forward = (target - eye).normalize();
        if !forward.is_finite() {
            return;
        }

        let mut world_up = self.scene.camera.up;
        if world_up.length_squared() < 1e-12 {
            world_up = glam::Vec3::Z;
        } else {
            world_up = world_up.normalize();
        }

        let mut right = forward.cross(world_up);
        if right.length_squared() < 1e-10 {
            // Gimbal-safe fallback when forward ∥ world_up
            let alt = if forward.z.abs() > 0.95 {
                glam::Vec3::X
            } else {
                glam::Vec3::Z
            };
            right = forward.cross(alt);
        }
        let right = right.normalize();
        let cam_up = right.cross(forward).normalize();

        let scale = match self.scene.camera.projection {
            Projection::Perspective => self.orbit_r * 0.003,
            Projection::Orthographic => self.scene.camera.ortho_half_height * 0.006,
        }
        .max(1e-8);

        // Horizontal: grab-drag (target moves opposite to RMB orbit convention). Vertical: dy>0 (mouse
        // down) moves the view down — target shifts +cam_up here so it matches screen motion.
        self.orbit_target -= right * (dx * scale) - cam_up * (dy * scale);
        self.orbit_on = true;
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
        self.capture.update_surface(phys.width, phys.height, renderer.surface_format);

        self.egui_renderer = Some(egui_renderer);
        self.renderer      = Some(renderer);
        self.egui_winit    = Some(egui_winit);
        self.window        = Some(window);
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _: WindowId,
        event: WindowEvent,
    ) {
        // Global convention: Space toggles GUI overlays even when egui currently has focus.
        if let WindowEvent::KeyboardInput { event: key_event, .. } = &event {
            let pressed = key_event.state == ElementState::Pressed;
            if pressed
                && matches!(
                    key_event.logical_key,
                    winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space)
                )
            {
                self.scene.show_inspector = !self.scene.show_inspector;
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
        }

        // Viewport zoom must run **before** egui: egui-winit often marks [`MouseWheel`] consumed, which
        // would otherwise skip our handler entirely. Zoom when the pointer is on the 3D view:
        // inspector closed → whole window; inspector open → use egui hover (not over a panel).
        if let WindowEvent::MouseWheel { delta, .. } = &event {
            if self.scene.orbit_enabled {
                let on_viewport = !self.scene.show_inspector
                    || !self.egui_ctx.is_pointer_over_area();
                if on_viewport {
                    use winit::event::MouseScrollDelta;
                    let scroll = match delta {
                        MouseScrollDelta::LineDelta(_, y) => *y,
                        MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                    };
                    self.apply_orbit_zoom(scroll);
                }
            }
        }

        // Feed the event to egui-winit; if egui consumes it skip remaining app logic.
        if let (Some(egui_winit), Some(window)) = (self.egui_winit.as_mut(), self.window.as_ref()) {
            let response = egui_winit.on_window_event(window, &event);
            if response.consumed {
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::CursorEntered { .. } => {
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::Resized(sz) => {
                let Some(renderer) = self.renderer.as_mut() else { return };
                renderer.resize(sz.width, sz.height);
                let lw = (sz.width  as f64 / renderer.scale_factor) as f32;
                let lh = (sz.height as f64 / renderer.scale_factor) as f32;
                renderer.update_2d_resolution(lw, lh);
                self.scene.width  = lw;
                self.scene.height = lh;
                self.capture.update_surface(sz.width, sz.height, renderer.surface_format);
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                let (Some(window), Some(renderer)) =
                    (self.window.as_ref(), self.renderer.as_mut()) else { return };
                self.scene.time = self.start.elapsed().as_secs_f32();
                let dt = (self.scene.time - self.prev_time).clamp(0.0, 0.05);
                self.prev_time = self.scene.time;

                // ── Auto-spin ──────────────────────────────────────────────
                // Rotate the orbit azimuth automatically, pausing while the
                // right button is held so manual dragging feels uninterrupted.
                if self.scene.auto_spin && !self.orbit_rdown && !self.orbit_mdown {
                    self.orbit_az -= self.scene.spin_speed * dt;
                    self.orbit_on  = true; // ensure orbit is active
                }

                // ── Run draw callback ──────────────────────────────────────
                oripop_core::draw::begin_frame();
                (self.draw_fn)(&mut self.scene);

                // ── Apply orbit camera override ────────────────────────────
                // Applied AFTER draw_fn so orbit takes precedence over any
                // camera position the sketch may have set.
                // Always applied when orbit_enabled — no longer gated on
                // orbit_on, so the closer default radius shows from frame 1.
                if self.scene.orbit_enabled {
                    let el = self.orbit_el;
                    let az = self.orbit_az;
                    let r  = self.orbit_r;
                    self.scene.camera.eye    = self.orbit_target + r * glam::Vec3::new(
                        el.cos() * az.cos(),
                        el.cos() * az.sin(),
                        el.sin(),
                    );
                    self.scene.camera.target = self.orbit_target;
                }

                let (bg, vertices_2d) = oripop_core::draw::take_2d_vertices();

                // ── Build egui frame ───────────────────────────────────────
                let egui_output = if let Some(egui_winit) = self.egui_winit.as_mut() {
                    let raw_input   = egui_winit.take_egui_input(window);
                    let scene       = &mut self.scene;
                    let capture = &self.capture;
                    let full_output = self.egui_ctx.run(raw_input, |ctx| {
                        if scene.show_inspector {
                            egui::SidePanel::right("inspector")
                                .min_width(220.0)
                                .resizable(true)
                                .show(ctx, |ui| inspector::draw(ui, scene, capture));
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

                // ── Optional capture (screenshot / recording) ──────────────
                // Must happen after all render passes but before present(),
                // while the surface texture is still valid for COPY_SRC.
                if self.capture.screenshot_pending {
                    self.capture.take_screenshot(
                        &renderer.device, &renderer.queue, &output.texture,
                    );
                }
                if self.capture.recording {
                    self.capture.record_frame(
                        &renderer.device, &renderer.queue, &output.texture,
                    );
                }

                // ── Single present for the whole frame ──────────────────────
                output.present();
            }

            WindowEvent::CursorMoved { position, .. } => {
                let sf  = self
                    .renderer
                    .as_ref()
                    .map(|r| r.scale_factor as f32)
                    .unwrap_or(1.0);
                let x   = position.x as f32 / sf;
                let y   = position.y as f32 / sf;
                oripop_core::draw::set_mouse_pos(x, y);

                if self.scene.orbit_enabled {
                    let dx = x - self.cur_x;
                    let dy = y - self.cur_y;
                    // RMB: tumble (spherical orbit). MMB: pan target in the view plane.
                    if self.orbit_rdown {
                        self.orbit_az -= dx * 0.005;
                        self.orbit_el = (self.orbit_el + dy * 0.005)
                            .clamp(-std::f32::consts::FRAC_PI_2 * 0.94,
                                   std::f32::consts::FRAC_PI_2 * 0.94);
                        self.orbit_on = true;
                    }
                    if self.orbit_mdown {
                        self.apply_orbit_pan(dx, dy);
                    }
                }
                self.cur_x = x;
                self.cur_y = y;
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                match button {
                    winit::event::MouseButton::Left => {
                        oripop_core::draw::set_mouse_pressed(pressed);
                    }
                    winit::event::MouseButton::Right => {
                        self.orbit_rdown = pressed;
                        if pressed && self.scene.orbit_enabled {
                            self.sync_orbit_from_camera();
                            self.orbit_on = true;
                        }
                    }
                    winit::event::MouseButton::Middle => {
                        self.orbit_mdown = pressed;
                        if pressed && self.scene.orbit_enabled {
                            self.sync_orbit_from_camera();
                            self.orbit_on = true;
                        }
                    }
                    _ => {}
                }
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::MouseWheel { .. } => {
                // Zoom is applied before egui (see above) so it still runs when egui consumes the wheel.
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                let pressed = key_event.state == ElementState::Pressed;

                // Capture shortcuts — handled before passing to the sketch.
                if pressed {
                    if let winit::keyboard::Key::Character(ref s) = key_event.logical_key {
                        match s.as_str() {
                            "s" | "S" => self.capture.request_screenshot(),
                            "r" | "R" => self.capture.toggle_recording(),
                            _ => {}
                        }
                    }
                }

                let code = if pressed {
                    match &key_event.logical_key {
                        winit::keyboard::Key::Character(s) => s.chars().next().unwrap_or('\0'),
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => ' ',
                        _ => '\0',
                    }
                } else {
                    '\0'
                };
                oripop_core::draw::set_key(pressed, code);
                if !oripop_core::draw::continuous_redraw_enabled() {
                    if let Some(w) = self.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Some(window) = &self.window {
            if oripop_core::draw::continuous_redraw_enabled() {
                window.request_redraw();
            }
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
/// Press **Space** to toggle the live inspector panel.
///
/// With [`Scene3D::orbit_enabled`], use **RMB drag** to orbit, **MMB drag** to pan, and **wheel** to
/// zoom the 3D view (wheel over the Inspector scrolls egui). See the crate-level docs for details.
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
        ObjectTexture,
        Projection,
        Scene3D,
        TextureGenParams,
        STIPPLE_CANVAS_SIZE,
    };
}
