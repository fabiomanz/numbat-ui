use eframe::egui;
use std::sync::{Arc, Mutex};

use numbat::module_importer::{BuiltinModuleImporter, ChainedImporter, FileSystemImporter};
use numbat::resolver::CodeSource;
use numbat::{Context, InterpreterSettings, NumbatError};
use numbat::markup::Markup;

use crate::format::markup_to_layout_job;

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
        Self {
            context: Self::make_fresh_context(),
            input: String::new(),
            history: Vec::new(),
            cmd_history: Vec::new(),
            cmd_history_idx: 0,
            scroll_to_bottom: false,
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
                    &numbat::FormatOptions::default(),
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
}

impl eframe::App for NumbatApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.cmd_history);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        // Main Central Panel
        egui::CentralPanel::default().show(ctx, |ui| {
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
                        let item_response = ui.vertical(|ui| {
                            ui.set_min_width(ui.available_width());

                            // Show the input
                            ui.allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), 24.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let hovered = ui.rect_contains_pointer(ui.max_rect());
                                    
                                    ui.label(egui::RichText::new(">>> ").color(egui::Color32::WHITE).monospace());
                                    ui.label(egui::RichText::new(&item.input).color(egui::Color32::WHITE).monospace());
                                    
                                    // Show trash icon on hover
                                    if hovered {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.button("🗑").on_hover_text("Delete item").clicked() {
                                                delete_idx = Some(idx);
                                            }
                                        });
                                    }
                                }
                            );
                            
                            // Show the printed output (if any)
                            for printed in &item.output_printed {
                                ui.label(markup_to_layout_job(printed));
                            }
                            
                            // Show result
                            if let Some(res) = &item.output_result {
                                ui.label(markup_to_layout_job(res));
                            }
                            
                            // Show error
                            if let Some(err) = &item.error {
                                ui.label(egui::RichText::new(err).color(egui::Color32::RED).monospace());
                            }
                            
                            ui.add_space(8.0);
                        }).response;

                        // Context menu for the whole item
                        item_response.context_menu(|ui| {
                            if ui.button("🗑 Delete this item").clicked() {
                                delete_idx = Some(idx);
                                ui.close_menu();
                            }
                            if ui.button("🧹 Clear all history").clicked() {
                                clear_all = true;
                                ui.close_menu();
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
                        
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.input)
                                .font(egui::TextStyle::Monospace)
                                .text_color(egui::Color32::WHITE)
                                .desired_width(f32::INFINITY)
                                .margin(egui::vec2(0.0, 0.0))
                                .frame(false)
                        );
                        
                        let mut should_request_focus = false;
                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.submit_input();
                            should_request_focus = true;
                        }
                        
                        // History navigation
                        if response.has_focus() {
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                if self.cmd_history_idx > 0 {
                                    self.cmd_history_idx -= 1;
                                    self.input = self.cmd_history[self.cmd_history_idx].clone();
                                }
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                if self.cmd_history_idx + 1 < self.cmd_history.len() {
                                    self.cmd_history_idx += 1;
                                    self.input = self.cmd_history[self.cmd_history_idx].clone();
                                } else {
                                    self.cmd_history_idx = self.cmd_history.len();
                                    self.input.clear();
                                }
                            }
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
