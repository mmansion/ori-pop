//! Texture source editor (`src/lib.rs`).

use std::fs;
use std::io;
use std::path::PathBuf;

use eframe::egui;
use oripop_project::Project;

pub struct CodeEditor {
    pub path:   PathBuf,
    pub source: String,
    saved:      String,
    pub error:  Option<String>,
}

impl CodeEditor {
    pub fn empty() -> Self {
        Self {
            path:   PathBuf::new(),
            source: String::new(),
            saved:  String::new(),
            error:  None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.path.as_os_str().is_empty() && self.source != self.saved
    }

    pub fn load(project: &Project, texture_id: &str) -> Self {
        match project.texture(texture_id) {
            Ok((dir, _manifest)) => {
                let path = dir.join("src").join("lib.rs");
                match fs::read_to_string(&path) {
                    Ok(source) => Self {
                        path,
                        saved: source.clone(),
                        source,
                        error: None,
                    },
                    Err(e) => Self {
                        path,
                        source: String::new(),
                        saved:  String::new(),
                        error:  Some(e.to_string()),
                    },
                }
            }
            Err(e) => Self {
                path:   PathBuf::new(),
                source: String::new(),
                saved:  String::new(),
                error:  Some(e.to_string()),
            },
        }
    }

    pub fn save(&mut self) -> io::Result<()> {
        if self.path.as_os_str().is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no file loaded"));
        }
        fs::write(&self.path, &self.source)?;
        self.saved = self.source.clone();
        Ok(())
    }

    pub fn draw(&mut self, ui: &mut egui::Ui) -> bool {
        let mut saved = false;

        ui.horizontal(|ui| {
            ui.strong("Source");
            if !self.path.as_os_str().is_empty() {
                ui.monospace(self.path.display().to_string());
            }
            if self.is_dirty() {
                ui.colored_label(egui::Color32::YELLOW, "modified");
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(self.is_dirty(), egui::Button::new("Save"))
                    .clicked()
                {
                    saved = true;
                }
            });
        });

        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, err);
        }

        ui.separator();

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
            });

        saved
    }
}
