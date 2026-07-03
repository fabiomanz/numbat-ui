use eframe::egui;
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};

use numbat::module_importer::{BuiltinModuleImporter, ChainedImporter, FileSystemImporter};
use numbat::resolver::CodeSource;
use numbat::{Context, InterpreterSettings, NumbatError};
use numbat::markup::Markup;

use crate::format::markup_to_layout_job;

#[cfg(target_os = "macos")]
use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu, MenuId};
#[cfg(target_os = "macos")]
use muda::accelerator::{Accelerator, Code, Modifiers};

const DEFAULT_CONFIG: &str = r#"# Numbat configuration file
# For details, see: https://numbat.dev/docs/cli/customization

[formatting]
# Character used to separate groups of digits (e.g. "_", ",", " ", or "" to disable)
digit-separator = "_"

# Minimum number of digits before separators are applied (default is 6)
digit-grouping-threshold = 6

# Maximum number of significant digits displayed (default is 6)
significant-digits = 6
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FormattingConfig {
    #[serde(default = "default_digit_separator")]
    pub digit_separator: String,
    #[serde(default = "default_digit_grouping_threshold")]
    pub digit_grouping_threshold: usize,
    #[serde(default = "default_significant_digits")]
    pub significant_digits: usize,
}

fn default_digit_separator() -> String {
    "_".to_string()
}

fn default_digit_grouping_threshold() -> usize {
    6
}

fn default_significant_digits() -> usize {
    6
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    #[serde(default)]
    pub formatting: FormattingConfig,
}

impl Default for FormattingConfig {
    fn default() -> Self {
        Self {
            digit_separator: default_digit_separator(),
            digit_grouping_threshold: default_digit_grouping_threshold(),
            significant_digits: default_significant_digits(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            formatting: FormattingConfig::default(),
        }
    }
}

fn get_config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|path| path.join("numbat").join("config.toml"))
}

fn load_or_create_config() -> (String, numbat::FormatOptions) {
    let mut toml_str = DEFAULT_CONFIG.to_string();
    let mut format_opts = numbat::FormatOptions::default();

    if let Some(config_path) = get_config_path() {
        if !config_path.exists() {
            if let Some(parent) = config_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&config_path, DEFAULT_CONFIG);
        } else if let Ok(content) = std::fs::read_to_string(&config_path) {
            toml_str = content;
        }

        if let Ok(app_config) = toml::from_str::<AppConfig>(&toml_str) {
            format_opts = numbat::FormatOptions {
                digit_separator: app_config.formatting.digit_separator.clone(),
                digit_grouping_threshold: app_config.formatting.digit_grouping_threshold,
                significant_digits: app_config.formatting.significant_digits,
                ..Default::default()
            };
        }
    }

    (toml_str, format_opts)
}

fn save_config(toml_str: &str) -> Result<numbat::FormatOptions, String> {
    let app_config: AppConfig = toml::from_str(toml_str)
        .map_err(|e| format!("Failed to parse TOML configuration: {}", e))?;

    if let Some(config_path) = get_config_path() {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create configuration directory: {}", e))?;
        }
        std::fs::write(&config_path, toml_str)
            .map_err(|e| format!("Failed to write configuration file: {}", e))?;
    } else {
        return Err("Could not locate system configuration directory.".to_string());
    }

    Ok(numbat::FormatOptions {
        digit_separator: app_config.formatting.digit_separator,
        digit_grouping_threshold: app_config.formatting.digit_grouping_threshold,
        significant_digits: app_config.formatting.significant_digits,
        ..Default::default()
    })
}

pub struct HistoryItem {
    pub input: String,
    pub output_printed: Vec<Markup>,
    pub output_result: Option<Markup>,
    pub error: Option<String>,
}

pub struct NumbatApp {
    context: Context,
    input: String,
    history: Vec<HistoryItem>,
    cmd_history: Vec<String>,
    cmd_history_idx: usize,
    scroll_to_bottom: bool,
    last_dead_key: Option<String>,
    show_settings: bool,
    settings_text: String,
    settings_error: Option<String>,
    format_options: numbat::FormatOptions,
    #[cfg(target_os = "macos")]
    _mac_menu: Menu,
    #[cfg(target_os = "macos")]
    mac_settings_id: MenuId,
}

