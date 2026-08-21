//! Thin wrapper around the numbat interpreter.

use std::sync::{Arc, Mutex};

use codespan_reporting::diagnostic::Severity;
use codespan_reporting::term::{self, termcolor::NoColor};
use numbat::diagnostic::{Diagnostic, ErrorDiagnostic};
use numbat::markup::{plain_text_format, Markup};
use numbat::module_importer::{BuiltinModuleImporter, ChainedImporter, FileSystemImporter};
use numbat::resolver::CodeSource;
use numbat::{Context, FormatOptions, InterpreterSettings, NumbatError};

/// Everything produced by evaluating one line of input.
#[derive(Default)]
pub struct EvalOutput {
    /// Lines printed via `print(...)` statements.
    pub printed: Vec<Markup>,
    /// The value of the evaluated expression, if any (with type info).
    pub result: Option<Markup>,
    /// Plain-text version of the value, for the clipboard.
    pub result_plain: Option<String>,
    pub error: Option<EvalError>,
}

/// A failed evaluation, in two renderings.
#[derive(Clone)]
pub struct EvalError {
    /// The compiler-style diagnostic with source snippet and span markers,
    /// as shown in the main window.
    pub full: String,
    /// A condensed one-glance version for places with room for a line or
    /// two, such as the quick panel.
    pub summary: String,
}

pub struct Engine {
    context: Context,
    pub format_options: FormatOptions,
}

impl Engine {
    pub fn new(format_options: FormatOptions) -> Self {
        Self {
            context: fresh_context(),
            format_options,
        }
    }

    pub fn reset(&mut self) {
        self.context = fresh_context();
    }

    pub fn eval(&mut self, line: &str) -> EvalOutput {
        let printed = Arc::new(Mutex::new(Vec::new()));
        let printed_sink = Arc::clone(&printed);
        let mut settings = InterpreterSettings {
            print_fn: Box::new(move |markup: &Markup| {
                printed_sink.lock().unwrap().push(markup.clone());
            }),
        };

        let result = self
            .context
            .interpret_with_settings(&mut settings, line, CodeSource::Text);

        let mut output = EvalOutput {
            printed: printed.lock().unwrap().clone(),
            ..Default::default()
        };

        match result {
            Ok((statements, interpreter_result)) => {
                if interpreter_result.is_value() {
                    let registry = self.context.dimension_registry();
                    output.result = Some(interpreter_result.to_markup(
                        statements.last(),
                        registry,
                        true,
                        false,
                        &self.format_options,
                    ));
                    let plain = interpreter_result.to_markup(
                        statements.last(),
                        registry,
                        false,
                        false,
                        &self.format_options,
                    );
                    output.result_plain = Some(plain_text_format(&plain, false).trim().to_owned());
                }
            }
            Err(e) => output.error = Some(self.format_error(&e)),
        }

        output
    }

    /// Evaluates `line` on a throwaway copy of the context, so definitions,
    /// prints and other side effects are discarded. Used for the live
    /// result preview while typing. Returns `(display_markup, plain_text)`.
    pub fn preview(&self, line: &str) -> Option<(Markup, String)> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        let mut scratch = self.context.clone();
        let mut settings = InterpreterSettings {
            print_fn: Box::new(|_: &Markup| {}),
        };
        let (statements, result) = scratch
            .interpret_with_settings(&mut settings, line, CodeSource::Text)
            .ok()?;
        if !result.is_value() {
            return None;
        }

        let registry = scratch.dimension_registry();
        let markup = result.to_markup(
            statements.last(),
            registry,
            false,
            false,
            &self.format_options,
        );
        let plain = plain_text_format(&markup, false).trim().to_owned();
        Some((markup, plain))
    }

    pub fn completions(&self, word_part: &str) -> Vec<String> {
        if word_part.is_empty() {
            return Vec::new();
        }
        self.context.get_completions_for(word_part, true).collect()
    }

    /// Markup for the `list` command.
    pub fn environment_markup(&self) -> Markup {
        self.context.print_environment()
    }

    /// Markup for `info <keyword>`.
    pub fn info_markup(&mut self, keyword: &str) -> Markup {
        self.context.print_info_for_keyword(keyword)
    }

    /// Renders a numbat error both as a plain-text compiler-style diagnostic
    /// (with source snippet and span markers) and as a condensed summary.
    fn format_error(&self, error: &NumbatError) -> EvalError {
        let diagnostics = match error {
            NumbatError::ResolverError(e) => e.diagnostics(),
            NumbatError::NameResolutionError(e) => e.diagnostics(),
            NumbatError::TypeCheckError(e) => e.diagnostics(),
            NumbatError::RuntimeError(e) => numbat::diagnostic::ResolverDiagnostic {
                resolver: self.context.resolver(),
                error: e,
            }
            .diagnostics(),
        };

        let mut buffer = NoColor::new(Vec::new());
        let config = term::Config::default();
        let files = &self.context.resolver().files;
        for diagnostic in &diagnostics {
            let _ = term::emit(&mut buffer, &config, files, diagnostic);
        }

        let text = String::from_utf8_lossy(buffer.get_ref());
        let text = text.trim_end();
        let full = if text.is_empty() {
            error.to_string()
        } else {
            text.to_owned()
        };

        EvalError {
            full,
            summary: summarize(error, &diagnostics),
        }
    }
}

