//! UI building blocks shared by the main window and the quick panel.

pub mod main_window;
pub mod quick_panel;
pub mod settings;

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Color32, CornerRadius, FontFamily, FontId, Frame, Margin, RichText, Stroke};

use crate::session::{HistoryEntry, Session};
use crate::theme::{highlight_input, markup_job, Palette};

// ---------------------------------------------------------------------------
// Toasts

/// Small transient notifications ("Copied to clipboard").
#[derive(Default)]
pub struct Toasts {
    items: Vec<(String, f64)>,
}

impl Toasts {
    pub fn push(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let now = ctx.input(|i| i.time);
        self.items.push((text.into(), now + 1.6));
    }

    /// Draws the toasts near the bottom of the current viewport.
    pub fn ui(&mut self, ctx: &egui::Context, palette: &Palette, id_salt: &str) {
        let now = ctx.input(|i| i.time);
        self.items.retain(|(_, expires)| *expires > now);
        if self.items.is_empty() {
            return;
        }
        ctx.request_repaint(); // keep animating until they expire

        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new(("toasts", id_salt)))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.center().x, screen.bottom() - 64.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ctx, |ui| {
                for (text, _) in &self.items {
                    Frame::new()
                        .fill(if palette.dark {
                            Color32::from_rgb(0x2c, 0x31, 0x3d)
                        } else {
                            Color32::from_rgb(0x33, 0x37, 0x41)
                        })
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(12, 7))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 3],
                            blur: 12,
                            spread: 0,
                            color: Color32::from_black_alpha(90),
                        })
                        .show(ui, |ui| {
                            ui.label(RichText::new(text).color(Color32::WHITE).size(13.0));
                        });
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Tab completion

#[derive(Default)]
pub struct CompletionState {
    items: Vec<String>,
    /// Index of the item currently cycled to with Tab.
    index: Option<usize>,
    /// Byte offset where the completed word starts.
    word_start: usize,
    /// Byte length of the text currently inserted at `word_start`.
    applied_len: usize,
    open: bool,
}

impl CompletionState {
    pub fn close(&mut self) {
        self.open = false;
        self.items.clear();
        self.index = None;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '°'
}

fn longest_common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else {
        return String::new();
    };
    let mut prefix = first.clone();
    for item in &items[1..] {
        let common = prefix
            .chars()
            .zip(item.chars())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(
            prefix
                .char_indices()
                .nth(common)
                .map_or(prefix.len(), |(i, _)| i),
        );
    }
    prefix
}

// ---------------------------------------------------------------------------
// The input field (used by both the main window and the quick panel)

pub struct InputFieldResult {
    pub submitted: bool,
    pub response: egui::Response,
}

pub struct InputField<'a> {
    pub session: &'a mut Session,
    pub completion: &'a mut CompletionState,
    pub palette: &'a Palette,
    pub font_size: f32,
    pub hint: &'a str,
    pub id: egui::Id,
}

