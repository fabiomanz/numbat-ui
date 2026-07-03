//! The settings window: a proper form UI over the app's config file.

use egui::{
    Align, ComboBox, CornerRadius, Frame, Layout, Margin, RichText, Slider, Stroke,
    ViewportBuilder, ViewportId,
};

use crate::app::NumbatApp;
use crate::config::{ThemeChoice, DEFAULT_QUICK_PANEL_HOTKEY};
use crate::hotkey;

const SEPARATOR_CHOICES: [(&str, &str); 5] = [
    ("_", "Underscore  1_000_000"),
    (",", "Comma  1,000,000"),
    (" ", "Thin space  1 000 000"),
    ("'", "Apostrophe  1'000'000"),
    ("", "None  1000000"),
];

impl NumbatApp {
    pub fn open_settings(&mut self) {
        self.settings_draft = self.config.clone();
        self.settings_error = None;
        self.show_settings = true;
    }

    /// Called from the root viewport each frame while settings are open.
    pub fn settings_viewport(&mut self, ctx: &egui::Context) {
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("settings"),
            ViewportBuilder::default()
                .with_title("Numbat Settings")
                .with_inner_size([440.0, 500.0])
                .with_min_inner_size([400.0, 420.0]),
            |ui, _class| {
                if ui.input(|i| i.viewport().close_requested()) {
                    self.show_settings = false;
                    return;
                }
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    self.settings_ui(ui);
                });
            },
        );
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;

        let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
            ui.label(
                RichText::new(title)
                    .size(11.5)
                    .strong()
                    .color(palette.text_dim),
            );
            ui.add_space(2.0);
            Frame::new()
                .fill(palette.card)
                .stroke(Stroke::new(1.0, palette.border))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    body(ui);
                });
            ui.add_space(12.0);
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(4.0);

            section(ui, "APPEARANCE", &mut |ui| {
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for choice in [ThemeChoice::Light, ThemeChoice::Dark, ThemeChoice::System]
                        {
                            ui.selectable_value(
                                &mut self.settings_draft.ui.theme,
                                choice,
                                choice.label(),
                            );
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Font size");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            Slider::new(&mut self.settings_draft.ui.font_size, 11.0..=20.0)
                                .step_by(0.5)
                                .fixed_decimals(1),
                        );
                    });
                });
            });

            section(ui, "NUMBER FORMATTING", &mut |ui| {
                let formatting = &mut self.settings_draft.formatting;
                ui.horizontal(|ui| {
                    ui.label("Digit separator");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let current = SEPARATOR_CHOICES
                            .iter()
                            .find(|(value, _)| *value == formatting.digit_separator)
                            .map(|(_, label)| *label)
                            .unwrap_or("Custom");
                        ComboBox::from_id_salt("digit_separator")
                            .selected_text(current)
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                for (value, label) in SEPARATOR_CHOICES {
                                    ui.selectable_value(
                                        &mut formatting.digit_separator,
                                        value.to_owned(),
                                        label,
                                    );
                                }
                            });
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Group digits from");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            Slider::new(&mut formatting.digit_grouping_threshold, 3..=12)
                                .suffix(" digits"),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.label("Significant digits");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(Slider::new(&mut formatting.significant_digits, 3..=15));
                    });
                });
            });

            section(ui, "QUICK PANEL", &mut |ui| {
                ui.horizontal(|ui| {
                    ui.label("Global hotkey");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(
                            egui::TextEdit::singleline(
                                &mut self.settings_draft.ui.quick_panel_hotkey,
                            )
                            .desired_width(180.0)
                            .font(egui::TextStyle::Monospace),
                        );
                    });
                });
                match hotkey::parse_combo(&self.settings_draft.ui.quick_panel_hotkey) {
                    Ok(_) => {
                        ui.label(
                            RichText::new(format!(
                                "Shows as:  {}",
                                hotkey::display_combo(
                                    &self.settings_draft.ui.quick_panel_hotkey
                                )
                            ))
                            .size(11.0)
                            .color(palette.text_dim),
                        );
                    }
                    Err(_) => {
                        ui.label(
                            RichText::new("Not a valid combo — e.g. Alt+Space, Cmd+Shift+KeyK")
                                .size(11.0)
                                .color(palette.error),
                        );
                    }
                }
                ui.label(
                    RichText::new(format!(
                        "Summons the quick calculator from anywhere. Default: {DEFAULT_QUICK_PANEL_HOTKEY}"
                    ))
                    .size(11.0)
                    .color(palette.text_faint),
                );
                if let Some(error) = &self.hotkey_error {
                    ui.label(RichText::new(error).size(11.0).color(palette.error));
                }

                ui.add_space(6.0);
                ui.checkbox(
                    &mut self.settings_draft.ui.launch_at_login,
                    "Launch at login",
                );
                ui.label(
                    RichText::new("Starts hidden in the background, so the hotkey works right after login.")
                        .size(11.0)
                        .color(palette.text_faint),
                );
            });

            if let Some(error) = &self.settings_error {
                ui.label(RichText::new(error).color(palette.error).size(12.0));
                ui.add_space(6.0);
            }

            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                let save = egui::Button::new(
                    RichText::new("Save & Apply").color(egui::Color32::WHITE),
                )
                .fill(palette.accent);
                if ui.add(save).clicked() {
                    self.apply_settings(ui.ctx());
                }
                if ui.button("Cancel").clicked() {
                    self.show_settings = false;
                }
            });

            if let Some(path) = crate::config::AppConfig::config_path() {
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(
                        RichText::new(format!("Stored in {}", path.display()))
                            .size(10.5)
                            .color(palette.text_faint),
                    );
                });
            }
        });
    }

    fn apply_settings(&mut self, ctx: &egui::Context) {
        // Validate the hotkey before touching anything.
        if let Err(e) = hotkey::parse_combo(&self.settings_draft.ui.quick_panel_hotkey) {
            self.settings_error = Some(e);
            return;
        }

        // Register/unregister launch-at-login before persisting the config.
        if self.settings_draft.ui.launch_at_login != self.config.ui.launch_at_login {
            if let Err(e) = crate::autostart::set_enabled(self.settings_draft.ui.launch_at_login) {
                self.settings_error = Some(e);
                return;
            }
        }

        if let Err(e) = self.settings_draft.save() {
            self.settings_error = Some(e);
            return;
        }

        let formatting_changed = self.config.formatting != self.settings_draft.formatting;
        let hotkey_changed =
            self.config.ui.quick_panel_hotkey != self.settings_draft.ui.quick_panel_hotkey;
        self.config = self.settings_draft.clone();

        // Re-register the global hotkey.
        if hotkey_changed {
            if let Some(hotkey) = &mut self.hotkey {
                self.hotkey_error = hotkey.register(&self.config.ui.quick_panel_hotkey).err();
            }
        }

        // Reformat existing results with the new options.
        if formatting_changed {
            self.session.engine.format_options = self.config.format_options();
            self.refresh_history();
        }

        // Theme/font changes are picked up by the per-frame sync in app.rs.
        self.applied_palette = None;

        self.settings_error = None;
        self.show_settings = false;
        let _ = ctx;
    }
}
