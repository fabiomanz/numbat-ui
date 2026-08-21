//! The calculator session shared by every front end: evaluated entries, the
//! current input line, command history with navigation, the live preview
//! cache, and history persistence.
//!
//! Where the history is *stored* is left to the platform via
//! [`HistoryStore`] — a file on the desktop, `localStorage` on the web.

use numbat::markup::Markup;

use crate::engine::{Engine, EvalError};

/// Maximum number of input lines persisted (and replayed on startup).
const MAX_PERSISTED_HISTORY: usize = 200;

/// Persistent storage for the submitted input lines. Only the lines are
/// kept; the visible history and all definitions are rebuilt by replaying
/// them through the engine on startup.
pub trait HistoryStore {
    fn load(&self) -> Vec<String>;
    fn save(&self, lines: &[String]);
}

/// A store that forgets everything, for tests and throwaway sessions.
pub struct NoHistory;

impl HistoryStore for NoHistory {
    fn load(&self) -> Vec<String> {
        Vec::new()
    }
    fn save(&self, _lines: &[String]) {}
}

pub struct HistoryEntry {
    pub input: String,
    pub printed: Vec<Markup>,
    pub result: Option<Markup>,
    pub result_plain: Option<String>,
    pub error: Option<EvalError>,
}

pub struct Session {
    pub engine: Engine,
    pub entries: Vec<HistoryEntry>,
    pub input: String,
    pub scroll_to_bottom: bool,

    /// All submitted lines, in order (persisted across restarts).
    cmd_history: Vec<String>,
    store: Box<dyn HistoryStore>,
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
    pub fn new(engine: Engine, store: Box<dyn HistoryStore>) -> Self {
        Self {
            engine,
            entries: Vec::new(),
            input: String::new(),
            scroll_to_bottom: false,
            cmd_history: Vec::new(),
            store,
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

    /// The submitted input lines, oldest first.
    pub fn command_history(&self) -> &[String] {
        &self.cmd_history
    }

    pub fn persist_history(&self) {
        let start = self.cmd_history.len().saturating_sub(MAX_PERSISTED_HISTORY);
        self.store.save(&self.cmd_history[start..]);
    }

    /// Replays persisted input lines to rebuild both the visible history and
    /// the engine state (variable/function definitions).
    pub fn restore_history(&mut self) {
        for line in self.store.load() {
            if line.trim().is_empty() {
                continue;
            }
            self.cmd_history.push(line.clone());
            self.run_line(&line);
        }
        self.scroll_to_bottom = true;
    }
}

fn is_repl_command(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or_default();
    matches!(
        first,
        "clear" | "reset" | "list" | "ls" | "help" | "?" | "info"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use numbat::FormatOptions;

    fn session() -> Session {
        Session::new(Engine::new(FormatOptions::default()), Box::new(NoHistory))
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
