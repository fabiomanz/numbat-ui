//! The Spotlight-style quick panel: a borderless, always-on-top window
//! summoned with a global hotkey for fast calculations. The session is
//! shared with the main window, so anything typed or evaluated here can be
//! picked up seamlessly in the full window ("Open in window", ⌘/Ctrl+⏎).

use egui::text::{CCursor, CCursorRange};
use egui::{
    Color32, CornerRadius, Frame, Margin, RichText, Stroke, ViewportBuilder, ViewportCommand,
    ViewportId,
};

use crate::app::NumbatApp;
use crate::engine::EvalError;
use crate::theme::{markup_job, Palette};
use crate::ui::InputField;

pub const PANEL_WIDTH: f32 = 680.0;
pub const PANEL_HEIGHT: f32 = 132.0;

fn input_id() -> egui::Id {
    egui::Id::new("quick_input")
}

impl NumbatApp {
    /// Called from the root viewport each frame while the panel is open.
    pub fn quick_panel_viewport(&mut self, ctx: &egui::Context) {
        // Compute the position once per open — it depends on the mouse
        // location, and the panel should not follow later mouse movement.
        if self.quick_just_opened {
            self.quick_position = self.quick_panel_position(ctx);
        }

        // The window is created invisible and revealed on the first frame,
        // after it has been positioned — otherwise it briefly flashes at a
        // default position before jumping to the center.
        let mut builder = ViewportBuilder::default()
            .with_title("Numbat Quick")
            .with_inner_size([PANEL_WIDTH, PANEL_HEIGHT])
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_has_shadow(false)
            .with_taskbar(false)
            .with_visible(false)
            .with_always_on_top();
        if let Some(position) = self.quick_position {
            builder = builder.with_position(position);
        }

        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("quick_panel"),
            builder,
            |ui, _class| {
                self.quick_panel_ui(ui);
            },
        );
    }

    /// Roughly Spotlight's position: horizontally centered, upper third —
    /// on the screen the mouse is on, falling back to the main window's.
    fn quick_panel_position(&self, ctx: &egui::Context) -> Option<egui::Pos2> {
        #[cfg(target_os = "macos")]
        if let Some(screen) = crate::platform::screen_rect_under_mouse() {
            return Some(egui::pos2(
                screen.left() + (screen.width() - PANEL_WIDTH) / 2.0,
                screen.top() + screen.height() * 0.22,
            ));
        }
        let monitor = ctx.input(|i| i.viewport().monitor_size)?;
        if monitor.x <= 0.0 || monitor.y <= 0.0 {
            return None;
        }
        Some(egui::pos2(
            (monitor.x - PANEL_WIDTH) / 2.0,
            monitor.y * 0.22,
        ))
    }

    fn quick_panel_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        let palette = self.palette;

        // Window-level events.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.close_quick_panel();
            return;
        }

        // A pending Edit-menu action (⌘Z/X/C/V/A) targeting this panel.
        #[cfg(target_os = "macos")]
        self.apply_edit_action(ctx);

        if self.quick_just_opened {
            self.quick_just_opened = false;
            self.quick_had_focus = false;
            self.quick_focus_nudges = 0;
            // Spotlight-style: the input is kept across closes (the session
            // is shared with the main window, so dropping it would discard a
            // draft there), but it comes up fully selected — the next
            // keystroke replaces it, while it stays readable and copyable.
            self.quick_select_all = true;
            self.quick_auto_selection = false;
            // The window was created hidden (and already at its final
            // position, via the builder). Configure its Space behavior while
            // it is still hidden, then reveal and focus: the commands are
            // processed in order after this frame is painted.
            #[cfg(target_os = "macos")]
            crate::platform::prepare_quick_panel_window("Numbat Quick");
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }

        // Focus management. `focused` is `None` until the OS reported a
        // focus state — never treat that as focused, or the panel closes
        // itself on the first `Some(false)` reading.
        let focused = ctx.input(|i| i.viewport().focused);
        if focused == Some(true) {
            self.quick_had_focus = true;
        } else if self.quick_had_focus {
            // Auto-hide once focus was gained and is lost (like Spotlight).
            if focused == Some(false) {
                self.close_quick_panel();
                return;
            }
        } else if self.quick_focus_nudges < 15 {
            // Not focused yet: macOS cooperative activation routinely
            // ignores a background app's first activation request, so keep
            // nudging for a short while. If it is never granted, the panel
            // stays open (on top) and a click will focus it.
            self.quick_focus_nudges += 1;
            #[cfg(target_os = "macos")]
            crate::platform::activate_app();
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Escape, in order: close the completion popup (handled by the input
        // field itself), clear a non-empty input, close the panel. Clearing
        // first means a stale expression is one keypress away from gone
        // without losing the panel — the last evaluated result stays visible.
        if !self.quick_completion.is_open()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            // Trimmed, to match the `input_empty` the key hint is drawn from.
            if self.session.input.trim().is_empty() {
                self.close_quick_panel();
                return;
            }
            self.session.input.clear();
            self.session.on_input_edited();
            self.quick_select_all = false;
        }

        // ⌘/Ctrl+⏎: continue in the full window.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
            self.open_main_window(ctx);
            self.close_quick_panel();
            return;
        }

        // ⌘/Ctrl+C without a text selection: copy the current result. The
        // shortcut arrives as an `Event::Copy`, never as a key event; with
        // a selection it is left for the input field to handle.
        let selection = egui::text_edit::TextEditState::load(ctx, input_id())
            .and_then(|state| state.cursor.char_range())
            .map(|range| range.as_sorted_char_range());
        // The selection made on open (`apply_quick_select_all`) is not one
        // the user asked for, so ⌘C keeps copying the result until they
        // touch the input. Once the selection is no longer the full text the
        // latch drops for good, so a later ⌘A ⌘C copies the input as usual.
        if self.quick_auto_selection
            && selection.as_ref() != Some(&(0..self.session.input.chars().count()))
        {
            self.quick_auto_selection = false;
        }
        let has_selection =
            selection.is_some_and(|range| !range.is_empty()) && !self.quick_auto_selection;
        if !has_selection {
            let copy_requested = ctx.input_mut(|i| {
                let before = i.events.len();
                i.events.retain(|e| !matches!(e, egui::Event::Copy));
                i.events.len() != before
            });
            if copy_requested {
                self.quick_copy_result(ctx);
            }
        }

        egui::CentralPanel::default()
            .frame(Frame::new().fill(Color32::TRANSPARENT))
            .show_inside(ui, |ui| {
                Frame::new()
                    .fill(palette.card)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::ZERO)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 6],
                        blur: 28,
                        spread: 0,
                        color: Color32::from_black_alpha(100),
                    })
                    .show(ui, |ui| {
                        // Fixed content size, also when rendered embedded
                        // (debug screenshot harness).
                        let size = egui::vec2(ui.available_width(), PANEL_HEIGHT - 2.0);
                        ui.set_min_size(size);
                        ui.set_max_size(size);
                        self.quick_panel_content(ui);
                    });
            });

        self.toasts.ui(ctx, &palette, "quick");
    }

    /// Selects the whole retained input after an open, Spotlight-style. The
    /// field only gains focus a frame or two later (macOS routinely denies a
    /// background app's first activation request), so this keeps applying
    /// until it does — but a click in that window cancels it, leaving the
    /// caret where the user put it.
    fn apply_quick_select_all(&mut self, ctx: &egui::Context) {
        if !self.quick_select_all {
            return;
        }
        let len = self.session.input.chars().count();
        if len == 0 || ctx.input(|i| i.pointer.any_pressed()) {
            self.quick_select_all = false;
            return;
        }

        let id = input_id();
        let mut state = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(len))));
        state.store(ctx, id);
        self.quick_auto_selection = true;

        if ctx.memory(|m| m.has_focus(id)) {
            self.quick_select_all = false;
        }
    }

    fn quick_panel_content(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette;
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

        // Must run before the input field is shown: `TextEdit` loads its
        // cursor state at the start of its own `show`.
        self.apply_quick_select_all(ui.ctx());

        // Top row: logo, big input, "open in window" action.
        let top_height = 58.0;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), top_height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(16.0);
                ui.spacing_mut().item_spacing.x = 12.0;
                if let Some(logo) = &self.logo {
                    ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(26.0, 26.0)));
                }

                // Right-side action first so the input takes the rest.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(14.0);
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let open =
                        egui::Button::new(RichText::new("⬈").size(17.0).color(Color32::WHITE))
                            .fill(palette.accent)
                            .corner_radius(CornerRadius::same(8))
                            .min_size(egui::vec2(34.0, 30.0));
                    if ui
                        .add(open)
                        .on_hover_text("Open in window   (cmd/ctrl+enter)")
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        self.open_main_window(&ctx);
                        self.close_quick_panel();
                    }

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let result = InputField {
                            session: &mut self.session,
                            completion: &mut self.quick_completion,
                            palette: &palette,
                            font_size: 20.0,
                            hint: "Calculate…",
                            id: input_id(),
                        }
                        .show(ui);

                        if result.submitted {
                            self.session.submit();
                        }
                        result.response.request_focus();
                    });
                });
            },
        );

        // Hairline separator.
        let rect = ui.max_rect();
        let y = ui.cursor().top();
        ui.painter().hline(
            rect.left()..=rect.right(),
            y,
            egui::Stroke::new(1.0, palette.border),
        );

        // Bottom row: the result (live preview, or last evaluated when the
        // input is empty), with quiet key hints on the right.
        let input_empty = self.session.input.trim().is_empty();
        let preview = self.session.preview();

        let mut shown: Option<(egui::text::LayoutJob, Option<String>)> = None;
        let mut stale = false;
        let mut error: Option<EvalError> = None;
        if let Some(preview) = preview {
            shown = Some((
                markup_job(&preview.markup, &palette, 19.0),
                Some(preview.plain),
            ));
            stale = !preview.fresh;
        } else if input_empty {
            if let Some(last) = self.session.entries.last() {
                if let Some(result) = &last.result {
                    shown = Some((
                        markup_job(result, &palette, 19.0),
                        last.result_plain.clone(),
                    ));
                } else {
                    error = last.error.clone();
                }
            }
        }
        let has_result = shown.is_some();

        // The key hints are laid out first (right to left) so that a wrapped
        // error message only uses the width actually left over, instead of
        // pushing the hints off the panel.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.available_height().max(40.0)),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.add_space(16.0);
                let cmd = if cfg!(target_os = "macos") {
                    "cmd"
                } else {
                    "ctrl"
                };
                // Escape clears a non-empty input before it closes.
                let escape = if input_empty {
                    "esc close"
                } else {
                    "esc clear"
                };
                let hint = if has_result {
                    format!("{cmd}+C copy  ·  {escape}")
                } else {
                    escape.to_owned()
                };
                ui.label(RichText::new(hint).size(11.0).color(palette.text_faint));
                ui.add_space(10.0);

                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(16.0);
                    ui.spacing_mut().item_spacing.x = 10.0;

                    match (shown, error) {
                        (Some((job, plain)), _) => {
                            if stale {
                                ui.set_opacity(0.45);
                            }
                            ui.label(
                                RichText::new("=")
                                    .monospace()
                                    .size(18.0)
                                    .color(palette.text_faint),
                            );
                            let label = ui
                                .add(egui::Label::new(job).truncate().sense(egui::Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::Copy)
                                .on_hover_text("Click to copy");
                            if label.clicked() {
                                if let Some(plain) = plain {
                                    self.copy_to_clipboard(ui.ctx(), plain);
                                }
                            }
                        }
                        (None, Some(error)) => error_label(ui, &error, &palette),
                        (None, None) => {
                            ui.label(
                                RichText::new("Results appear as you type")
                                    .size(12.5)
                                    .color(palette.text_faint),
                            );
                        }
                    }
                });
            },
        );
    }

    /// Copies the live preview if present, otherwise the last result.
    fn quick_copy_result(&mut self, ctx: &egui::Context) {
        let preview = self.session.preview().map(|p| p.plain);
        let text = preview.or_else(|| self.session.last_result_plain().map(str::to_owned));
        if let Some(text) = text {
            self.copy_to_clipboard(ctx, text);
        }
    }
}

/// The condensed error, wrapped into the three lines the panel has room for.
/// The full compiler-style diagnostic is one hover away — showing it inline
/// would not fit, and its source snippet only repeats the input field above.
fn error_label(ui: &mut egui::Ui, error: &EvalError, palette: &Palette) {
    let mut job = egui::text::LayoutJob::simple(
        error.summary.clone(),
        egui::FontId::monospace(12.5),
        palette.error,
        ui.available_width(),
    );
    job.wrap.max_rows = 3;
    job.wrap.overflow_character = Some('…');
    // Laid out here rather than by the `Label`: a label re-lays a `LayoutJob`
    // with the wrapping of its `Ui` (`Extend` inside a horizontal layout,
    // which would push the key hints off the panel), while a ready-made
    // galley is drawn as-is.
    let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(job));

    ui.add(egui::Label::new(galley).sense(egui::Sense::hover()))
        .on_hover_ui(|ui| {
            // The diagnostic aligns its span markers with the source line, so
            // it must not be re-wrapped.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.label(
                RichText::new(&error.full)
                    .monospace()
                    .size(12.0)
                    .color(palette.error),
            );
        });
}
