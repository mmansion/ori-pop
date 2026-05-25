//! Native desktop window chrome — standard OS title bar with min / max / close.

use eframe::egui::{self, Vec2};

/// Standard resizable desktop window with native decorations (Windows title bar, etc.).
pub fn desktop_viewport(title: impl Into<String>, inner_size: [f32; 2]) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_app_id("com.oripop.studio")
        .with_title(title)
        .with_inner_size(inner_size)
        .with_min_inner_size(Vec2::new(720.0, 540.0))
        .with_resizable(true)
        .with_decorations(true)
        .with_titlebar_shown(true)
        .with_titlebar_buttons_shown(true)
        .with_close_button(true)
        .with_minimize_button(true)
        .with_maximize_button(true)
        .with_transparent(false)
        .with_fullsize_content_view(false)
        .with_taskbar(true)
}

pub fn main_window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: desktop_viewport("Ori Pop Studio", [1280.0, 800.0]),
        centered: true,
        persist_window: true,
        ..Default::default()
    }
}
