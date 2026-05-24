//! egui studio shell — library browser and Play / Bake controls.

use std::path::PathBuf;

use eframe::egui;
use oripop_project::TextureLibrary;

use crate::bake::{self, BakeOptions};
use crate::paths::{default_library_path, engine_root};
use crate::play;

pub struct StudioApp {
    library_path: PathBuf,
    library:      Option<TextureLibrary>,
    load_error:   Option<String>,
    selected:     Option<String>,
    log:          Vec<String>,
    busy:         bool,
}

impl StudioApp {
    pub fn new() -> Self {
        let library_path = default_library_path();
        let mut app = Self {
            library_path,
            library: None,
            load_error: None,
            selected: None,
            log: Vec::new(),
            busy: false,
        };
        app.reload_library();
        app
    }

    fn reload_library(&mut self) {
        self.load_error = None;
        match TextureLibrary::load(&self.library_path) {
            Ok(lib) => {
                self.push_log(format!(
                    "Loaded {} ({} designs)",
                    lib.manifest.title,
                    lib.designs().len()
                ));
                if self.selected.is_none() {
                    self.selected = lib.designs().first().map(|d| d.id.clone());
                }
                self.library = Some(lib);
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
                self.library = None;
            }
        }
    }

    fn push_log(&mut self, msg: impl Into<String>) {
        self.log.push(msg.into());
        if self.log.len() > 64 {
            self.log.remove(0);
        }
    }

    fn selected_id(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    fn run_play(&mut self) {
        let Some(id) = self.selected.clone() else {
            self.push_log("Select a design first.");
            return;
        };
        if self.library.is_none() {
            self.push_log("No library loaded.");
            return;
        }

        self.busy = true;
        self.push_log(format!("Building and launching {id}…"));
        let path = self.library_path.clone();
        let result = (|| -> Result<(), String> {
            let library = TextureLibrary::load(&path).map_err(|e| e.to_string())?;
            let engine = engine_root().map_err(|e| e.to_string())?;
            play::spawn_play(&library, &id, &engine).map_err(|e| e.to_string())?;
            Ok(())
        })();
        match result {
            Ok(()) => self.push_log(format!("Playing {id} (preview window opened).")),
            Err(e) => self.push_log(format!("Play failed: {e}")),
        }
        self.busy = false;
    }

    fn run_bake(&mut self) {
        let Some(id) = self.selected.clone() else {
            self.push_log("Select a design first.");
            return;
        };
        if self.library.is_none() {
            self.push_log("No library loaded.");
            return;
        }

        self.busy = true;
        self.push_log(format!("Baking {id}…"));
        let path = self.library_path.clone();
        let result = (|| -> Result<(PathBuf, PathBuf), String> {
            let library = TextureLibrary::load(&path).map_err(|e| e.to_string())?;
            bake::bake(&library, &id, BakeOptions::default()).map_err(|e| e.to_string())
        })();
        match result {
            Ok((png, manifest)) => {
                self.push_log(format!("Baked {}", png.display()));
                self.push_log(format!("Manifest {}", manifest.display()));
            }
            Err(e) => self.push_log(format!("Bake failed: {e}")),
        }
        self.busy = false;
    }
}

impl eframe::App for StudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("library_browser")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Texture Library");
                ui.separator();

                ui.label("Library path:");
                ui.monospace(&self.library_path.display().to_string());

                if ui.button("Reload").clicked() {
                    self.reload_library();
                }

                ui.separator();

                if let Some(err) = &self.load_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, err);
                } else if let Some(lib) = &self.library {
                    ui.label(format!(
                        "{} · engine {}",
                        lib.manifest.title, lib.manifest.engine_version
                    ));
                    ui.separator();
                    ui.label("Designs");
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for entry in lib.designs() {
                            let selected = self.selected.as_deref() == Some(entry.id.as_str());
                            if ui.selectable_label(selected, &entry.id).clicked() {
                                self.selected = Some(entry.id.clone());
                            }
                        }
                    });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Ori Pop Studio");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.separator();

            ui.horizontal(|ui| {
                let can_run = !self.busy && self.selected_id().is_some() && self.library.is_some();
                ui.add_enabled_ui(can_run, |ui| {
                    if ui.button("▶  Play").clicked() {
                        self.run_play();
                    }
                    if ui.button("Bake PNG").clicked() {
                        self.run_bake();
                    }
                });
                if self.busy {
                    ui.spinner();
                }
            });

            if let Some(id) = self.selected_id() {
                ui.separator();
                ui.label(format!("Selected: {id}"));
                if let Some(lib) = self.library.as_ref() {
                    if let Ok((dir, manifest)) = lib.design(id) {
                        ui.label(format!("Title: {}", manifest.title));
                        ui.monospace(dir.display().to_string());
                    }
                }
            }

            ui.separator();
            ui.label("Log");
            egui::ScrollArea::vertical()
                .max_height(220.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.monospace(line);
                    }
                });
        });
    }
}

pub fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_title("Ori Pop Studio"),
        ..Default::default()
    };
    eframe::run_native(
        "Ori Pop Studio",
        options,
        Box::new(|_cc| Ok(Box::new(StudioApp::new()) as Box<dyn eframe::App>)),
    )
}
