//! Application state and per-frame orchestration: window lifecycle
//! (closing the main window hides it so the quick panel stays summonable),
//! global shortcuts, menu events, and theme synchronization.

use egui::{ViewportCommand, ViewportId};

use crate::config::AppConfig;
use crate::engine::Engine;
use crate::hotkey::QuickPanelHotkey;
use crate::session::Session;
use crate::theme::{self, Palette};
use crate::ui::{CompletionState, Toasts};

/// The switch back to the Regular activation policy (Dock icon + menu
/// bar) after the hidden main window was reopened. Switching in the same
/// runloop pass that (re)activated the app leaves the menu bar showing
/// the previous app's menus — macOS only rebuilds it when the policy
/// changes on an app whose activation has settled. So the switch waits
/// until the app has been active for a moment, and re-claims activation
/// afterwards (the policy change can drop it).
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum RegularPolicySwitch {
    Idle,
    WaitSettle { since: std::time::Instant },
    Reactivate { since: std::time::Instant },
}

pub struct NumbatApp {
    pub config: AppConfig,
    pub palette: Palette,
    /// The palette that was last applied to the egui style; `None` forces a
    /// re-apply (e.g. after settings changed).
    pub applied_palette: Option<(Palette, f32)>,

    pub session: Session,
    pub toasts: Toasts,
    pub completion: CompletionState,
    pub quick_completion: CompletionState,

    pub logo: Option<egui::TextureHandle>,

    // Window state.
    main_visible: bool,
    pub quick_open: bool,
    pub quick_just_opened: bool,
    pub quick_had_focus: bool,
    /// Activation/focus retries after opening the quick panel; macOS
    /// cooperative activation often ignores a background app's first request.
    pub quick_focus_nudges: u8,
    #[cfg(target_os = "macos")]
    regular_policy_switch: RegularPolicySwitch,
    /// Where the quick panel should appear (computed from the monitor size).
    pub quick_position: Option<egui::Pos2>,
    pub show_settings: bool,
    quitting: bool,
    /// Consecutive frames on which the quick panel window could not be
    /// created (see the catch_unwind in `ui`).
    quick_panel_retries: u8,

    // Settings dialog state.
    pub settings_draft: AppConfig,
    pub settings_error: Option<String>,

    pub hotkey: Option<QuickPanelHotkey>,
    pub hotkey_error: Option<String>,

    #[cfg(target_os = "macos")]
    last_dead_key: Option<String>,
    #[cfg(target_os = "macos")]
    menu: crate::platform::MacMenu,
    /// An Edit-menu action waiting to be injected into the focused
    /// viewport's input — see `apply_edit_action`.
    #[cfg(target_os = "macos")]
    pending_edit: Option<crate::platform::EditAction>,
    /// Set (from a notification observer) when the app becomes active, so a
    /// re-open from the Finder/Spotlight/Dock can bring the window back.
    #[cfg(target_os = "macos")]
    reactivated: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(target_os = "macos")]
    started_at: std::time::Instant,

    /// Frame counter for the debug screenshot harness (`NUMBAT_UI_SHOT`).
    #[cfg(debug_assertions)]
    debug_frame: u64,
}

