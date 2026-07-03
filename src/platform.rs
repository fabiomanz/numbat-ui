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
