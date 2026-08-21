use std::fs;
use std::path::{Path, PathBuf};
use toml::Table;

fn main() {
    export_numbat_version();

    #[cfg(windows)]
    embed_windows_icon();
}

/// Makes the version of the bundled numbat crate available as `NUMBAT_VERSION`
/// (to this crate's code *and* to its tests).
fn export_numbat_version() {
    let manifest = workspace_manifest();
    println!("cargo:rerun-if-changed={}", manifest.display());

    let cargo_toml = fs::read_to_string(&manifest).expect("Failed to read the workspace Cargo.toml");
    let cargo_toml: Table = cargo_toml.parse().expect("Failed to parse Cargo.toml");

    // The version is declared once, in `[workspace.dependencies]`.
    let version = cargo_toml
        .get("workspace")
        .and_then(|w| w.as_table())
        .and_then(|w| w.get("dependencies"))
        .and_then(|d| d.as_table())
        .and_then(|deps| deps.get("numbat"))
        .and_then(|numbat| {
            numbat.as_str().map(str::to_owned).or_else(|| {
                numbat
                    .as_table()
                    .and_then(|t| t.get("version"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "unknown".to_owned());

    println!("cargo:rustc-env=NUMBAT_VERSION={version}");
}

/// Walks up from this package to the Cargo.toml that declares `[workspace]`.
fn workspace_manifest() -> PathBuf {
    let mut dir: &Path = &PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("Cargo.toml");
        if fs::read_to_string(&candidate).is_ok_and(|c| c.contains("[workspace]")) {
            return candidate;
        }
        dir = dir.parent().expect("No workspace Cargo.toml above this crate");
    }
}

/// Embeds the application icon into the Windows executable so that
/// Explorer and the taskbar show the correct icon.
#[cfg(windows)]
fn embed_windows_icon() {
    let mut res = winresource::WindowsResource::new();
    res.set_icon("src/icons/icon.ico");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Failed to embed Windows icon: {e}");
    }
}