impl NumbatApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let start_hidden = std::env::args().any(|arg| arg == "--hidden");

        // Keep the launch agent in sync (survives the app being moved).
        crate::autostart::reconcile(config.ui.launch_at_login);

        #[cfg(target_os = "macos")]
        {
            crate::platform::set_dock_icon();
            if start_hidden {
                crate::platform::set_dock_visible(false);
            }
        }

        if start_hidden {
            // The imperceptible-window trick (see `hide_main_window`);
            // eframe force-shows the window after the first painted frame,
            // even when it was requested hidden.
            cc.egui_ctx
                .send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MousePassthrough(true));
        }

        // Re-opening the app (Finder, Spotlight, Dock) while it runs hidden
        // in the background only *activates* the existing instance — and if
        // it silently stayed active (macOS often ignores `deactivate`), only
        // a "reopen" Apple event arrives. Watch for both so the main window
        // can be brought back.
        #[cfg(target_os = "macos")]
        let reactivated = {
            use std::sync::atomic::{AtomicBool, Ordering};
            let flag = std::sync::Arc::new(AtomicBool::new(false));
            let raise = {
                let flag = std::sync::Arc::clone(&flag);
                let ctx = cc.egui_ctx.clone();
                std::sync::Arc::new(move || {
                    log::debug!("App activation/reopen; reactivated flag set");
                    flag.store(true, Ordering::SeqCst);
                    ctx.request_repaint();
                })
            };
            let on_activate = std::sync::Arc::clone(&raise);
            crate::platform::observe_app_activation(move || on_activate());
            crate::platform::observe_app_reopen(move || raise());
            flag
        };

        let mut session = Session::new(Engine::new(config.format_options()));
        session.restore_history();

        let (hotkey, hotkey_error) =
            match QuickPanelHotkey::new(&config.ui.quick_panel_hotkey, cc.egui_ctx.clone()) {
                Ok(hotkey) => (Some(hotkey), None),
                Err(e) => {
                    log::warn!("{e}");
                    (None, Some(e))
                }
            };

        Self {
            settings_draft: config.clone(),
            config,
            palette: theme::DARK,
            applied_palette: None,
            session,
            toasts: Toasts::default(),
            completion: CompletionState::default(),
            quick_completion: CompletionState::default(),
            logo: load_logo(&cc.egui_ctx),
            main_visible: !start_hidden,
            quick_open: false,
            quick_just_opened: false,
            quick_had_focus: false,
            quick_focus_nudges: 0,
            #[cfg(target_os = "macos")]
            regular_policy_switch: RegularPolicySwitch::Idle,
            quick_position: None,
            show_settings: false,
            quitting: false,
            quick_panel_retries: 0,
            settings_error: None,
            hotkey,
            hotkey_error,
            #[cfg(target_os = "macos")]
            last_dead_key: None,
            #[cfg(target_os = "macos")]
            menu: crate::platform::MacMenu::install(),
            #[cfg(target_os = "macos")]
            pending_edit: None,
            #[cfg(target_os = "macos")]
            reactivated,
            #[cfg(target_os = "macos")]
            started_at: std::time::Instant::now(),
            #[cfg(debug_assertions)]
            debug_frame: 0,
        }
    }

    /// Debug-build-only screenshot harness: with `NUMBAT_UI_SHOT=<dir>` the
    /// app types a demo calculation, captures the main window, the quick
    /// panel and the settings window as PNGs, then exits. Used to verify
    /// the UI without OS screen-recording permissions.
    #[cfg(debug_assertions)]
    fn debug_screenshot_harness(&mut self, ctx: &egui::Context) {
        let Ok(dir) = std::env::var("NUMBAT_UI_SHOT") else {
            return;
        };
        ctx.request_repaint(); // keep frames flowing

        self.debug_frame += 1;
        match self.debug_frame {
            20 => {
                for line in [
                    "1500 kcal -> kWh",
                    "let radius = 6371 km",
                    "2 pi radius -> miles",
                ] {
                    self.session.input = line.to_owned();
                    self.session.submit();
                }
                // Left in the input to demo the live preview.
                self.session.input = "sqrt(2) * 10 cm".to_owned();
            }
            60 => ctx.send_viewport_cmd_to(
                ViewportId::ROOT,
                ViewportCommand::Screenshot(egui::UserData::new("main")),
            ),
            80 => {
                // eframe cannot screenshot immediate viewports (their render
                // path passes no screenshot commands), so embed them into the
                // root window for capturing.
                ctx.set_embed_viewports(true);
                self.quick_open = true;
                self.quick_just_opened = true;
            }
            160 => ctx.send_viewport_cmd_to(
                ViewportId::ROOT,
                ViewportCommand::Screenshot(egui::UserData::new("quick")),
            ),
            200 => {
                self.quick_open = false;
                self.open_settings();
            }
            280 => ctx.send_viewport_cmd_to(
                ViewportId::ROOT,
                ViewportCommand::Screenshot(egui::UserData::new("settings")),
            ),
            320 => std::process::exit(0),
            _ => {}
        }
        let _ = dir;
    }

    /// Saves screenshot replies (from any viewport) for the debug harness.
    #[cfg(debug_assertions)]
    fn debug_save_screenshots(&self, raw_input: &egui::RawInput) {
        let Ok(dir) = std::env::var("NUMBAT_UI_SHOT") else {
            return;
        };
        for event in &raw_input.events {
            let egui::Event::Screenshot {
                image, user_data, ..
            } = event
            else {
                continue;
            };
            let name = user_data
                .data
                .as_ref()
                .and_then(|d| d.downcast_ref::<&str>().copied())
                .unwrap_or("main");
            let path = std::path::Path::new(&dir).join(format!("{name}.png"));
            let pixels: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|p| [p.r(), p.g(), p.b(), p.a()])
                .collect();
            if let Some(buffer) =
                image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
            {
                let _ = buffer.save(&path);
                log::info!("Saved screenshot {}", path.display());
            }
        }
    }

    /// Keeps the egui style in sync with the configured theme (which can
    /// also change with the system theme).
    fn sync_theme(&mut self, ctx: &egui::Context) {
        let desired = theme::palette_for(self.config.ui.theme, ctx);
        let font_size = self.config.ui.font_size;
        if self.applied_palette != Some((desired, font_size)) {
            theme::apply(ctx, &desired, font_size);
            self.palette = desired;
            self.applied_palette = Some((desired, font_size));
        }
    }

    pub fn open_main_window(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "macos")]
        if self.main_visible {
            crate::platform::set_dock_visible(true);
        } else {
            // The window was hidden, so the app runs under the Accessory
            // policy. Switching back to Regular is deferred until the
            // activation triggered by this reopen has settled — see
            // `RegularPolicySwitch`. The window itself shows right away.
            self.regular_policy_switch = RegularPolicySwitch::WaitSettle {
                since: std::time::Instant::now(),
            };
            ctx.request_repaint();
        }
        self.main_visible = true;
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MousePassthrough(false));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
    }

    /// "Hides" the main window by making it imperceptible — borderless,
    /// click-through and painting nothing — instead of actually hiding it.
    /// eframe renders OS-hidden windows without an event-loop context and
    /// can then never create new viewport windows, so a truly hidden main
    /// window would make opening the quick panel panic.
    fn hide_main_window(&mut self, ctx: &egui::Context) {
        self.main_visible = false;
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::MousePassthrough(true));
        // Without any window, the app should disappear from the Dock too;
        // it stays reachable through the global hotkey. Dropping to the
        // Accessory policy also deactivates the app, so keyboard focus
        // returns to the previously active app.
        #[cfg(target_os = "macos")]
        crate::platform::set_dock_visible(false);
    }

    pub fn toggle_quick_panel(&mut self) {
        if self.quick_open {
            self.close_quick_panel();
        } else {
            self.quick_open = true;
            self.quick_just_opened = true;
        }
    }

    pub(crate) fn close_quick_panel(&mut self) {
        self.quick_open = false;
        self.quick_completion.close();
        // Activating the app for the panel raised the `reactivated` flag,
        // and `ui` could not consume it while the root window was occluded
        // by the panel — discard it, or the main window pops up after the
        // panel closes.
        #[cfg(target_os = "macos")]
        self.reactivated
            .store(false, std::sync::atomic::Ordering::SeqCst);
        // If the panel held focus and no other window of ours is visible,
        // hand activation back to the previous app — otherwise this app
        // would stay active with no window to receive keystrokes.
        #[cfg(target_os = "macos")]
        if self.quick_had_focus && !self.main_visible && !self.show_settings {
            crate::platform::deactivate_app();
        }
    }

    pub fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.session.persist_history();
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Close);
    }

    /// Rebuilds the session by replaying the persisted history with a fresh
    /// engine — used when formatting options change, so all results are
    /// re-rendered with the new options. Equivalent to an app restart.
    pub fn refresh_history(&mut self) {
        let input = std::mem::take(&mut self.session.input);
        let mut session = Session::new(Engine::new(self.config.format_options()));
        session.restore_history();
        session.input = input;
        self.session = session;
    }

    /// Drains native macOS menu events. Runs from `logic` rather than `ui`
    /// so the menu keeps working while eframe considers the root viewport
    /// invisible (fully occluded or hidden) — e.g. shortcuts pressed in the
    /// quick panel while the main window is hidden. Edit actions are
    /// stashed and picked up by whichever viewport has keyboard focus.
    #[cfg(target_os = "macos")]
    fn poll_menu(&mut self, ctx: &egui::Context) {
        self.pending_edit = None;
        while let Some(action) = self.menu.poll() {
            use crate::platform::MenuAction;
            match action {
                MenuAction::OpenSettings => self.open_settings(),
                MenuAction::ShowMainWindow => self.open_main_window(ctx),
                MenuAction::ClearHistory => self.session.clear(),
                MenuAction::Quit => self.quit(ctx),
                MenuAction::Edit(action) => self.pending_edit = Some(action),
            }
        }
    }

    /// Injects the egui events for a pending Edit-menu action into the
    /// current pass's input. The native menu swallows the ⌘Z/X/C/V/A key
    /// equivalents before the window ever sees a key event, so the menu
    /// event is the only signal that the shortcut was pressed. Every
    /// viewport calls this at the top of its UI (before any text field is
    /// laid out); the one holding keyboard focus consumes the action.
    #[cfg(target_os = "macos")]
    pub(crate) fn apply_edit_action(&mut self, ctx: &egui::Context) {
        use crate::platform::EditAction;
        if self.pending_edit.is_none() || ctx.input(|i| i.viewport().focused) != Some(true) {
            return;
        }
        let Some(action) = self.pending_edit.take() else {
            return;
        };
        let key_event = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        let event = match action {
            EditAction::Undo => key_event(egui::Key::Z, egui::Modifiers::COMMAND),
            EditAction::Redo => key_event(
                egui::Key::Z,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            ),
            EditAction::Cut => egui::Event::Cut,
            EditAction::Copy => egui::Event::Copy,
            EditAction::SelectAll => key_event(egui::Key::A, egui::Modifiers::COMMAND),
            EditAction::Paste => match crate::platform::pasteboard_string() {
                Some(text) if !text.is_empty() => egui::Event::Paste(text.replace("\r\n", "\n")),
                _ => return,
            },
        };
        ctx.input_mut(|i| i.events.push(event));
    }

    fn handle_menu_and_shortcuts(&mut self, ctx: &egui::Context) {
        // A pending Edit-menu action targeting the main window.
        #[cfg(target_os = "macos")]
        self.apply_edit_action(ctx);

        // Global (in-app) shortcuts for the main viewport.
        let mut clear = false;
        let mut settings = false;
        let mut quit = false;
        let mut hide = false;
        let mut copy_last = false;
        ctx.input_mut(|i| {
            clear = i.consume_key(egui::Modifiers::COMMAND, egui::Key::L);
            copy_last = i.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::C,
            );
            // Settings/Quit come from the native menu on macOS.
            if !cfg!(target_os = "macos") {
                settings = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma);
                quit = i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q);
            }
            quit |= i.consume_key(egui::Modifiers::CTRL, egui::Key::D);
            hide = i.consume_key(egui::Modifiers::COMMAND, egui::Key::W);
        });

        if clear {
            self.session.clear();
        }
        if copy_last {
            if let Some(text) = self.session.last_result_plain().map(str::to_owned) {
                self.copy_to_clipboard(ctx, text);
            }
        }
        if settings {
            self.open_settings();
        }
        if hide {
            self.hide_main_window(ctx);
        }
        if quit {
            self.quit(ctx);
        }
    }

    /// Shows the quick panel and settings viewports. Called from `logic`
    /// rather than `ui` so both keep working while eframe considers the
    /// root viewport invisible (fully occluded or hidden); the debug
    /// screenshot harness calls it from `ui` instead when viewports are
    /// embedded into the root window.
    fn show_child_viewports(&mut self, ctx: &egui::Context) {
        if self.quick_open {
            // Safety net: if every window is OS-hidden or minimized, eframe
            // cannot create the panel window and egui panics ("the user
            // callback was never called"). Catch it, restore the root
            // window, and retry on the next frame.
            let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.quick_panel_viewport(ctx);
            }));
            match shown {
                Ok(()) => self.quick_panel_retries = 0,
                Err(_) if self.quick_panel_retries < 10 => {
                    self.quick_panel_retries += 1;
                    log::warn!("Could not create the quick panel window; retrying");
                    #[cfg(target_os = "macos")]
                    crate::platform::unhide_app();
                    ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Minimized(false));
                    ctx.request_repaint();
                }
                Err(_) => {
                    log::error!("Giving up on opening the quick panel");
                    self.quick_open = false;
                    self.quick_panel_retries = 0;
                }
            }
        }

        if self.show_settings {
            self.settings_viewport(ctx);
        }
    }
}

