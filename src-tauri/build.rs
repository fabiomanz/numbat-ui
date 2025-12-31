use std::fs;
use toml::Table;

fn main() {
    // Read Cargo.toml to get the numbat version
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Table = cargo_toml.parse().expect("Failed to parse Cargo.toml");

    if let Some(dependencies) = cargo_toml.get("dependencies").and_then(|d| d.as_table()) {
        if let Some(numbat) = dependencies.get("numbat") {
            let version = if let Some(v) = numbat.as_str() {
                v.to_string()
            } else if let Some(table) = numbat.as_table() {
                table
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                "unknown".to_string()
            };
            println!("cargo:rustc-env=NUMBAT_VERSION={}", version);
        }
    }

    tauri_build::build()
}
