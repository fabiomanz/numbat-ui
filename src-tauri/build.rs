use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_triple = env::var("TARGET").unwrap();
    let ext = if target_os == "windows" { ".exe" } else { "" };
    let binary_name = format!("numbat-{}{}", target_triple, ext);
    let out_dir = Path::new("binaries");
    let dest_path = out_dir.join(&binary_name);

    if !dest_path.exists() {
        println!("cargo:warning=Numbat binary not found. Compiling numbat-cli...");

        // Create binaries dir if not exists
        std::fs::create_dir_all(out_dir).unwrap();

        // Build numbat-cli
        // We use a custom target directory to avoid polluting the global cargo cache or project target
        let status = Command::new("cargo")
            .args(&[
                "install",
                "numbat-cli",
                "--locked",
                "--root",
                "target/numbat-build",
            ])
            .status()
            .expect("Failed to run cargo install numbat-cli");

        if !status.success() {
            panic!("Failed to install numbat-cli");
        }

        // Move and rename binary
        let compiled_name = format!("numbat{}", ext);
        let src_path = Path::new("target/numbat-build/bin").join(&compiled_name);
        std::fs::rename(&src_path, &dest_path).expect("Failed to move numbat binary");

        println!(
            "cargo:warning=Numbat binary compiled and moved to {:?}",
            dest_path
        );
    }

    tauri_build::build()
}
