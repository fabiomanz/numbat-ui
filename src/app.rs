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
        // in the background only *activates* the existing instance; watch for
        // that so the main window can be brought back.
        #[cfg(target_os = "macos")]
        let reactivated = {
            use std::sync::atomic::{AtomicBool, Ordering};
            let flag = std::sync::Arc::new(AtomicBool::new(false));
            let observer_flag = std::sync::Arc::clone(&flag);
            let ctx = cc.egui_ctx.clone();
            crate::platform::observe_app_activation(move || {
                observer_flag.store(true, Ordering::SeqCst);
                ctx.request_repaint();
            });
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
        self.main_visible = true;
        #[cfg(target_os = "macos")]
        crate::platform::set_dock_visible(true);
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
        self.quick_open = !self.quick_open;
        if self.quick_open {
            self.quick_just_opened = true;
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

    fn handle_menu_and_shortcuts(&mut self, ctx: &egui::Context) {
        // Native macOS menu events.
        #[cfg(target_os = "macos")]
        while let Some(action) = self.menu.poll() {
            use crate::platform::MenuAction;
            match action {
                MenuAction::OpenSettings => self.open_settings(),
                MenuAction::ShowMainWindow => self.open_main_window(ctx),
                MenuAction::ClearHistory => self.session.clear(),
                MenuAction::Quit => self.quit(ctx),
            }
        }

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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.sync_theme(&ctx);

        #[cfg(debug_assertions)]
        self.debug_screenshot_harness(&ctx);

        // The global hotkey fired (possibly while every window was hidden).
        if self.hotkey.as_ref().is_some_and(|h| h.take_pressed()) {
            self.toggle_quick_panel();
        }

        // The app was re-opened (Finder, Spotlight, Dock) while running
        // hidden in the background: bring the main window back. The startup
        // grace period keeps a `--hidden` login launch in the background.
        #[cfg(target_os = "macos")]
        if self
            .reactivated
            .swap(false, std::sync::atomic::Ordering::SeqCst)
            && !self.main_visible
            && !self.quick_open
            && !self.show_settings
            && self.started_at.elapsed().as_secs() >= 2
        {
            self.open_main_window(&ctx);
        }

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

        if self.quick_open {
            // Safety net: if every window is OS-hidden or minimized, eframe
            // cannot create the panel window and egui panics ("the user
            // callback was never called"). Catch it, restore the root
            // window, and retry on the next frame.
            let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.quick_panel_viewport(&ctx);
            }));
            match shown {
                Ok(()) => self.quick_panel_retries = 0,
                Err(_) if self.quick_panel_retries < 10 => {
                    self.quick_panel_retries += 1;
                    log::warn!("Could not create the quick panel window; retrying");
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
            self.settings_viewport(&ctx);
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
