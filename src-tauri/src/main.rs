#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod repl;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--repl".to_string()) {
        if let Err(e) = repl::run() {
            eprintln!("Error running REPL: {}", e);
            std::process::exit(1);
        }
    } else {
        numbat_ui_lib::run();
    }
}
