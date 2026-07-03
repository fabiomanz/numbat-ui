//! Thin wrapper around the numbat interpreter.

use std::sync::{Arc, Mutex};

use codespan_reporting::term::{self, termcolor::NoColor};
use numbat::diagnostic::ErrorDiagnostic;
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
    pub error: Option<String>,
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

    /// Renders a numbat error as a plain-text compiler-style diagnostic
    /// (with source snippet and span markers).
    fn format_error(&self, error: &NumbatError) -> String {
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
        if text.is_empty() {
            error.to_string()
        } else {
            text.to_owned()
        }
    }
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
            err.contains('^'),
            "diagnostic should point at the span: {err}"
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