impl InputField<'_> {
    pub fn show(mut self, ui: &mut egui::Ui) -> InputFieldResult {
        let id = self.id;
        let had_focus = ui.ctx().memory(|m| m.has_focus(id));
        let mut set_cursor_to_end = false;

        if had_focus {
            self.handle_keys(ui, &mut set_cursor_to_end);
        }

        let palette = *self.palette;
        let font_size = self.font_size;
        let mut layouter = move |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = highlight_input(buf.as_str(), &palette, font_size);
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };

        let before_edit = self.session.input.clone();
        let output = egui::TextEdit::singleline(&mut self.session.input)
            .id(id)
            .font(FontId::new(self.font_size, FontFamily::Monospace))
            .hint_text(RichText::new(self.hint).color(self.palette.text_faint))
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(f32::INFINITY)
            .lock_focus(true) // Tab is ours (completion)
            .layouter(&mut layouter)
            .show(ui);
        let response = output.response.response;

        if self.session.input != before_edit {
            // The user typed: restart history navigation and close the popup.
            self.session.on_input_edited();
            self.completion.close();
        }

        if set_cursor_to_end {
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            let end = CCursor::new(self.session.input.chars().count());
            state.cursor.set_char_range(Some(CCursorRange::one(end)));
            state.store(ui.ctx(), id);
        }

        let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        self.show_completion_popup(ui, &response);

        InputFieldResult {
            submitted,
            response,
        }
    }

    fn handle_keys(&mut self, ui: &mut egui::Ui, set_cursor_to_end: &mut bool) {
        // Tab: complete the word before the cursor.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
            self.complete(ui);
            *set_cursor_to_end = true;
            return;
        }

        // Escape: close the completion popup (the caller may use Escape
        // for other things when the popup is closed).
        if self.completion.is_open()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.completion.close();
            return;
        }

        // Up/Down: command history.
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
            && self.session.navigate_up()
        {
            self.completion.close();
            *set_cursor_to_end = true;
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown))
            && self.session.navigate_down()
        {
            self.completion.close();
            *set_cursor_to_end = true;
        }
    }

    /// Tab handler: first press completes to the longest common prefix and
    /// opens the candidate list; further presses cycle through candidates.
    fn complete(&mut self, ui: &egui::Ui) {
        let text = self.session.input.clone();

        if self.completion.open && !self.completion.items.is_empty() {
            let next = match self.completion.index {
                None => 0,
                Some(i) => (i + 1) % self.completion.items.len(),
            };
            self.apply_completion(next);
            return;
        }

        // Find the word ending at the cursor (or at the end of the text).
        let cursor_byte = TextEditState::load(ui.ctx(), self.id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                text.char_indices()
                    .nth(r.primary.index)
                    .map_or(text.len(), |(i, _)| i)
            })
            .unwrap_or(text.len());
        let word_start = text[..cursor_byte]
            .char_indices()
            .rev()
            .take_while(|(_, c)| is_word_char(*c))
            .last()
            .map_or(cursor_byte, |(i, _)| i);
        let word = &text[word_start..cursor_byte];
        if word.is_empty() {
            return;
        }

        let items = self.session.engine.completions(word);
        match items.len() {
            0 => {}
            1 => {
                self.completion.items = items;
                self.completion.word_start = word_start;
                self.completion.applied_len = cursor_byte - word_start;
                self.apply_completion(0);
                self.completion.close();
            }
            _ => {
                let prefix = longest_common_prefix(&items);
                self.completion.word_start = word_start;
                self.completion.applied_len = cursor_byte - word_start;
                self.completion.items = items;
                self.completion.index = None;
                self.completion.open = true;
                if prefix.len() > word.len() {
                    self.replace_word(&prefix);
                }
            }
        }
    }

    fn apply_completion(&mut self, index: usize) {
        let item = self.completion.items[index].clone();
        self.replace_word(&item);
        self.completion.index = Some(index);
    }

    fn replace_word(&mut self, replacement: &str) {
        let start = self.completion.word_start;
        let end = (start + self.completion.applied_len).min(self.session.input.len());
        self.session.input.replace_range(start..end, replacement);
        self.completion.applied_len = replacement.len();
    }

    fn show_completion_popup(&mut self, ui: &mut egui::Ui, response: &egui::Response) {
        if !self.completion.open || self.completion.items.is_empty() {
            return;
        }
        // Close if the field lost focus (e.g. user clicked elsewhere).
        if !ui.ctx().memory(|m| m.has_focus(self.id)) {
            self.completion.close();
            return;
        }

        let popup_id = self.id.with("completions");
        let selected = self.completion.index;
        let mut clicked: Option<usize> = None;

        egui::Area::new(popup_id)
            .order(egui::Order::Foreground)
            .fixed_pos(response.rect.left_bottom() + egui::vec2(0.0, 8.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(self.palette.card)
                    .stroke(Stroke::new(1.0, self.palette.border))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::same(6))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: Color32::from_black_alpha(80),
                    })
                    .show(ui, |ui| {
                        ui.set_min_width(220.0);
                        egui::ScrollArea::vertical()
                            .max_height(180.0)
                            .show(ui, |ui| {
                                for (i, item) in self.completion.items.iter().enumerate() {
                                    let is_selected = selected == Some(i);
                                    let label = ui.selectable_label(
                                        is_selected,
                                        RichText::new(item).monospace().size(self.font_size - 1.0),
                                    );
                                    if is_selected {
                                        label.scroll_to_me(None);
                                    }
                                    if label.clicked() {
                                        clicked = Some(i);
                                    }
                                }
                            });
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("tab cycle · esc dismiss")
                                .size(10.0)
                                .color(self.palette.text_faint),
                        );
                    });
            });

        if let Some(i) = clicked {
            self.apply_completion(i);
            self.completion.close();
            ui.ctx().memory_mut(|m| m.request_focus(self.id));
        }
    }
}

