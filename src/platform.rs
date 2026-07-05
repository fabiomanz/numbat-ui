//! Platform-specific integration: the native macOS menu bar and a
//! workaround for macOS dead keys in math input.

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "macos")]
mod macos {
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

    /// Sets the Dock (and About panel) icon at runtime. Without this, a
    /// bare binary launched outside the .app bundle shows the generic
    /// executable icon.
    pub fn set_dock_icon() {
        use objc2::AnyThread;
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;

        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let bytes = include_bytes!("icons/icon_rounded.png");
        let data = NSData::with_bytes(bytes);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            log::warn!("Failed to decode the dock icon");
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        unsafe { app.setApplicationIconImage(Some(&image)) };
    }

    /// Shows or hides the app in the Dock (and app switcher) by toggling the
    /// activation policy between Regular and Accessory. Hidden is used while
    /// no window is open, so the app lives on only behind its global hotkey.
    pub fn set_dock_visible(visible: bool) {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let policy = if visible {
            NSApplicationActivationPolicy::Regular
        } else {
            NSApplicationActivationPolicy::Accessory
        };
        app.setActivationPolicy(policy);
        if visible {
            // Returning to the Regular policy can reset the Dock tile to
            // the generic executable icon; re-apply ours.
            set_dock_icon();
        }
    }

    /// Asks macOS to bring the app to the foreground so the quick panel can
    /// take keyboard focus. Under cooperative activation (macOS 14+) a
    /// single request from a background app is often ignored — the panel
    /// therefore keeps re-requesting for a short while after opening.
    // `ActivateIgnoringOtherApps` is a no-op on macOS 14+, but still helps on 13.
    #[allow(deprecated)]
    pub fn activate_app() {
        use objc2_app_kit::{NSApplication, NSApplicationActivationOptions, NSRunningApplication};
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        NSRunningApplication::currentApplication()
            .activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }

    /// Hands activation back to the previously active app. Used when the
    /// quick panel closes while no other window of ours is visible; without
    /// this the now-windowless app would keep swallowing keystrokes.
    pub fn deactivate_app() {
        use objc2_app_kit::NSApplication;
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        NSApplication::sharedApplication(mtm).deactivate();
    }

    /// Marks the quick-panel window as joining every Space — including
    /// fullscreen ones — so it appears on whatever Space is active, like
    /// Spotlight, instead of opening on the Space the app's other windows
    /// live on (or not being visible at all over a fullscreen app). Must run
    /// after eframe created the (still hidden) window, i.e. on the panel's
    /// first frame.
    pub fn prepare_quick_panel_window(title: &str) {
        use objc2_app_kit::{NSApplication, NSWindowCollectionBehavior};
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        for window in NSApplication::sharedApplication(mtm).windows().iter() {
            if window.title().to_string() == title {
                window.setCollectionBehavior(
                    NSWindowCollectionBehavior::CanJoinAllSpaces
                        | NSWindowCollectionBehavior::FullScreenAuxiliary,
                );
            }
        }
    }

    /// The frame of the screen the mouse cursor is on, in points with a
    /// top-left origin (winit's coordinate space). The quick panel opens
    /// there — like Spotlight — rather than on the main window's screen.
    pub fn screen_rect_under_mouse() -> Option<egui::Rect> {
        use objc2_app_kit::{NSEvent, NSScreen};
        let mtm = objc2::MainThreadMarker::new()?;
        let mouse = NSEvent::mouseLocation();
        let screens = NSScreen::screens(mtm);
        // Cocoa coordinates have a bottom-left origin (y grows upward),
        // anchored to the primary screen — the first one in the list.
        let primary_height = screens.iter().next()?.frame().size.height;
        for screen in screens.iter() {
            let frame = screen.frame();
            let (x, y) = (frame.origin.x, frame.origin.y);
            let (w, h) = (frame.size.width, frame.size.height);
            if mouse.x >= x && mouse.x < x + w && mouse.y >= y && mouse.y < y + h {
                let top = primary_height - (y + h);
                return Some(egui::Rect::from_min_size(
                    egui::pos2(x as f32, top as f32),
                    egui::vec2(w as f32, h as f32),
                ));
            }
        }
        None
    }

    /// Calls `on_activate` (on the main thread) whenever the app becomes
    /// active — e.g. it is re-opened through the Finder, Spotlight or the
    /// Dock while already running in the background.
    pub fn observe_app_activation(on_activate: impl Fn() + 'static) {
        use block2::RcBlock;
        use objc2_app_kit::NSApplicationDidBecomeActiveNotification;
        use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};

