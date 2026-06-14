//! Preview state — loaded cartridge, animation clock, play / pause.
//!
//! Actual rendering lives in [`oripop_3d::SketchViewport`]; this struct only
//! tracks what to draw and when.

use std::io;
use std::path::PathBuf;

use oripop_project::{CanvasKind, Project};

use crate::cartridge::Cartridge;
use crate::paths::engine_root;

pub struct EmbeddedPreview {
    pub texture_id: String,
    pub cartridge:  Option<Cartridge>,
    pub width:      u32,
    pub height:     u32,
    pub frame:      u64,
    pub playing:    bool,
    pub error:      Option<String>,
}

impl EmbeddedPreview {
    pub fn new() -> Self {
        Self {
            texture_id: String::new(),
            cartridge:  None,
            width:      0,
            height:     0,
            frame:      0,
            playing:    false,
            error:      None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.cartridge.is_some()
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn toggle_playing(&mut self) {
        self.playing = !self.playing;
    }

    pub fn time(&self) -> f32 {
        self.frame as f32 / 60.0
    }

    pub fn tick_frame(&mut self) {
        if self.playing {
            self.frame = self.frame.wrapping_add(1);
        }
    }

    pub fn load(&mut self, project: &Project, texture_id: &str) {
        self.texture_id = texture_id.to_string();
        self.frame = 0;
        self.playing = true;
        self.error = None;
        self.cartridge = None;
        match load_cartridge(project, texture_id) {
            Ok((cartridge, w, h)) => {
                self.cartridge = Some(cartridge);
                self.width = w;
                self.height = h;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.width = 0;
                self.height = 0;
            }
        }
    }

}

pub fn load_cartridge(
    project: &Project,
    texture_id: &str,
) -> io::Result<(Cartridge, u32, u32)> {
    let (dir, manifest) = project.texture(texture_id)?;
    let (width, height) = match &manifest.canvas {
        CanvasKind::Provisional { width, height } => (*width, *height),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "in-app preview v0 supports provisional canvas only",
            ));
        }
    };
    let params_path: PathBuf = dir.join(&manifest.params);
    let workspace = engine_root()?;
    let cartridge = Cartridge::build_and_load(&workspace, texture_id, params_path)?;
    Ok((cartridge, width, height))
}
