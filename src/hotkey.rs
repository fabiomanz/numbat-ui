//! System-wide hotkey that summons the quick panel.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

pub struct QuickPanelHotkey {
    manager: GlobalHotKeyManager,
    registered: Option<HotKey>,
    /// Set by the listener thread when the hotkey fires; drained by the app.
    pressed: Arc<AtomicBool>,
}

impl QuickPanelHotkey {
    /// Creates the manager and registers `combo`. Must be called on the main
    /// thread (macOS requirement). A repaint of the ROOT viewport is
    /// requested whenever the hotkey fires, so the app wakes up even while
    /// hidden. (Explicitly ROOT: a plain `request_repaint()` from this
    /// thread can race into a pass of the quick-panel viewport and target
    /// it instead — and eframe silently drops repaint requests for viewports
    /// that no longer exist, losing the hotkey press.)
    pub fn new(combo: &str, ctx: egui::Context) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("Failed to initialize global hotkeys: {e}"))?;

        let pressed = Arc::new(AtomicBool::new(false));
        {
            let pressed = Arc::clone(&pressed);
            std::thread::spawn(move || {
                while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                    if event.state() == HotKeyState::Pressed {
                        pressed.store(true, Ordering::SeqCst);
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
            });
        }

        let mut hotkey = Self {
            manager,
            registered: None,
            pressed,
        };
        hotkey.register(combo)?;
        Ok(hotkey)
    }

    /// Replaces the registered combo. On failure the old combo stays active.
    pub fn register(&mut self, combo: &str) -> Result<(), String> {
        let hotkey = parse_combo(combo)?;
        if self.registered == Some(hotkey) {
            return Ok(());
        }
        self.manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register \"{combo}\": {e}"))?;
        if let Some(old) = self.registered.replace(hotkey) {
            let _ = self.manager.unregister(old);
        }
        Ok(())
    }

    /// True once per hotkey press.
    pub fn take_pressed(&self) -> bool {
        self.pressed.swap(false, Ordering::SeqCst)
    }
}

pub fn parse_combo(combo: &str) -> Result<HotKey, String> {
    HotKey::from_str(combo.trim()).map_err(|e| format!("Invalid hotkey \"{combo}\": {e}"))
}

/// Human-readable form of a combo for hints, e.g. "Opt+Space" on macOS.
/// (Plain text: egui's default fonts have no ⌘/⌥/⇧ glyphs.)
pub fn display_combo(combo: &str) -> String {
    let pretty = |part: &str| -> String {
        let part = part.trim();
        if cfg!(target_os = "macos") {
            match part.to_ascii_lowercase().as_str() {
                "alt" | "option" => return "Opt".to_owned(),
                "super" | "cmd" | "command" | "meta" => return "Cmd".to_owned(),
                _ => {}
            }
        }
        let mut chars = part.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    };

    combo.split('+').map(pretty).collect::<Vec<_>>().join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_combos() {
        assert!(parse_combo("Alt+Space").is_ok());
        assert!(parse_combo("Ctrl+Alt+Space").is_ok());
        assert!(parse_combo("Cmd+Shift+KeyK").is_ok());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_combo("NotAKey+Space+Wat").is_err());
        assert!(parse_combo("").is_err());
    }

    #[test]
    fn display_is_readable() {
        let shown = display_combo("Alt+Space");
        assert!(shown.contains("Space"));
    }
}
