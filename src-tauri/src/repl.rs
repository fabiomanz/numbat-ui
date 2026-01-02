use anyhow::{bail, Result};
use colored::Colorize;
use numbat::command::{CommandControlFlow, CommandRunner};
use numbat::compact_str::{CompactString, ToCompactString};
use numbat::diagnostic::ResolverDiagnostic;
use numbat::markup::{self as m, FormatType, FormattedString, Formatter, Markup};
use numbat::module_importer::{BuiltinModuleImporter, ChainedImporter, FileSystemImporter};
use numbat::resolver::CodeSource;
use numbat::session_history::SessionHistory;
use numbat::{Context, InterpreterSettings, NumbatError};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Completer, Editor, Helper, Highlighter, Hinter, Validator};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{fs, thread};

const NUMBAT_VERSION: &str = env!("NUMBAT_VERSION");

pub struct ANSIFormatter;

impl Formatter for ANSIFormatter {
    fn format_part(
        &self,
        FormattedString(_output_type, format_type, text): &FormattedString,
    ) -> CompactString {
        (match format_type {
            FormatType::Whitespace => text.normal(),
            FormatType::Emphasized => text.bold(),
            FormatType::Dimmed => text.dimmed(),
            FormatType::Text => text.normal(),
            FormatType::String => text.green(),
            FormatType::Keyword => text.magenta(),
            FormatType::Value => text.yellow(),
            FormatType::Unit => text.cyan(),
            FormatType::Identifier => text.normal(),
            FormatType::TypeIdentifier => text.blue().italic(),
            FormatType::Operator => text.bold(),
            FormatType::Decorator => text.green(),
        })
        .to_compact_string()
    }
}

pub fn ansi_format(m: &Markup, indent: bool) -> CompactString {
    ANSIFormatter {}.format(m, indent)
}

#[derive(Completer, Helper, Hinter, Validator, Highlighter)]
struct NumbatHelper {
    // Minimal helper, we can expand later
}

struct Cli {
    context: Arc<Mutex<Context>>,
}

impl Cli {
    fn make_fresh_context() -> Context {
        let fs_importer = FileSystemImporter::default();
        // Add default module paths if needed, simplified for embedded
        let importer = ChainedImporter::new(
            Box::new(fs_importer),
            Box::<BuiltinModuleImporter>::default(),
        );

        let mut context = Context::new(importer);
        context.set_terminal_width(
            terminal_size::terminal_size().map(|(terminal_size::Width(w), _)| w as usize),
        );
        context
    }

    fn new() -> Result<Self> {
        let context = Self::make_fresh_context();
        // Prelude load is now deferred to run()

        Ok(Self {
            context: Arc::new(Mutex::new(context)),
        })
    }

    fn run(&mut self) -> Result<()> {
        #[cfg(windows)]
        let _ = colored::control::set_virtual_terminal(true);

        colored::control::set_override(true);

        // Load prelude and currency module in background
        // This allows the prompt to appear immediately.
        // If the user types a command before this finishes, the main thread
        // will block on the context lock, which is the desired behavior.
        let context_clone = self.context.clone();
        thread::spawn(move || {
            let mut ctx = context_clone.lock().unwrap();
            let _ = ctx.interpret("use prelude", CodeSource::Internal);
            let _ = ctx.interpret("use units::currencies", CodeSource::Internal);
        });

        self.repl()
    }

    fn repl(&mut self) -> Result<()> {
        let history_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("numbat")
            .join("history");

        if let Some(parent) = history_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let mut rl = Editor::<NumbatHelper, DefaultHistory>::new()?;
        rl.set_helper(Some(NumbatHelper {}));
        rl.load_history(&history_path).ok();

        println!();
        println!("  █▄░█ █░█ █▀▄▀█ █▄▄ ▄▀█ ▀█▀    Numbat {}", NUMBAT_VERSION);
        println!("  █░▀█ █▄█ █░▀░█ █▄█ █▀█ ░█░    github.com/fabiomanz/numbat-ui");
        println!();

        let result = self.repl_loop(&mut rl);
        rl.save_history(&history_path).ok();

        result
    }

