//! WebAssembly bindings for the Numbat UI core.
//!
//! The whole calculator — parser, type checker, units, formatting, history —
//! runs in the browser; JavaScript only draws the result. Values cross the
//! boundary as JSON strings, which keeps the interface small enough to read
//! in one sitting and avoids a serde/bindgen bridge crate.

use numbat::markup::{FormatType, FormattedString, Markup};
use numbat_ui_core::{Engine, FormattingConfig, HistoryEntry, HistoryStore, Session};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

const HISTORY_KEY: &str = "numbat-ui.history";

/// Guards the one-shot handover of the exchange rates to numbat.
static EXCHANGE_RATES: std::sync::Once = std::sync::Once::new();

/// A calculator session, mirroring the desktop app's: definitions persist
/// between lines, and the input history survives a reload.
#[wasm_bindgen]
pub struct NumbatSession {
    session: Session,
}

#[wasm_bindgen]
impl NumbatSession {
    /// `exchange_rates_json` is a Frankfurter `/latest` response (the ECB
    /// daily reference rates, base EUR), or `undefined` to leave currency
    /// units unavailable. Anything that does not parse counts as `undefined`.
    ///
    /// It has to be handed over *here*: numbat keeps the rates in a global
    /// that can only be filled once, and only before the first currency
    /// lookup — and building the context is already a currency lookup.
    /// Taking it as a constructor argument makes that order impossible to
    /// get wrong from JavaScript.
    #[wasm_bindgen(constructor)]
    pub fn new(exchange_rates_json: Option<String>) -> Self {
        console_error_panic_hook::set_once();
        if let Some(xml) = exchange_rates_json.as_deref().and_then(ecb_xml) {
            // A second session would panic on numbat's `OnceLock::set`.
            EXCHANGE_RATES.call_once(|| numbat::Context::set_exchange_rates(&xml));
        }
        Self {
            session: Session::new(
                Engine::new(FormattingConfig::default().format_options()),
                Box::new(LocalStorageHistory),
            ),
        }
    }

    /// Replays the persisted input lines, rebuilding both the visible history
    /// and every definition made in it.
    pub fn restore(&mut self) {
        self.session.restore_history();
    }

    /// Evaluates one line and appends it to the session.
    pub fn submit(&mut self, line: &str) {
        self.session.input = line.to_owned();
        self.session.submit();
    }

    /// Every entry in the session, as a JSON array. Cheap enough to re-read
    /// after each submit — the history is capped at a couple of hundred
    /// entries — and it keeps `clear` and `reset` correct for free.
    pub fn entries(&self) -> String {
        let entries: Vec<JsEntry> = self.session.entries.iter().map(JsEntry::from).collect();
        json(&entries)
    }

    /// The result of `line` without committing it: JSON `{html, plain, fresh}`,
    /// or `null` when there is nothing to show. `fresh: false` marks a value
    /// left over from an earlier, still-valid version of the input.
    pub fn preview(&mut self, line: &str) -> Option<String> {
        self.session.input = line.to_owned();
        self.session.preview().map(|preview| {
            json(&JsPreview {
                html: markup_html(&preview.markup),
                plain: preview.plain,
                fresh: preview.fresh,
            })
        })
    }

    /// Completions for a partially typed unit, function or variable name.
    pub fn completions(&self, word_part: &str) -> Vec<String> {
        self.session.engine.completions(word_part)
    }

    /// The submitted input lines, oldest first, for history navigation.
    pub fn history(&self) -> Vec<String> {
        self.session.command_history().to_vec()
    }

    /// The most recent copyable result.
    pub fn last_result(&self) -> Option<String> {
        self.session.last_result_plain().map(str::to_owned)
    }

    pub fn set_formatting(
        &mut self,
        digit_separator: String,
        digit_grouping_threshold: usize,
        significant_digits: usize,
    ) {
        self.session.engine.format_options = FormattingConfig {
            digit_separator,
            digit_grouping_threshold,
            significant_digits,
        }
        .format_options();
    }
}

// ---- JSON shapes -------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsEntry {
    input: String,
    printed: Vec<String>,
    result: Option<String>,
    result_plain: Option<String>,
    error: Option<JsError>,
}

#[derive(Serialize)]
struct JsError {
    full: String,
    summary: String,
}

#[derive(Serialize)]
struct JsPreview {
    html: String,
    plain: String,
    fresh: bool,
}

