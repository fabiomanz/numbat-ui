use std::fs;
use toml::Table;

fn main() {
    export_numbat_version();

    #[cfg(windows)]
    embed_windows_icon();
}

/// Makes the version of the bundled numbat crate available as `NUMBAT_VERSION`.
fn export_numbat_version() {
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Table = cargo_toml.parse().expect("Failed to parse Cargo.toml");

    let version = cargo_toml
        .get("dependencies")
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