// ---------------------------------------------------------------------------
// History entry cards

pub enum EntryAction {
    None,
    Delete,
    Reuse(String),
    CopyResult(String),
    ClearAll,
}

/// One evaluated line, rendered as a card. Action buttons appear on hover.
pub fn entry_card(
    ui: &mut egui::Ui,
    entry: &HistoryEntry,
    palette: &Palette,
    font_size: f32,
) -> EntryAction {
    let mut action = EntryAction::None;

    // Hover state from the previous frame's card rect (one-frame lag is
    // invisible in practice).
    let hover_id = ui.id().with("card_rect");
    let hovered = ui
        .ctx()
        .data(|d| d.get_temp::<egui::Rect>(hover_id))
        .zip(ui.ctx().pointer_hover_pos())
        .is_some_and(|(rect, pos)| rect.contains(pos));

    let frame_response = Frame::new()
        .fill(palette.card)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 7.0;

            // Input line, with action buttons appearing on the right on hover.
            ui.horizontal(|ui| {
                let job = highlight_input(&entry.input, palette, font_size);
                ui.add(egui::Label::new(job).wrap());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !hovered {
                        return;
                    }
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let subtle = |text: &str| {
                        egui::Button::new(RichText::new(text).size(15.0).color(palette.text_dim))
                            .frame(false)
                    };
                    if ui
                        .add(subtle("🗙"))
                        .on_hover_text("Remove from history")
                        .clicked()
                    {
                        action = EntryAction::Delete;
                    }
                    if let Some(plain) = &entry.result_plain {
                        if ui.add(subtle("📋")).on_hover_text("Copy result").clicked() {
                            action = EntryAction::CopyResult(plain.clone());
                        }
                    }
                    if ui.add(subtle("↻")).on_hover_text("Edit again").clicked() {
                        action = EntryAction::Reuse(entry.input.clone());
                    }
                });
            });

            for printed in &entry.printed {
                ui.add(egui::Label::new(markup_job(printed, palette, font_size)).wrap());
            }

            if let Some(result) = &entry.result {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("=")
                            .monospace()
                            .size(font_size + 3.0)
                            .color(palette.accent),
                    );
                    let job = markup_job(result, palette, font_size + 3.0);
                    let label = ui
                        .add(egui::Label::new(job).wrap().sense(egui::Sense::click()))
                        .on_hover_cursor(egui::CursorIcon::Copy)
                        .on_hover_text("Click to copy");
                    if label.clicked() {
                        if let Some(plain) = &entry.result_plain {
                            action = EntryAction::CopyResult(plain.clone());
                        }
                    }
                });
            }

            if let Some(error) = &entry.error {
                ui.label(
                    RichText::new(error)
                        .monospace()
                        .size(font_size - 1.0)
                        .color(palette.error),
                );
            }
        })
        .response;

    ui.ctx()
        .data_mut(|d| d.insert_temp(hover_id, frame_response.rect));

    frame_response.context_menu(|ui| {
        if ui.button("↻ Edit again").clicked() {
            action = EntryAction::Reuse(entry.input.clone());
            ui.close();
        }
        if ui.button("📋 Copy input").clicked() {
            action = EntryAction::CopyResult(entry.input.clone());
            ui.close();
        }
        if let Some(plain) = &entry.result_plain {
            if ui.button("📋 Copy result").clicked() {
                action = EntryAction::CopyResult(plain.clone());
                ui.close();
            }
        }
        ui.separator();
        if ui.button("🗙 Remove from history").clicked() {
            action = EntryAction::Delete;
            ui.close();
        }
        if ui.button("🗑 Clear all").clicked() {
            action = EntryAction::ClearAll;
            ui.close();
        }
    });

    action
}
