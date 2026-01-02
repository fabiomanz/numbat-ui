mod repl;

#[cfg(windows)]
use windows_sys::Win32::System::Console::FreeConsole;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.contains(&"--repl".to_string()) {
        if let Err(e) = repl::run() {
            eprintln!("Error running REPL: {}", e);
            std::process::exit(1);
        }
    } else {
        #[cfg(windows)]
        unsafe {
            FreeConsole();
        }
        numbat_ui_lib::run();
    }
}
