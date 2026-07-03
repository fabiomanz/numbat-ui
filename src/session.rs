//! The calculator session shared by the main window and the quick panel:
//! evaluated entries, the current input line, command history with
//! navigation, the live preview cache, and history persistence.

use std::path::PathBuf;

use numbat::markup::Markup;

use crate::engine::Engine;

/// Maximum number of input lines persisted (and replayed on startup).
const MAX_PERSISTED_HISTORY: usize = 200;

pub struct HistoryEntry {
    pub input: String,
    pub printed: Vec<Markup>,
    pub result: Option<Markup>,
    pub result_plain: Option<String>,
    pub error: Option<String>,
}

pub struct Session {
    pub engine: Engine,
    pub entries: Vec<HistoryEntry>,
    pub input: String,
    pub scroll_to_bottom: bool,

    /// All submitted lines, in order (persisted across restarts).
    cmd_history: Vec<String>,
    /// Current position while navigating with Up/Down; `None` = not navigating.
    nav_index: Option<usize>,
    /// The in-progress input stashed away when navigation started.
    nav_stash: String,

    /// Cache for the live preview: (input it was computed for, result).
    preview_cache: Option<(String, Option<(Markup, String)>)>,
    /// The most recent *valid* preview, kept while typing so the preview
    /// doesn't flicker away every time the input is momentarily incomplete.
    last_good_preview: Option<(Markup, String)>,
}

/// A live preview of the current input. `fresh` is false when the shown
/// value belongs to an earlier (valid) version of the input.
pub struct Preview {
    pub markup: Markup,
    pub plain: String,
    pub fresh: bool,
}

impl Session {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            entries: Vec::new(),
            input: String::new(),
            scroll_to_bottom: false,
            cmd_history: Vec::new(),
            nav_index: None,
            nav_stash: String::new(),
            preview_cache: None,
            last_good_preview: None,
        }
    }

    /// Evaluates the current input line and appends the result to the session.
    pub fn submit(&mut self) {
        let line = self.input.trim().to_owned();
        self.input.clear();
        self.nav_index = None;
        self.preview_cache = None;
        self.last_good_preview = None;
        if line.is_empty() {
            return;
        }

        self.cmd_history.push(line.clone());
        self.run_line(&line);
        self.scroll_to_bottom = true;
        self.persist_history();
    }

    /// Runs one line: either a REPL command or numbat code.
    fn run_line(&mut self, line: &str) {
        let mut parts = line.split_whitespace();
        let command = parts.next().unwrap_or_default();
        let argument = parts.next().unwrap_or_default();

        match (command, argument) {
            ("clear", "") => {
                self.entries.clear();
            }
            ("reset", "") => {
                self.engine.reset();
                self.entries.clear();
            }
            ("list", "") | ("ls", "") => {
                self.push_command_output(line, self.engine.environment_markup());
            }
            ("help", "") | ("?", "") => {
                self.push_command_output(line, numbat::help::basic_help_markup());
            }
            ("info", ident) if !ident.is_empty() => {
                let markup = self.engine.info_markup(ident);
                self.push_command_output(line, markup);
            }
            _ => {
                let output = self.engine.eval(line);
                self.entries.push(HistoryEntry {
                    input: line.to_owned(),
                    printed: output.printed,
                    result: output.result,
                    result_plain: output.result_plain,
                    error: output.error,
                });
            }
        }
    }

    fn push_command_output(&mut self, input: &str, markup: Markup) {
        self.entries.push(HistoryEntry {
            input: input.to_owned(),
            printed: vec![markup],
            result: None,
            result_plain: None,
            error: None,
        });
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.input.clear();
        self.nav_index = None;
        self.preview_cache = None;
        self.last_good_preview = None;
        self.scroll_to_bottom = true;
    }

    /// Deletes a single entry. The engine state (definitions) is unaffected.
    pub fn delete_entry(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    /// The most recent copyable result, if any.
    pub fn last_result_plain(&self) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| e.result_plain.as_deref())
    }

    // ---- Command history navigation -------------------------------------

    pub fn navigate_up(&mut self) -> bool {
        if self.cmd_history.is_empty() {
            return false;
        }
        let next = match self.nav_index {
            None => {
                self.nav_stash = self.input.clone();
                self.cmd_history.len() - 1
            }
            Some(0) => return false,
            Some(i) => i - 1,
        };
        self.nav_index = Some(next);
        self.input = self.cmd_history[next].clone();
        true
    }

    pub fn navigate_down(&mut self) -> bool {
        let Some(current) = self.nav_index else {
            return false;
        };
        if current + 1 < self.cmd_history.len() {
            self.nav_index = Some(current + 1);
            self.input = self.cmd_history[current + 1].clone();
        } else {
            self.nav_index = None;
            self.input = std::mem::take(&mut self.nav_stash);
        }
        true
    }

    /// Must be called whenever the user edits the input, so that history
    /// navigation restarts from the new text.
    pub fn on_input_edited(&mut self) {
        self.nav_index = None;
    }

    // ---- Live preview ----------------------------------------------------

    /// Returns the live preview for the current input, evaluating it at most
    /// once per distinct input string.
    ///
    /// While the input is momentarily invalid (mid-edit), the last valid
    /// preview is returned with `fresh: false` instead of nothing, so the
    /// UI doesn't flicker on every keystroke.
    pub fn preview(&mut self) -> Option<Preview> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() || is_repl_command(trimmed) {
            self.preview_cache = None;
            self.last_good_preview = None;
            return None;
        }

        let outdated = self
            .preview_cache
            .as_ref()
            .is_none_or(|(input, _)| input != &self.input);
        if outdated {
            let result = self.engine.preview(&self.input);
            if let Some(good) = &result {
                self.last_good_preview = Some(good.clone());
            }
            self.preview_cache = Some((self.input.clone(), result));
        }

        let current = self
            .preview_cache
            .as_ref()
            .and_then(|(_, result)| result.as_ref());
        match current {
            Some((markup, plain)) => Some(Preview {
                markup: markup.clone(),
                plain: plain.clone(),
                fresh: true,
            }),
            None => self
                .last_good_preview
                .as_ref()
                .map(|(markup, plain)| Preview {
                    markup: markup.clone(),
                    plain: plain.clone(),
                    fresh: false,
                }),
        }
    }

    // ---- Persistence -----------------------------------------------------

    fn history_path() -> Option<PathBuf> {
        dirs::data_dir().map(|dir| dir.join("numbat-ui").join("history.numbat"))
    }

    pub fn persist_history(&self) {
        // Unit tests must not touch the real history file.
        #[cfg(test)]
        return;

        // The debug screenshot harness submits demo lines; keep them out of
        // the real history file.
        #[cfg(debug_assertions)]
        if std::env::var("NUMBAT_UI_SHOT").is_ok() {
            return;
        }

        let Some(path) = Self::history_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let start = self.cmd_history.len().saturating_sub(MAX_PERSISTED_HISTORY);
        let contents = self.cmd_history[start..].join("\n");
        if let Err(e) = std::fs::write(&path, contents) {
            log::warn!("Failed to persist history: {e}");
        }
    }

    /// Replays persisted input lines to rebuild both the visible history and
    /// the engine state (variable/function definitions).
    pub fn restore_history(&mut self) {
        let lines = Self::load_history_lines();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            self.cmd_history.push(line.clone());
            self.run_line(&line);
        }
        self.scroll_to_bottom = true;
    }

    fn load_history_lines() -> Vec<String> {
        if let Some(path) = Self::history_path() {
            if let Ok(contents) = std::fs::read_to_string(&path) {
                return contents.lines().map(str::to_owned).collect();
            }
        }
        legacy_history_lines().unwrap_or_default()
    }
}