impl From<&HistoryEntry> for JsEntry {
    fn from(entry: &HistoryEntry) -> Self {
        Self {
            input: entry.input.clone(),
            printed: entry.printed.iter().map(markup_html).collect(),
            result: entry.result.as_ref().map(markup_html),
            result_plain: entry.result_plain.clone(),
            error: entry.error.as_ref().map(|e| JsError {
                full: e.full.clone(),
                summary: e.summary.clone(),
            }),
        }
    }
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("session data is always serializable")
}

// ---- Markup rendering --------------------------------------------------

/// Renders numbat markup as HTML, tagging each run with the class its
/// desktop counterpart paints a color for (see `theme.rs`). Runs that
/// inherit the normal text color get no wrapper at all.
fn markup_html(markup: &Markup) -> String {
    let mut html = String::new();
    for FormattedString(_, format_type, text) in &markup.0 {
        let class = match format_type {
            FormatType::Whitespace
            | FormatType::Text
            | FormatType::Identifier
            | FormatType::Emphasized => None,
            FormatType::Dimmed => Some("nb-dim"),
            FormatType::String => Some("nb-string"),
            FormatType::Keyword => Some("nb-keyword"),
            FormatType::Value => Some("nb-value"),
            FormatType::Unit => Some("nb-unit"),
            FormatType::TypeIdentifier => Some("nb-type"),
            FormatType::Operator => Some("nb-operator"),
            FormatType::Decorator => Some("nb-decorator"),
        };
        match class {
            Some(class) => {
                html.push_str("<span class=\"");
                html.push_str(class);
                html.push_str("\">");
                escape_into(text, &mut html);
                html.push_str("</span>");
            }
            None => escape_into(text, &mut html),
        }
    }
    // Markup from numbat often ends with a trailing newline, which would
    // show up as an empty last line.
    while html.ends_with('\n') {
        html.pop();
    }
    html
}

fn escape_into(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

// ---- Exchange rates ----------------------------------------------------

/// The part of a Frankfurter response that matters here.
#[derive(Deserialize)]
struct FrankfurterRates {
    base: String,
    rates: BTreeMap<String, f64>,
}

/// Rewrites Frankfurter's JSON as the ECB XML numbat insists on. The ECB
/// itself sends no CORS headers, so the browser cannot read its file; its
/// parser only looks at `<Cube currency=".." rate=".."/>` elements, so those
/// are all this writes.
fn ecb_xml(json: &str) -> Option<String> {
    let parsed: FrankfurterRates = serde_json::from_str(json).ok()?;
    // numbat's currency module defines every unit relative to the euro.
    if parsed.base != "EUR" {
        return None;
    }
    let mut xml = String::from("<Cube>");
    for (currency, rate) in &parsed.rates {
        // The text comes off the network; a currency code is all it may be.
        if currency.bytes().all(|b| b.is_ascii_uppercase()) && rate.is_finite() && *rate > 0.0 {
            xml.push_str(&format!(r#"<Cube currency="{currency}" rate="{rate}"/>"#));
        }
    }
    xml.push_str("</Cube>");
    Some(xml)
}

// ---- Persistence -------------------------------------------------------

/// History in `localStorage`. Every failure mode here (private browsing,
/// storage disabled, quota) is non-fatal: the session simply starts empty.
struct LocalStorageHistory;

impl LocalStorageHistory {
    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }
}

impl HistoryStore for LocalStorageHistory {
    fn load(&self) -> Vec<String> {
        let Some(stored) = Self::storage().and_then(|s| s.get_item(HISTORY_KEY).ok().flatten())
        else {
            return Vec::new();
        };
        stored.lines().map(str::to_owned).collect()
    }

    fn save(&self, lines: &[String]) {
        if let Some(storage) = Self::storage() {
            let _ = storage.set_item(HISTORY_KEY, &lines.join("\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frankfurter_json_becomes_ecb_cubes() {
        let xml = ecb_xml(
            r#"{"amount":1.0,"base":"EUR","date":"2026-10-15","rates":{"USD":1.1208,"JPY":177.06}}"#,
        );
        assert_eq!(
            xml.as_deref(),
            Some(
                r#"<Cube><Cube currency="JPY" rate="177.06"/><Cube currency="USD" rate="1.1208"/></Cube>"#
            )
        );
    }

    #[test]
    fn unusable_rates_are_refused() {
        assert_eq!(ecb_xml("<html>rate limited</html>"), None);
        assert_eq!(ecb_xml(r#"{"base":"USD","rates":{"EUR":0.89}}"#), None);
        assert_eq!(
            ecb_xml(r#"{"base":"EUR","rates":{"US\"D":1.1,"GBP":-1}}"#).as_deref(),
            Some("<Cube></Cube>")
        );
    }
}