/// Condenses an error into a single line.
///
/// The diagnostic's own first line is a category ("error: while type
/// checking") and the rest is a source snippet the user is already looking
/// at, so neither carries the point. The error's `Display` does — "Unknown
/// identifier 'accos'." — and the diagnostic notes carry the follow-up hint
/// ("Did you mean 'acos'?"), which is often the most useful part of all.
fn summarize(error: &NumbatError, diagnostics: &[Diagnostic]) -> String {
    // Runtime errors trail a `help` diagnostic holding the backtrace, which
    // is only noise here.
    let errors = || {
        diagnostics
            .iter()
            .filter(|d| matches!(d.severity, Severity::Bug | Severity::Error))
    };

    let mut parts = Vec::new();
    let head = collapse(&error.to_string());
    if !head.is_empty() {
        parts.push(head);
    }
    for note in errors().flat_map(|d| &d.notes) {
        // Some errors put their whole `Display` in a note; don't repeat it.
        let note = collapse(note);
        if !note.is_empty() && !parts.iter().any(|part: &String| part.contains(&note)) {
            parts.push(note);
        }
    }
    if parts.is_empty() {
        // No error text at all: fall back to the label the caret points at.
        parts.extend(
            errors()
                .flat_map(|d| &d.labels)
                .map(|label| collapse(&label.message))
                .filter(|message| !message.is_empty()),
        );
    }
    parts.join("  ·  ")
}

/// Flattens multi-line error text onto one line, dropping blank lines and
/// the padding that lines columns up in the terminal — nothing can stay
/// aligned on a single line anyway.
fn collapse(text: &str) -> String {
    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("  ·  ")
}

fn fresh_context() -> Context {
    let importer = ChainedImporter::new(
        Box::new(FileSystemImporter::default()),
        Box::<BuiltinModuleImporter>::default(),
    );

    let mut context = Context::new(importer);
    // Load the prelude and currency units, like the numbat CLI does.
    let _ = context.interpret("use prelude", CodeSource::Internal);
    let _ = context.interpret("use units::currencies", CodeSource::Internal);
    context
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> Engine {
        Engine::new(FormatOptions::default())
    }

    #[test]
    fn evaluates_simple_expression() {
        let mut e = engine();
        let out = e.eval("2 + 3");
        assert!(out.error.is_none());
        assert_eq!(out.result_plain.as_deref(), Some("5"));
    }

    #[test]
    fn evaluates_unit_conversion() {
        let mut e = engine();
        let out = e.eval("2 m + 3 cm -> cm");
        assert!(out.error.is_none());
        assert_eq!(out.result_plain.as_deref(), Some("203 cm"));
    }

    #[test]
    fn definitions_persist_across_lines() {
        let mut e = engine();
        assert!(e.eval("let radius = 3 m").error.is_none());
        let out = e.eval("2 * radius");
        assert_eq!(out.result_plain.as_deref(), Some("6 m"));
    }

    #[test]
    fn preview_does_not_leak_definitions() {
        let e = engine();
        assert!(e.preview("let x = 17").is_none() || e.preview("x").is_none());
    }

    #[test]
    fn preview_returns_value() {
        let e = engine();
        let (_, plain) = e.preview("6 * 7").unwrap();
        assert_eq!(plain, "42");
    }

    #[test]
    fn error_contains_span_markers() {
        let mut e = engine();
        let out = e.eval("2 m + 3 s");
        let err = out.error.expect("expected a type error");
        assert!(
            err.full.contains('^'),
            "diagnostic should point at the span: {}",
            err.full
        );
    }

    #[test]
    fn error_summary_is_one_informative_line() {
        let mut e = engine();
        let err = e.eval("accos(1/2)").error.expect("expected a type error");
        // The first line of the full diagnostic is the useless part.
        assert_eq!(err.full.lines().next(), Some("error: while type checking"));
        assert!(!err.summary.contains('\n'));
        assert!(
            err.summary.contains("Unknown identifier 'accos'")
                && err.summary.contains("Did you mean 'acos'?"),
            "summary should carry the identifier and the suggestion: {}",
            err.summary
        );
    }

    #[test]
    fn error_summary_omits_the_runtime_backtrace() {
        let mut e = engine();
        let err = e.eval("2 / 0").error.expect("expected a runtime error");
        assert!(err.full.contains("Backtrace:"));
        assert_eq!(err.summary, "Division by zero");
    }

    #[test]
    fn error_summary_does_not_repeat_itself() {
        let mut e = engine();
        // This error puts its whole `Display` in a diagnostic note as well.
        let err = e.eval("1 m + 1").error.expect("expected a type error");
        assert!(!err.summary.is_empty());
        let fix = "Suggested fix:";
        assert_eq!(
            err.summary.matches(fix).count(),
            1,
            "summary repeats the note: {}",
            err.summary
        );
    }

    #[test]
    fn print_statements_are_captured() {
        let mut e = engine();
        let out = e.eval("print(\"hello\")");
        assert_eq!(out.printed.len(), 1);
        assert!(out.result.is_none());
    }

    #[test]
    fn completions_include_functions() {
        let e = engine();
        let completions = e.completions("sqr");
        assert!(completions.iter().any(|c| c.starts_with("sqrt(")));
    }
}
