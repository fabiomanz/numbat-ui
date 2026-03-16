mod app;
mod format;

fn main() -> eframe::Result {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Numbat UI"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Numbat UI",
        options,
        Box::new(|cc: &eframe::CreationContext| {
            let mut fonts = egui::FontDefinitions::default();
            
            // Install JetBrains Mono Nerd Font
            fonts.font_data.insert(
                "JetBrainsMono".to_owned(),
                egui::FontData::from_static(include_bytes!(
                    "jetbrains_mono.ttf"
                )),
            );
            
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "JetBrainsMono".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            
            let mut style = (*cc.egui_ctx.style()).clone();
            style.text_styles.insert(
                egui::TextStyle::Monospace,
                egui::FontId::new(14.0, egui::FontFamily::Monospace),
            );
            style.text_styles.insert(
                egui::TextStyle::Body,
                egui::FontId::new(14.0, egui::FontFamily::Proportional),
            );
            cc.egui_ctx.set_style(style);

            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = egui::Color32::BLACK;
            visuals.window_fill = egui::Color32::BLACK;
            cc.egui_ctx.set_visuals(visuals);

            Ok(Box::new(app::NumbatApp::new(cc)))
        }),
    )
}
