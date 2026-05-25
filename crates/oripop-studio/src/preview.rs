//! Preview state — loaded params, animation clock, play / pause.
//!
//! Actual rendering lives in [`crate::gpu::PreviewGpu`]; this struct only
//! tracks what to draw and when.

use std::fs;
use std::io;

use oripop_canvas::Params;
use oripop_project::{CanvasKind, TextureLibrary};

pub struct EmbeddedPreview {
    pub design_id: String,
    pub params:    Option<Params>,
    pub width:     u32,
    pub height:    u32,
    pub frame:     u64,
    pub playing:   bool,
    pub error:     Option<String>,
}

impl EmbeddedPreview {
    pub fn new() -> Self {
        Self {
            design_id: String::new(),
            params:    None,
            width:     0,
            height:    0,
            frame:     0,
            playing:   false,
            error:     None,
        }
    }

    pub fn is_loaded(&self) -> bool {
        self.params.is_some()
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

    pub fn load(&mut self, library: &TextureLibrary, design_id: &str) {
        self.design_id = design_id.to_string();
        self.frame = 0;
        self.playing = true;
        self.error = None;
        self.params = None;
        match load_preview_params(library, design_id) {
            Ok((params, w, h)) => {
                self.params = Some(params);
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

pub fn load_preview_params(
    library: &TextureLibrary,
    design_id: &str,
) -> io::Result<(Params, u32, u32)> {
    let (dir, design) = library.design(design_id)?;
    let (width, height) = match &design.canvas {
        CanvasKind::Provisional { width, height } => (*width, *height),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "in-app preview v0 supports provisional canvas only",
            ));
        }
    };
    let path = dir.join(&design.params);
    let text = fs::read_to_string(&path)?;
    let mut params: Params = serde_json::from_str(&text).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?;
    params.canvas.width = width as f32;
    params.canvas.height = height as f32;
    Ok((params, width, height))
}
