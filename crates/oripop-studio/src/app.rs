//! Studio shell — project browser, GPU preview, code editor, pop-out viewport.

use std::path::PathBuf;

use eframe::egui;
use oripop_project::Project;

use crate::bake::{self, BakeOptions};
use crate::editor::CodeEditor;
use crate::gpu::PreviewGpu;
use crate::paths::default_project_path;
use crate::preview::EmbeddedPreview;

pub struct StudioApp {
    project_path: PathBuf,
    project:      Option<Project>,
    load_error:   Option<String>,
    selected:     Option<String>,
    status:       Option<String>,
    busy:         bool,
    preview:      EmbeddedPreview,
    editor:       CodeEditor,
    gpu:          PreviewGpu,
    popped_out:   bool,
}

impl StudioApp {
    pub fn new(gpu: PreviewGpu) -> Self {
        let project_path = default_project_path();
        let mut app = Self {
            project_path,
            project: None,
            load_error: None,
            selected: None,
            status: None,
            busy: false,
            preview: EmbeddedPreview::new(),
            editor: CodeEditor::empty(),
            gpu,
            popped_out: false,
        };
        app.reload_project();
        app
    }

    fn reload_project(&mut self) {
        self.load_error = None;
        match Project::load(&self.project_path) {
            Ok(proj) => {
                self.set_status(format!(
                    "Loaded {} ({} textures)",
                    proj.manifest.title,
                    proj.textures.len()
                ));
                if self.selected.is_none() {
                    if let Some(id) = proj.textures.first().map(|t| t.id.clone()) {
                        self.project = Some(proj);
                        self.select_texture(&id);
                        return;
                    }
                }
                self.project = Some(proj);
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
                self.project = None;
            }
        }
    }

    fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some(msg.into());
    }

    fn select_texture(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        let Some(proj) = self.project.clone() else {
            return;
        };
        self.set_status(format!("Compiling {id}…"));
        self.preview.load(&proj, id);
        self.gpu.invalidate_target();
        self.editor = CodeEditor::load(&proj, id);
        if let Some(err) = self.preview.error.clone() {
            self.set_status(format!("Failed: {err}"));
        } else {
            self.set_status(format!("Opened {id}"));
        }
    }

    fn save_editor(&mut self) {
        match self.editor.save() {
            Ok(()) => {
                self.set_status(format!("Saved {}", self.editor.path.display()));
                if let Some(id) = self.selected.clone() {
                    self.set_status(format!("Recompiling {id}…"));
                    let proj = self.project.as_ref().cloned();
                    if let Some(proj) = proj {
                        self.preview.load(&proj, &id);
                        self.gpu.invalidate_target();
                        if let Some(err) = &self.preview.error {
                            self.set_status(format!("Build failed: {err}"));
                        } else {
                            self.set_status(format!("Reloaded {id}"));
                        }
                    }
                }
            }
            Err(e) => self.set_status(format!("Save failed: {e}")),
        }
    }

    fn run_bake(&mut self) {
        let Some(id) = self.selected.clone() else {
            self.set_status("Select a texture first.");
            return;
        };
        let Some(proj) = self.project.clone() else {
            self.set_status("No project loaded.");
            return;
        };
        if self.preview.cartridge.is_none() {
            self.set_status("Preview not ready.");
            return;
        }

        self.busy = true;
        self.set_status(format!("Baking {id}…"));
        let width  = self.preview.width;
        let height = self.preview.height;
        let opts   = BakeOptions {
            time:  self.preview.time(),
            frame: self.preview.frame,
        };
        let cartridge = self.preview.cartridge.as_ref().unwrap();
        let result = bake::bake(&proj, &id, cartridge, width, height, &mut self.gpu, opts);
        self.gpu.invalidate_target();
        match result {
            Ok((png, _)) => self.set_status(format!("Baked {}", png.display())),
            Err(e) => self.set_status(format!("Bake failed: {e}")),
        }
        self.busy = false;
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_editor();
        }
    }

    fn current_texture(&mut self) -> Option<egui::TextureId> {
        let cartridge = self.preview.cartridge.as_ref()?;
        let t = self.preview.time();
        let w = self.preview.width;
        let h = self.preview.height;
        self.gpu.render(cartridge, t, w, h)
    }

    fn draw_preview_body(
        ui: &mut egui::Ui,
        texture: Option<egui::TextureId>,
        canvas_size: [f32; 2],
        error: Option<&str>,
        loaded: bool,
    ) {
        if let Some(err) = error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
            return;
        }
        if !loaded {
            ui.centered_and_justified(|ui| {
                ui.label("Select a texture to preview.");
            });
            return;
        }
        let Some(tex) = texture else {
            ui.centered_and_justified(|ui| ui.label("Rendering…"));
            return;
        };
        let avail = ui.available_size();
        let size = fit_size(avail, canvas_size);
        ui.centered_and_justified(|ui| {
            ui.image((tex, size));
        });
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        self.preview.tick_frame();

        let texture = self.current_texture();
        let canvas_size = [self.preview.width as f32, self.preview.height as f32];

        egui::SidePanel::left("project_browser")
            .resizable(true)
            .default_width(240.0)
            .min_width(180.0)
            .show(ctx, |ui| {
                ui.heading("Textures");
                ui.separator();

                if ui.button("Reload project").clicked() {
                    self.reload_project();
                }

                ui.separator();

                if let Some(err) = &self.load_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                } else if let Some(proj) = &self.project {
                    ui.label(format!(
                        "{} · {}",
                        proj.manifest.title, proj.manifest.engine_version
                    ));
                    ui.separator();
                    let entries: Vec<_> = proj
                        .textures
                        .iter()
                        .map(|t| (t.id.clone(), self.selected.as_deref() == Some(t.id.as_str())))
                        .collect();
                    let mut pick: Option<String> = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (id, selected) in &entries {
                            if ui.selectable_label(*selected, id).clicked() {
                                pick = Some(id.clone());
                            }
                        }
                    });
                    if let Some(id) = pick {
                        self.select_texture(&id);
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    if let Some(status) = &self.status {
                        ui.separator();
                        ui.label(status);
                    }
                    ui.separator();
                    ui.monospace(self.project_path.display().to_string());
                });
            });

        egui::TopBottomPanel::bottom("code_editor")
            .resizable(true)
            .default_height(280.0)
            .min_height(120.0)
            .show(ctx, |ui| {
                if self.editor.draw(ui) {
                    self.save_editor();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Preview");
                if let Some(id) = self.selected.as_deref() {
                    ui.label(format!("— {id}"));
                }
                if self.preview.is_loaded() {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} × {} · frame {}",
                            self.preview.width, self.preview.height, self.preview.frame
                        ))
                        .small()
                        .weak(),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    let has_texture = self.selected.is_some() && self.project.is_some();
                    ui.add_enabled_ui(has_texture && !self.busy, |ui| {
                        if ui.button("Bake PNG").clicked() {
                            self.run_bake();
                        }
                        let play_label = if self.preview.is_playing() {
                            "Pause"
                        } else {
                            "Play"
                        };
                        if ui.button(play_label).clicked() {
                            self.preview.toggle_playing();
                        }
                        let popout_label = if self.popped_out {
                            "Close window"
                        } else {
                            "Pop out"
                        };
                        if ui.button(popout_label).clicked() {
                            self.popped_out = !self.popped_out;
                        }
                    });
                });
            });
            ui.separator();
            Self::draw_preview_body(
                ui,
                texture,
                canvas_size,
                self.preview.error.as_deref(),
                self.preview.is_loaded(),
            );
        });

        if self.popped_out {
            let title = match self.selected.as_deref() {
                Some(id) => format!("Ori Pop — {id}"),
                None => "Ori Pop Preview".to_string(),
            };
            let viewport_id = egui::ViewportId::from_hash_of("preview_popout");
            let builder = egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(canvas_size_with_min(canvas_size, [640.0, 640.0]))
                .with_min_inner_size([320.0, 320.0]);
            ctx.show_viewport_immediate(viewport_id, builder, |vctx, _class| {
                if vctx.input(|i| i.viewport().close_requested()) {
                    self.popped_out = false;
                }
                egui::CentralPanel::default().show(vctx, |ui| {
                    Self::draw_preview_body(
                        ui,
                        texture,
                        canvas_size,
                        self.preview.error.as_deref(),
                        self.preview.is_loaded(),
                    );
                });
            });
        }

        if self.preview.is_playing() {
            ctx.request_repaint();
        }
    }
}

fn fit_size(avail: egui::Vec2, tex: [f32; 2]) -> egui::Vec2 {
    if tex[0] <= 0.0 || tex[1] <= 0.0 {
        return avail;
    }
    let scale = (avail.x / tex[0]).min(avail.y / tex[1]).min(1.0);
    egui::vec2(
        (tex[0] * scale).floor().max(1.0),
        (tex[1] * scale).floor().max(1.0),
    )
}

fn canvas_size_with_min(canvas: [f32; 2], min: [f32; 2]) -> [f32; 2] {
    [canvas[0].max(min[0]), canvas[1].max(min[1])]
}

pub fn run_gui() -> eframe::Result<()> {
    eframe::run_native(
        "Ori Pop Studio",
        crate::window::main_window_options(),
        Box::new(|cc| {
            cc.egui_ctx
                .options_mut(|o| o.tessellation_options.feathering = true);
            let render_state = cc
                .wgpu_render_state
                .as_ref()
                .expect("oripop-studio requires the eframe wgpu backend");
            let gpu = PreviewGpu::new(render_state);
            Ok(Box::new(StudioApp::new(gpu)) as Box<dyn eframe::App>)
        }),
    )
}