impl eframe::App for NumbatApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, _raw_input: &mut egui::RawInput) {
        #[cfg(target_os = "macos")]
        crate::platform::fix_macos_dead_keys(_raw_input, &mut self.last_dead_key);
        #[cfg(debug_assertions)]
        self.debug_save_screenshots(_raw_input);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // The quick panel window is transparent; every opaque surface paints
        // its own background.
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Runs every frame, even while eframe considers the root viewport
    /// invisible — `ui` does not. The imperceptible background window is
    /// routinely fully occluded (covered by other windows, app hidden via
    /// Cmd+H or "Hide Others"), and eframe skips `ui` in that state; with
    /// the hotkey handled in `ui`, the app went permanently deaf to the
    /// hotkey once that happened. No painting is allowed in here — child
    /// viewports are separate passes and are fine.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_theme(ctx);

        // Native menu events — drained before the child viewports are
        // shown, so a stashed Edit action reaches the quick panel or the
        // settings window in this same frame.
        #[cfg(target_os = "macos")]
        self.poll_menu(ctx);

        // The global hotkey fired (possibly while every window was hidden
        // or occluded).
        if self.hotkey.as_ref().is_some_and(|h| h.take_pressed()) {
            self.toggle_quick_panel();
            log::debug!("Hotkey press consumed; quick_open={}", self.quick_open);
            #[cfg(target_os = "macos")]
            if self.quick_open {
                // A hidden app's windows are all ordered out and eframe
                // could then never create the panel window.
                crate::platform::unhide_app();
                // Request activation while the press is fresh: macOS is far
                // more willing to activate a background app right after the
                // user interaction that asked for it.
                crate::platform::activate_app();
            }
        }

