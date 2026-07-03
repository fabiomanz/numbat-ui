#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod config;
mod engine;
mod hotkey;
mod platform;
mod session;
mod theme;
mod ui;

use std::sync::Arc;

fn main() -> eframe::Result {
    env_logger::init(); // set RUST_LOG=debug for logs

    // `--hidden` (used by launch-at-login) starts the app in the background:
    // no window, no Dock icon, just the global hotkey.
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");

    // `with_transparent` is required for the quick panel's rounded corners:
    // eframe enables the transparent wgpu backbuffer painter-wide based on
    // the root viewport flag. The main window itself paints fully opaque.
    #[allow(unused_mut)]
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Numbat UI")
        .with_inner_size([900.0, 640.0])
        .with_min_inner_size([420.0, 320.0])
        .with_transparent(true)
        .with_visible(!start_hidden)
        .with_app_id("numbat-ui");

    // Window icon (macOS uses the bundle's .icns instead).
    #[cfg(not(target_os = "macos"))]
    {
        let icon = image::load_from_memory(include_bytes!("icons/icon.png"))
            .expect("Failed to load app icon")
            .into_rgba8();
        let (width, height) = icon.dimensions();
        viewport = viewport.with_icon(egui::IconData {
            rgba: icon.into_raw(),
            width,
            height,
        });
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Numbat UI", // storage key; kept from 2.x so window geometry survives
        options,
        Box::new(|cc: &eframe::CreationContext| {
            install_fonts(&cc.egui_ctx);
            Ok(Box::new(app::NumbatApp::new(cc)))
        }),
    )
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "JetBrainsMono".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "jetbrains_mono.ttf"
        ))),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .insert(0, "JetBrainsMono".to_owned());
    ctx.set_fonts(fonts);
}