fn is_repl_command(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "clear" | "reset" | "list" | "ls" | "help" | "?" | "info"
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use numbat::FormatOptions;

    fn session() -> Session {
        Session::new(Engine::new(FormatOptions::default()))
    }

    #[test]
    fn submit_appends_entry() {
        let mut s = session();
        s.input = "1 + 1".to_owned();
        s.submit();
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].result_plain.as_deref(), Some("2"));
        assert!(s.input.is_empty());
    }

    #[test]
    fn clear_command_clears_view_but_keeps_definitions() {
        let mut s = session();
        s.input = "let y = 4".to_owned();
        s.submit();
        s.input = "clear".to_owned();
        s.submit();
        assert!(s.entries.is_empty());
        s.input = "y".to_owned();
        s.submit();
        assert_eq!(s.entries[0].result_plain.as_deref(), Some("4"));
    }

    #[test]
    fn reset_command_discards_definitions() {
        let mut s = session();
        s.input = "let z = 4".to_owned();
        s.submit();
        s.input = "reset".to_owned();
        s.submit();
        s.input = "z".to_owned();
        s.submit();
        assert!(s.entries[0].error.is_some());
    }

    #[test]
    fn history_navigation_round_trip() {
        let mut s = session();
        for line in ["1", "2", "3"] {
            s.input = line.to_owned();
            s.submit();
        }
        s.input = "draft".to_owned();
        assert!(s.navigate_up());
        assert_eq!(s.input, "3");
        assert!(s.navigate_up());
        assert_eq!(s.input, "2");
        assert!(s.navigate_down());
        assert_eq!(s.input, "3");
        assert!(s.navigate_down());
        assert_eq!(s.input, "draft");
        assert!(!s.navigate_down());
    }

    #[test]
    fn last_result_skips_errors() {
        let mut s = session();
        s.input = "6 * 7".to_owned();
        s.submit();
        s.input = "1 +".to_owned();
        s.submit();
        assert_eq!(s.last_result_plain(), Some("42"));
    }

    #[test]
    fn preview_is_cached_per_input() {
        let mut s = session();
        s.input = "2^10".to_owned();
        let preview = s.preview().unwrap();
        assert_eq!(preview.plain, "1024");
        assert!(preview.fresh);
        // Same input: served from cache (no way to observe directly, but
        // must return the same value).
        assert_eq!(s.preview().unwrap().plain, "1024");
    }

    #[test]
    fn preview_sticks_while_input_is_invalid() {
        let mut s = session();
        s.input = "2^10".to_owned();
        assert!(s.preview().unwrap().fresh);
        // Continue typing: momentarily invalid input keeps the last value.
        s.input = "2^10 +".to_owned();
        let stale = s.preview().unwrap();
        assert_eq!(stale.plain, "1024");
        assert!(!stale.fresh);
        // Empty input drops the sticky preview.
        s.input.clear();
        assert!(s.preview().is_none());
    }
}