        let block = RcBlock::new(move |_: core::ptr::NonNull<NSNotification>| on_activate());
        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            let token = center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidBecomeActiveNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            );
            // The observer lives for the whole app lifetime.
            std::mem::forget(token);
        }
    }

    /// Actions triggered from the native menu bar.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MenuAction {
        OpenSettings,
        ShowMainWindow,
        ClearHistory,
        Quit,
    }

    pub struct MacMenu {
        _menu: Menu,
        settings_id: MenuId,
        show_main_id: MenuId,
        clear_id: MenuId,
        quit_id: MenuId,
    }

    impl MacMenu {
        pub fn install() -> Self {
            let menu = Menu::new();

            let app_menu = Submenu::new("Numbat", true);
            let settings_item = MenuItem::new(
                "Settings…",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
            );
            // Custom quit item (instead of the predefined one, which calls
            // [NSApp terminate:] and would skip eframe's clean shutdown).
            let quit_item = MenuItem::new(
                "Quit Numbat",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyQ)),
            );
            let about = muda::AboutMetadata {
                name: Some("Numbat UI".to_owned()),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
                copyright: Some("© Fabio Manz · MIT License".to_owned()),
                ..Default::default()
            };
            let _ = app_menu.append_items(&[
                &PredefinedMenuItem::about(Some("About Numbat UI"), Some(about)),
                &PredefinedMenuItem::separator(),
                &settings_item,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &quit_item,
            ]);

            let edit_menu = Submenu::new("Edit", true);
            let _ = edit_menu.append_items(&[
                &PredefinedMenuItem::undo(None),
                &PredefinedMenuItem::redo(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::cut(None),
                &PredefinedMenuItem::copy(None),
                &PredefinedMenuItem::paste(None),
                &PredefinedMenuItem::select_all(None),
            ]);

            let history_menu = Submenu::new("History", true);
            let clear_item = MenuItem::new(
                "Clear History",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyL)),
            );
            let _ = history_menu.append_items(&[&clear_item]);

            let window_menu = Submenu::new("Window", true);
            let show_main_item = MenuItem::new(
                "Show Numbat",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit1)),
            );
            let _ = window_menu.append_items(&[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::close_window(None),
                &PredefinedMenuItem::separator(),
                &show_main_item,
            ]);

            let _ = menu.append_items(&[&app_menu, &edit_menu, &history_menu, &window_menu]);
            menu.init_for_nsapp();

            Self {
                settings_id: settings_item.id().clone(),
                show_main_id: show_main_item.id().clone(),
                clear_id: clear_item.id().clone(),
                quit_id: quit_item.id().clone(),
                _menu: menu,
            }
        }

        /// Drains pending native menu events.
        pub fn poll(&self) -> Option<MenuAction> {
            let event = muda::MenuEvent::receiver().try_recv().ok()?;
            if event.id == self.settings_id {
                Some(MenuAction::OpenSettings)
            } else if event.id == self.show_main_id {
                Some(MenuAction::ShowMainWindow)
            } else if event.id == self.clear_id {
                Some(MenuAction::ClearHistory)
            } else if event.id == self.quit_id {
                Some(MenuAction::Quit)
            } else {
                None
            }
        }
    }

    fn decompose_dead_key(c: char) -> Option<(char, char)> {
        match c {
            // Circumflex
            'â' => Some(('^', 'a')),
            'ê' => Some(('^', 'e')),
            'î' => Some(('^', 'i')),
            'ô' => Some(('^', 'o')),
            'û' => Some(('^', 'u')),
            'Â' => Some(('^', 'A')),
            'Ê' => Some(('^', 'E')),
            'Î' => Some(('^', 'I')),
            'Ô' => Some(('^', 'O')),
            'Û' => Some(('^', 'U')),
            // Grave
            'à' => Some(('`', 'a')),
            'è' => Some(('`', 'e')),
            'ì' => Some(('`', 'i')),
            'ò' => Some(('`', 'o')),
            'ù' => Some(('`', 'u')),
            'À' => Some(('`', 'A')),
            'È' => Some(('`', 'E')),
            'Ì' => Some(('`', 'I')),
            'Ò' => Some(('`', 'O')),
            'Ù' => Some(('`', 'U')),
            // Tilde
            'ã' => Some(('~', 'a')),
            'õ' => Some(('~', 'o')),
            'ñ' => Some(('~', 'n')),
            'Ã' => Some(('~', 'A')),
            'Õ' => Some(('~', 'O')),
            'Ñ' => Some(('~', 'N')),
            _ => None,
        }
    }

    /// Works around macOS dead keys for the math symbols `^`, `` ` `` and `~`:
    /// without this, typing `^` swallows the next keystroke into a composed
    /// character (like `ê`). Natural-language dead keys (acute etc.) are left
    /// untouched.
    pub fn fix_macos_dead_keys(raw_input: &mut egui::RawInput, last_dead_key: &mut Option<String>) {
        let mut new_events = Vec::with_capacity(raw_input.events.len());

        for event in std::mem::take(&mut raw_input.events) {
            match event {
                egui::Event::Ime(egui::ImeEvent::Preedit(ref text)) => {
                    if text == "^" || text == "`" || text == "~" {
                        *last_dead_key = Some(text.clone());
                        new_events.push(egui::Event::Ime(egui::ImeEvent::Commit(text.clone())));
                        new_events.push(egui::Event::Ime(egui::ImeEvent::Preedit(String::new())));
                    } else {
                        new_events.push(event);
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Commit(ref text)) => {
                    let mut uncombined = String::new();
                    for c in text.chars() {
                        if let Some((dead, base)) = decompose_dead_key(c) {
                            uncombined.push(dead);
                            uncombined.push(base);
                        } else {
                            uncombined.push(c);
                        }
                    }

                    if let Some(dead_key) = last_dead_key.take() {
                        if let Some(stripped) = uncombined.strip_prefix(&dead_key) {
                            uncombined = stripped.to_owned();
                        }
                    }

                    if !uncombined.is_empty() {
                        new_events.push(egui::Event::Ime(egui::ImeEvent::Commit(uncombined)));
                    }
                }
                _ => new_events.push(event),
            }
        }

        raw_input.events = new_events;
    }
}
