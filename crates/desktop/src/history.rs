//! Desktop history persistence: the submitted input lines live in a plain
//! text file next to the config, one line per entry.

use std::path::PathBuf;

use numbat_ui_core::HistoryStore;

pub struct FileHistoryStore;

impl HistoryStore for FileHistoryStore {
    fn load(&self) -> Vec<String> {
        if let Some(path) = history_path() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                return contents.lines().map(str::to_owned).collect();
            }
        }
        legacy_history_lines().unwrap_or_default()
    }

    fn save(&self, lines: &[String]) {
        // The debug screenshot harness submits demo lines; keep them out of
        // the real history file.
        #[cfg(debug_assertions)]
        if std::env::var("NUMBAT_UI_SHOT").is_ok() {
            return;
        }

        let Some(path) = history_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&path, lines.join("\n")) {
            log::warn!("Failed to persist history: {e}");
        }
    }
}

fn history_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("numbat-ui").join("history.numbat"))
}

/// Best-effort migration from the storage of numbat-ui 2.x, which kept the
/// command history in eframe's `app.ron` (a RON map with an "app" key
/// holding a RON-encoded `Vec<String>`).
fn legacy_history_lines() -> Option<Vec<String>> {
    let path = dirs::data_dir()?.join("Numbat UI").join("app.ron");
    let contents = std::fs::read_to_string(path).ok()?;
    let map: std::collections::HashMap<String, String> = ron::from_str(&contents).ok()?;
    let lines: Vec<String> = ron::from_str(map.get("app")?).ok()?;
    log::info!("Migrated {} history lines from numbat-ui 2.x", lines.len());
    Some(lines)
}
