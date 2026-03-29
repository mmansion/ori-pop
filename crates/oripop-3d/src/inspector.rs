//! egui inspector panel — live parameter editing for the 3D scene.
//!
//! Called once per frame inside the egui run closure when `scene.show_inspector`
//! is true.  Everything in this module is pure UI logic: it reads and writes
//! `Scene3D` fields through normal mutable references with no GPU involvement.

use egui::{DragValue, Grid, ScrollArea, Slider, Ui};
use crate::scene::Scene3D;

/// Build the inspector panel contents into `ui`.
///
/// Call this inside an `egui::SidePanel` or `egui::Window` show closure.
pub fn draw(ui: &mut Ui, scene: &mut Scene3D) {
    ui.heading("Inspector");
    ui.small("Tab — toggle  │  drag values to edit");
    ui.separator();

    ScrollArea::vertical().show(ui, |ui| {
        scene_stats(ui, scene);
        ui.separator();
        camera_section(ui, scene);
        ui.separator();
        lighting_section(ui, scene);
        ui.separator();
        texture_gen_section(ui, scene);
        ui.separator();
        objects_section(ui, scene);
    });
}

// ── Sections ──────────────────────────────────────────────────────────────────

fn scene_stats(ui: &mut Ui, scene: &Scene3D) {
    ui.strong("Scene");
    Grid::new("scene_grid").num_columns(2).show(ui, |ui| {
        ui.label("Time");
        ui.label(format!("{:.2} s", scene.time));
        ui.end_row();
        ui.label("Objects");
        ui.label(scene.objects.len().to_string());
        ui.end_row();
    });
}

fn camera_section(ui: &mut Ui, scene: &mut Scene3D) {
    ui.collapsing("Camera", |ui| {
        Grid::new("cam_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            vec3_row(ui, "Eye X",    &mut scene.camera.eye.x);
            vec3_row(ui, "Eye Y",    &mut scene.camera.eye.y);
            vec3_row(ui, "Eye Z",    &mut scene.camera.eye.z);
            vec3_row(ui, "Target X", &mut scene.camera.target.x);
            vec3_row(ui, "Target Y", &mut scene.camera.target.y);
            vec3_row(ui, "Target Z", &mut scene.camera.target.z);

            ui.label("FOV (°)");
            let mut fov_deg = scene.camera.fov_y.to_degrees();
            if ui.add(Slider::new(&mut fov_deg, 10.0..=120.0)).changed() {
                scene.camera.fov_y = fov_deg.to_radians();
            }
            ui.end_row();
        });
    });
}

fn lighting_section(ui: &mut Ui, scene: &mut Scene3D) {
    ui.collapsing("Lighting", |ui| {
        Grid::new("light_grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
            vec3_row(ui, "Direction X", &mut scene.light_dir.x);
            vec3_row(ui, "Direction Y", &mut scene.light_dir.y);
            vec3_row(ui, "Direction Z", &mut scene.light_dir.z);
        });
    });
}

fn texture_gen_section(ui: &mut Ui, scene: &mut Scene3D) {
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
}

fn objects_section(ui: &mut Ui, scene: &Scene3D) {
    ui.collapsing("Objects", |ui| {
        if scene.objects.is_empty() {
            ui.label("No objects this frame.");
        } else {
            for obj in &scene.objects {
                let label = obj.label.as_deref().unwrap_or("(unnamed)");
                ui.label(format!("• {} — {:?}", label, obj.mesh));
            }
        }
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// One label + drag-value row inside a two-column Grid.
fn vec3_row(ui: &mut Ui, label: &str, value: &mut f32) {
    ui.label(label);
    ui.add(DragValue::new(value).speed(0.05));
    ui.end_row();
}