    fn repl_loop(&mut self, rl: &mut Editor<NumbatHelper, DefaultHistory>) -> Result<()> {
        let mut cmd_runner = CommandRunner::<Editor<NumbatHelper, DefaultHistory>>::new()
            .print_with(|m| println!("{}", ansi_format(m, true)))
            .enable_clear(|rl| match rl.clear_screen() {
                Ok(_) => CommandControlFlow::Continue,
                Err(_) => CommandControlFlow::Return,
            })
            .enable_save(SessionHistory::default())
            .enable_reset()
            .enable_quit();

        loop {
            let readline = rl.readline(">>> ");
            match readline {
                Ok(line) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    rl.add_history_entry(&line)?;

                    let mut ctx = self.context.lock().unwrap();
                    match cmd_runner.try_run_command(&line, &mut ctx, rl) {
                        Ok(cf) => match cf {
                            CommandControlFlow::Continue => continue,
                            CommandControlFlow::Return => return Ok(()),
                            CommandControlFlow::NotACommand => {}
                            CommandControlFlow::Reset => {
                                *ctx = Self::make_fresh_context();
                                continue;
                            }
                        },
                        Err(err) => {
                            ctx.print_diagnostic(
                                ResolverDiagnostic {
                                    error: &*err,
                                    resolver: ctx.resolver(),
                                },
                                true,
                            );
                            continue;
                        }
                    }
                    drop(ctx);

                    // Parse and evaluate
                    let to_be_printed: Arc<Mutex<Vec<m::Markup>>> = Arc::new(Mutex::new(vec![]));
                    let to_be_printed_c = to_be_printed.clone();
                    let mut settings = InterpreterSettings {
                        print_fn: Box::new(move |s: &m::Markup| {
                            to_be_printed_c.lock().unwrap().push(s.clone());
                        }),
                    };

                    let interpretation_result = self
                        .context
                        .lock()
                        .unwrap()
                        .interpret_with_settings(&mut settings, &line, CodeSource::Text);

                    match interpretation_result {
                        Ok((statements, interpreter_result)) => {
                            let to_be_printed = to_be_printed.lock().unwrap();
                            for s in to_be_printed.iter() {
                                println!("{}", ansi_format(s, true));
                            }

                            let ctx = self.context.lock().unwrap();
                            let registry = ctx.dimension_registry();
                            let result_markup = interpreter_result.to_markup(
                                statements.last(),
                                registry,
                                true, // interactive
                                true, // pretty_print
                            );
                            print!("{}", ansi_format(&result_markup, false));
                            if interpreter_result.is_value() {
                                println!();
                            }
                        }
                        Err(e) => {
                            let ctx = self.context.lock().unwrap();
                            match *e {
                                NumbatError::ResolverError(e) => ctx.print_diagnostic(e, true),
                                NumbatError::NameResolutionError(e) => {
                                    ctx.print_diagnostic(e, true)
                                }
                                NumbatError::TypeCheckError(e) => ctx.print_diagnostic(e, true),
                                NumbatError::RuntimeError(e) => ctx.print_diagnostic(
                                    ResolverDiagnostic {
                                        error: &e,
                                        resolver: ctx.resolver(),
                                    },
                                    true,
                                ),
                            }
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {}
                Err(ReadlineError::Eof) => {
                    return Ok(());
                }
                Err(err) => {
                    bail!(err);
                }
            }
        }
    }
}

pub fn run() -> Result<()> {
    #[cfg(windows)]
    let _ = colored::control::set_virtual_terminal(true);

    if let Err(e) = Cli::new().and_then(|mut cli| cli.run()) {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
    Ok(())
}