impl NumbatApp {
    fn make_fresh_context() -> Context {
        let fs_importer = FileSystemImporter::default();
        let importer = ChainedImporter::new(
            Box::new(fs_importer),
            Box::<BuiltinModuleImporter>::default(),
        );

        let mut context = Context::new(importer);
        // We load prelude and standard currencies, similar to CLI
        let _ = context.interpret("use prelude", CodeSource::Internal);
        let _ = context.interpret("use units::currencies", CodeSource::Internal);
        
        context
    }

    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (settings_text, format_options) = load_or_create_config();

        #[cfg(target_os = "macos")]
        let (mac_menu, mac_settings_id) = {
            let menu = Menu::new();
            let app_menu = Submenu::new("Numbat UI", true);
            
            let settings_item = MenuItem::new(
                "Settings...",
                true,
                Some(Accelerator::new(
                    Some(Modifiers::SUPER),
                    Code::Comma,
                )),
            );
            let settings_id = settings_item.id().clone();

            let _ = app_menu.append_items(&[
                &settings_item,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ]);

            let _ = menu.append(&app_menu);
            let _ = menu.init_for_nsapp();

            (menu, settings_id)
        };

        Self {
            context: Self::make_fresh_context(),
            input: String::new(),
            history: Vec::new(),
            cmd_history: Vec::new(),
            cmd_history_idx: 0,
            scroll_to_bottom: false,
            last_dead_key: None,
            show_settings: false,
            settings_text,
            settings_error: None,
            format_options,
            #[cfg(target_os = "macos")]
            _mac_menu: mac_menu,
            #[cfg(target_os = "macos")]
            mac_settings_id,
        }
    }

    pub fn restore_history(&mut self, history: Vec<String>) {
        for cmd in history {
            self.input = cmd;
            self.submit_input();
        }
        self.input.clear();
    }

    fn submit_input(&mut self) {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return;
        }

        if line == "clear" {
            self.history.clear();
            self.cmd_history.clear();
            self.cmd_history_idx = 0;
            self.input.clear();
            self.scroll_to_bottom = true;
            return;
        }

        self.cmd_history.push(line.clone());
        self.cmd_history_idx = self.cmd_history.len();

        let printed = Arc::new(Mutex::new(Vec::new()));
        let printed_clone = printed.clone();
        
        let mut settings = InterpreterSettings {
            print_fn: Box::new(move |s: &Markup| {
                printed_clone.lock().unwrap().push(s.clone());
            }),
        };

        let result = self.context.interpret_with_settings(&mut settings, &line, CodeSource::Text);

        let mut item = HistoryItem {
            input: line.clone(),
            output_printed: Vec::new(),
            output_result: None,
            error: None,
        };

        match result {
            Ok((statements, interpreter_result)) => {
                let registry = self.context.dimension_registry();
                let result_markup = interpreter_result.to_markup(
                    statements.last(),
                    registry,
                    true,
                    true,
                    &self.format_options,
                );
                
                item.output_printed = printed.lock().unwrap().clone();
                if interpreter_result.is_value() {
                    item.output_result = Some(result_markup);
                } else {
                    // Just side-effects or empty
                     item.output_result = Some(result_markup);
                }
            }
            Err(e) => {
                let error_str = match *e {
                    NumbatError::ResolverError(ref err) => err.to_string(), // Need a better error formatting soon
                    NumbatError::NameResolutionError(ref err) => err.to_string(),
                    NumbatError::TypeCheckError(ref err) => err.to_string(),
                    NumbatError::RuntimeError(ref err) => err.to_string(),
                };
                item.error = Some(error_str);
            }
        }

        self.history.push(item);
        self.input.clear();
        self.scroll_to_bottom = true;
    }

    fn refresh_history_markup(&mut self) {
        let saved_cmd_history = self.cmd_history.clone();
        self.history.clear();
        self.cmd_history.clear();
        self.cmd_history_idx = 0;
        self.context = Self::make_fresh_context();
        self.restore_history(saved_cmd_history);
    }
}

