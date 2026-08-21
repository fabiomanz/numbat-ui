//! The full calculator window.

use egui::{Align, Frame, Layout, Margin, RichText, ScrollArea, Stroke};

use crate::app::NumbatApp;
use crate::theme::markup_job;
use crate::ui::{entry_card, EntryAction, InputField};

/// Maximum width of the content column, for readability on wide windows.
const CONTENT_MAX_WIDTH: f32 = 860.0;

impl NumbatApp {
    pub fn main_window_ui(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;

        self.input_panel(ui);

        // History fills the remaining space.
        egui::CentralPanel::default()
            .frame(Frame::new().fill(palette.bg))
            .show_inside(ui, |ui| {
                self.history_view(ui);
            });

        self.toasts.ui(ui.ctx(), &palette, "main");
    }

    fn history_view(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        let font_size = self.config.ui.font_size;

        let mut delete_index = None;
        let mut clear_all = false;
        let mut reuse: Option<String> = None;
        let mut copy: Option<String> = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                // Center the content column.
                let inner_width = ui.available_width().min(CONTENT_MAX_WIDTH);
                let side = ((ui.available_width() - inner_width) / 2.0).max(12.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_max_width(inner_width - 12.0);
                        ui.spacing_mut().item_spacing.y = 10.0;
                        ui.add_space(16.0);

                        if self.session.entries.is_empty() {
                            self.empty_state(ui);
                        }

                        for (index, entry) in self.session.entries.iter().enumerate() {
                            ui.push_id(index, |ui| {
                                match entry_card(ui, entry, &palette, font_size) {
                                    EntryAction::None => {}
                                    EntryAction::Delete => delete_index = Some(index),
                                    EntryAction::ClearAll => clear_all = true,
                                    EntryAction::Reuse(text) => reuse = Some(text),
                                    EntryAction::CopyResult(text) => copy = Some(text),
                                }
                            });
                        }
                        ui.add_space(8.0);
                    });
                });

                if self.session.scroll_to_bottom {
                    ui.scroll_to_cursor(Some(Align::BOTTOM));
                    self.session.scroll_to_bottom = false;
                }
            });

        if clear_all {
            self.session.clear();
        } else if let Some(index) = delete_index {
            self.session.delete_entry(index);
        }
        if let Some(text) = reuse {
            self.session.input = text;
            self.session.on_input_edited();
            ui.ctx()
                .memory_mut(|m| m.request_focus(self.main_input_id()));
        }
        if let Some(text) = copy {
            self.copy_to_clipboard(ui.ctx(), text);
        }
    }

    fn empty_state(&self, ui: &mut egui::Ui) {
        let palette = self.palette;
        ui.add_space(ui.available_height() * 0.22);
        ui.vertical_centered(|ui| {
            if let Some(logo) = &self.logo {
                ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(96.0, 96.0)));
                ui.add_space(12.0);
            }
            ui.label(RichText::new("Numbat UI").size(26.0).strong());
            ui.label(
                RichText::new("The scientific calculator with physical units")
                    .size(13.5)
                    .color(palette.text_dim),
            );
            ui.add_space(14.0);
            ui.label(
                RichText::new(concat!("Powered by Numbat v", env!("NUMBAT_VERSION")))
                    .size(11.5)
                    .color(palette.text_faint),
            );
        });
    }

    fn input_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        let font_size = self.config.ui.font_size;

        egui::Panel::bottom("input_panel")
            .frame(
                Frame::new()
                    .fill(palette.bg_raised)
                    .stroke(Stroke::new(1.0, palette.border))
                    .inner_margin(Margin::symmetric(18, 14)),
            )
            .show_separator_line(false)
            .show_inside(ui, |ui| {
                let inner_width = ui.available_width().min(CONTENT_MAX_WIDTH);
                let side = ((ui.available_width() - inner_width) / 2.0).max(0.0);
                ui.horizontal_top(|ui| {
                    ui.add_space(side);
                    ui.vertical(|ui| {
                        ui.set_max_width(inner_width);

                        // Live preview of the value while typing. The row has
                        // a fixed height so the bar never jumps while typing.
                        let preview = self.session.preview();
                        let row_height = font_size + 10.0;
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), row_height),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                // egui shrinks this to the space actually
                                // used; claim the full row even when empty so
                                // the bar height never changes.
                                ui.set_min_height(row_height);
                                let Some(preview) = preview else { return };
                                // Dim the value while it belongs to an older
                                // (still valid) version of the input.
                                if !preview.fresh {
                                    ui.set_opacity(0.45);
                                }
                                ui.label(RichText::new("=").monospace().color(palette.text_faint));
                                let job = markup_job(&preview.markup, &palette, font_size + 1.0);
                                let label = ui
                                    .add(
                                        egui::Label::new(job)
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::Copy)
                                    .on_hover_text("Click to copy");
                                if label.clicked() {
                                    self.copy_to_clipboard(ui.ctx(), preview.plain);
                                }
                            },
                        );
                        ui.add_space(4.0);

                        let input_id = self.main_input_id();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("❯")
                                    .monospace()
                                    .size(font_size + 4.0)
                                    .color(palette.accent)
                                    .strong(),
                            );

                            // Action buttons live at the right end of the row.
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;

                                let icon_button = |text: &str| {
                                    egui::Button::new(
                                        RichText::new(text).size(18.0).color(palette.text_dim),
                                    )
                                    .frame(false)
                                };

                                if ui.add(icon_button("⚙")).on_hover_text("Settings").clicked() {
                                    self.open_settings();
                                }
                                if ui
                                    .add(icon_button("🗑"))
                                    .on_hover_text("Clear history")
                                    .clicked()
                                {
                                    self.session.clear();
                                }

                                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                    let result = InputField {
                                        session: &mut self.session,
                                        completion: &mut self.completion,
                                        palette: &palette,
                                        font_size: font_size + 2.0,
                                        hint: "Calculate…",
                                        id: input_id,
                                    }
                                    .show(ui);

                                    if result.submitted {
                                        self.session.submit();
                                        result.response.request_focus();
                                    }

                                    // Keep the prompt focused, terminal-style,
                                    // unless something else grabbed focus.
                                    if ui.ctx().memory(|m| m.focused().is_none()) {
                                        result.response.request_focus();
                                    }
                                });
                            });
                        });
                    });
                });
            });
    }

    pub fn main_input_id(&self) -> egui::Id {
        egui::Id::new("main_input")
    }

    pub fn copy_to_clipboard(&mut self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text);
        self.toasts.push(ctx, "Copied to clipboard");
    }
}