        // The app was re-opened (Finder, Spotlight, Dock) while running
        // hidden in the background: bring the main window back. Handled
        // here rather than in `ui` because `ui` is skipped while the root
        // window is occluded — which the imperceptible hidden window
        // routinely is. Activations caused by the quick panel itself are
        // discarded by the `quick_open` check (the flag is consumed either
        // way). The startup grace period keeps a `--hidden` login launch
        // in the background.
        #[cfg(target_os = "macos")]
        if self
            .reactivated
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            log::debug!(
                "Reactivated: main_visible={} quick_open={} show_settings={} elapsed={}s",
                self.main_visible,
                self.quick_open,
                self.show_settings,
                self.started_at.elapsed().as_secs()
            );
            if !self.main_visible
                && !self.quick_open
                && !self.show_settings
                && self.started_at.elapsed().as_secs() >= 2
            {
                log::debug!("Opening main window after reactivation");
                self.open_main_window(ctx);
            }
        }

        // Complete a deferred Accessory→Regular policy switch (Dock icon,
        // menu bar): switch once the app has been active for a moment
        // (switching too early leaves a stale menu bar; if activation is
        // never granted, switch anyway on timeout), then re-claim
        // activation, which the policy change tends to drop.
        #[cfg(target_os = "macos")]
        {
            use crate::platform::{activate_app, is_app_active};
            use std::time::{Duration, Instant};
            const FRAME: Duration = Duration::from_millis(50);
            self.regular_policy_switch = match self.regular_policy_switch {
                RegularPolicySwitch::Idle => RegularPolicySwitch::Idle,
                _ if !self.main_visible => RegularPolicySwitch::Idle,
                RegularPolicySwitch::WaitSettle { since } => {
                    let elapsed = since.elapsed();
                    if (is_app_active() && elapsed >= Duration::from_millis(300))
                        || elapsed >= Duration::from_millis(1500)
                    {
                        log::debug!("Switching to the Regular activation policy");
                        crate::platform::set_dock_visible(true);
                        ctx.request_repaint_after(FRAME);
                        RegularPolicySwitch::Reactivate {
                            since: Instant::now(),
                        }
                    } else {
                        if !is_app_active() {
                            activate_app();
                            ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                        }
                        ctx.request_repaint_after(FRAME);
                        RegularPolicySwitch::WaitSettle { since }
                    }
                }
                RegularPolicySwitch::Reactivate { since } => {
                    if is_app_active() || since.elapsed() >= Duration::from_millis(1000) {
                        // Belt and braces: macOS sometimes keeps showing the
                        // previous app's menu bar after the policy switch
                        // even though this app is active.
                        self.menu.reinstall();
                        RegularPolicySwitch::Idle
                    } else {
                        activate_app();
                        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Focus);
                        ctx.request_repaint_after(FRAME);
                        RegularPolicySwitch::Reactivate { since }
                    }
                }
            };
        }

        if !ctx.embed_viewports() {
            self.show_child_viewports(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        #[cfg(debug_assertions)]
        self.debug_screenshot_harness(&ctx);

        // Closing the main window hides it; the app keeps running so the
        // quick panel remains summonable. Quitting goes through `quit()`.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.hide_main_window(&ctx);
        }

        self.handle_menu_and_shortcuts(&ctx);

        if self.main_visible {
            self.main_window_ui(ui);
        }

        // Embedded viewports (debug screenshot harness) paint into the root
        // window, so they must be laid out here rather than in `logic`.
        if ctx.embed_viewports() {
            self.show_child_viewports(&ctx);
        }
    }
}

fn load_logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(include_bytes!("icons/icon.png"))
        .ok()?
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    Some(ctx.load_texture("logo", color_image, egui::TextureOptions::LINEAR))
}