impl eframe::App for NumbatApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        crate::macos_ime::fix_macos_dead_keys(raw_input, &mut self.last_dead_key);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.cmd_history);
    }

    fn ui(&mut self, base_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = base_ui.ctx().clone();

        // Check for macOS native menu events
        #[cfg(target_os = "macos")]
        {
            if let Ok(event) = muda::MenuEvent::receiver().try_recv() {
                if event.id == self.mac_settings_id {
                    let (current_text, _) = load_or_create_config();
                    self.settings_text = current_text;
                    self.settings_error = None;
                    self.show_settings = true;
                }
            }
        }

        // Global shortcut: Ctrl-D to close
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::D)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // Global shortcut: Ctrl-L to clear
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::L)) {
            self.history.clear();
            self.cmd_history.clear();
            self.cmd_history_idx = 0;
            self.input.clear();
            self.scroll_to_bottom = true;
        }

        // Render Settings Window
        if self.show_settings {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings_viewport"),
                egui::ViewportBuilder::default()
                    .with_title("⚙ Numbat Settings")
                    .with_inner_size([550.0, 450.0])
                    .with_resizable(true),
                |ui, _class| {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Numbat Configuration Editor").strong().size(15.0));
                            ui.label(egui::RichText::new("Changes are saved directly to your standard Numbat config.toml file.").weak());
                            ui.add_space(8.0);

                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::TextEdit::multiline(&mut self.settings_text)
                                            .font(egui::TextStyle::Monospace)
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(15)
                                    );
                                });

                            if let Some(ref err) = self.settings_error {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(err).color(egui::Color32::LIGHT_RED).monospace());
                            }

                            ui.add_space(12.0);

                            ui.horizontal(|ui| {
                                if ui.button("💾 Save & Apply").clicked() {
                                    match save_config(&self.settings_text) {
                                        Ok(opts) => {
                                            self.format_options = opts;
                                            self.settings_error = None;
                                            self.show_settings = false;
                                            self.refresh_history_markup();
                                        }
                                        Err(err) => {
                                            self.settings_error = Some(err);
                                        }
                                    }
                                }

                                if ui.button("❓ Help").on_hover_text("Open Numbat customization documentation").clicked() {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab("https://numbat.dev/docs/cli/customization"));
                                }

                                if ui.button("Cancel").clicked() {
                                    self.show_settings = false;
                                }
                            });
                        });

                        // Check if OS close button was clicked
                        if ui.input(|i| i.viewport().close_requested()) {
                            self.show_settings = false;
                        }
                    });
                },
            );
        }

        // Top Panel with menu bar (Windows / Linux only)
        #[cfg(not(target_os = "macos"))]
        egui::Panel::top("top_panel")
            .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(20)))
            .show_inside(base_ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Clear History (Ctrl-L)").clicked() {
                            self.history.clear();
                            self.cmd_history.clear();
                            self.cmd_history_idx = 0;
                            ui.close();
                        }
                        if ui.button("Quit (Ctrl-D)").clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close();
                        }
                    });

                    if ui.button("⚙ Settings").clicked() {
                        let (current_text, _) = load_or_create_config();
                        self.settings_text = current_text;
                        self.settings_error = None;
                        self.show_settings = true;
                        ui.close();
                    }
                });
            });

        // Main Central Panel
        egui::CentralPanel::default().show_inside(base_ui, |ui| {
            // A scroll area for the history
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;

                    // Display the banner
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(format!("  █▄░█ █░█ █▀▄▀█ █▄▄ ▄▀█ ▀█▀    Numbat {}", env!("NUMBAT_VERSION"))).monospace().color(egui::Color32::from_gray(200)));
                        ui.label(egui::RichText::new("  █░▀█ █▄█ █░▀░█ █▄█ █▀█ ░█░    github.com/fabiomanz/numbat-ui").monospace().color(egui::Color32::from_gray(200)));
                        ui.add_space(8.0);
                    });

                    let mut delete_idx = None;
                    let mut clear_all = false;

                    for (idx, item) in self.history.iter().enumerate() {
                        let item_response = ui.push_id(idx, |ui| {
                            ui.vertical(|ui| {
                                ui.set_min_width(ui.available_width());

                                macro_rules! add_ctx_menu {
                                    ($res:expr) => {
                                        $res.context_menu(|ui| {
                                            if ui.button("🗑 Delete this item").clicked() {
                                                delete_idx = Some(idx);
                                                ui.close();
                                            }
                                            if ui.button("🚫 Clear all history").clicked() {
                                                clear_all = true;
                                                ui.close();
                                            }
                                        })
                                    };
                                }

                                // Show the input
                                let input_res = ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), 24.0),
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        let hovered = ui.rect_contains_pointer(ui.max_rect());
                                        
                                        let r1 = ui.label(egui::RichText::new(">>> ").color(egui::Color32::WHITE).monospace());
                                        add_ctx_menu!(r1);
                                        let r2 = ui.label(egui::RichText::new(&item.input).color(egui::Color32::WHITE).monospace());
                                        add_ctx_menu!(r2);
                                        
                                        // Show trash icon on hover
                                        if hovered {
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let btn = ui.button("🗑").on_hover_text("Delete item");
                                                if btn.clicked() {
                                                    delete_idx = Some(idx);
                                                }
                                                add_ctx_menu!(btn);
                                            });
                                        }
                                    }
                                ).response;
                                add_ctx_menu!(input_res);
                                
                                // Show the printed output (if any)
                                for printed in &item.output_printed {
                                    let r = ui.label(markup_to_layout_job(printed));
                                    add_ctx_menu!(r);
                                }
                                
                                // Show result
                                if let Some(res) = &item.output_result {
                                    let r = ui.label(markup_to_layout_job(res));
                                    add_ctx_menu!(r);
                                }
                                
                                // Show error
                                if let Some(err) = &item.error {
                                    let r = ui.label(egui::RichText::new(err).color(egui::Color32::RED).monospace());
                                    add_ctx_menu!(r);
                                }
                                
                                ui.add_space(8.0);
                            }).response
                        }).inner;

                        // Context menu for the whole item
                        item_response.context_menu(|ui| {
                            if ui.button("🗑 Delete this item").clicked() {
                                delete_idx = Some(idx);
                                ui.close();
                            }
                            if ui.button("🚫 Clear all history").clicked() {
                                clear_all = true;
                                ui.close();
                            }
                        });
                    }

                    if clear_all {
                        self.history.clear();
                        self.cmd_history.clear();
                        self.cmd_history_idx = 0;
                    } else if let Some(idx) = delete_idx {
                        self.history.remove(idx);
                    }

                    // Input box at the bottom
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(">>> ").color(egui::Color32::WHITE).monospace());
                        
                        let id = ui.make_persistent_id("numbat_input");
                        let has_focus = ui.ctx().memory(|mem| mem.has_focus(id));
                        
                        // History navigation
                        let mut history_navigated = false;
                        if has_focus {
                            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                                if self.cmd_history_idx > 0 {
                                    self.cmd_history_idx -= 1;
                                    self.input = self.cmd_history[self.cmd_history_idx].clone();
                                    history_navigated = true;
                                }
                            }
                            if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                                if self.cmd_history_idx + 1 < self.cmd_history.len() {
                                    self.cmd_history_idx += 1;
                                    self.input = self.cmd_history[self.cmd_history_idx].clone();
                                    history_navigated = true;
                                } else {
                                    self.cmd_history_idx = self.cmd_history.len();
                                    self.input.clear();
                                    history_navigated = true;
                                }
                            }

                            if history_navigated {
                                let mut state = egui::widgets::text_edit::TextEditState::load(ui.ctx(), id)
                                    .unwrap_or_default();
                                let char_len = self.input.chars().count();
                                let cursor_pos = egui::text::CCursor::new(char_len);
                                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(cursor_pos)));
                                state.store(ui.ctx(), id);
                            }
                        }

                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .id(id)
                                .code_editor()
                                .text_color(egui::Color32::WHITE)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(0.0, 0.0))
                                .frame(egui::Frame::NONE)
                        );
                        
                        let mut should_request_focus = false;
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.submit_input();
                            should_request_focus = true;
                        }

                        // Focus input if we submitted or if nothing else has focus
                        if should_request_focus || !response.has_focus() {
                            response.request_focus();
                        }
                    });
                    
                    if self.scroll_to_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        self.scroll_to_bottom = false;
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_parsing() {
        let parsed = toml::from_str::<AppConfig>(DEFAULT_CONFIG).unwrap();
        assert_eq!(parsed.formatting.digit_separator, "_");
        assert_eq!(parsed.formatting.digit_grouping_threshold, 6);
        assert_eq!(parsed.formatting.significant_digits, 6);
    }

    #[test]
    fn test_valid_custom_config() {
        let toml_str = r#"
            [formatting]
            digit-separator = ","
            digit-grouping-threshold = 3
            significant-digits = 10
        "#;
        let opts = save_config(toml_str).unwrap();
        assert_eq!(opts.digit_separator, ",");
        assert_eq!(opts.digit_grouping_threshold, 3);
        assert_eq!(opts.significant_digits, 10);
    }

    #[test]
    fn test_invalid_toml() {
        let toml_str = r#"
            [formatting]
            digit-separator = 123  # should be a string
        "#;
        let res = save_config(toml_str);
        assert!(res.is_err());
    }

    #[test]
    fn test_missing_fields_fallback() {
        let toml_str = r#"
            [formatting]
            significant-digits = 8
        "#;
        let opts = save_config(toml_str).unwrap();
        assert_eq!(opts.digit_separator, "_"); // fallback
        assert_eq!(opts.digit_grouping_threshold, 6); // fallback
        assert_eq!(opts.significant_digits, 8); // custom
    }
}
