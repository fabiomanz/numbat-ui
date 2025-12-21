use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_triple = env::var("TARGET").unwrap();
    let ext = if target_os == "windows" { ".exe" } else { "" };

    // We want to build the "numbat-repl" crate
    // The binary name will be "numbat-repl-TRIPLE"
    let binary_name = format!("numbat-repl-{}{}", target_triple, ext);
    let out_dir = Path::new("binaries");
    let dest_path = out_dir.join(&binary_name);

    if !dest_path.exists() {
        println!("cargo:warning=Numbat REPL binary not found. Compiling numbat-repl...");

        std::fs::create_dir_all(out_dir).unwrap();

        // Build the nested crate
        let status = Command::new("cargo")
            .args(&[
                "build",
                "--release",
                "--manifest-path",
                "../numbat-repl/Cargo.toml",
            ])
            .status()
            .expect("Failed to build numbat-repl");

        if !status.success() {
            panic!("Failed to build numbat-repl");
        }

        // Move binary
        // Note: cargo build --release puts it in numbat-repl/target/release/numbat-repl
        let src_path =
            Path::new("../numbat-repl/target/release").join(format!("numbat-repl{}", ext));
        std::fs::rename(&src_path, &dest_path).expect("Failed to move numbat-repl binary");

        println!(
            "cargo:warning=Numbat REPL binary compiled and moved to {:?}",
            dest_path
        );
    }

    tauri_build::build()
}
