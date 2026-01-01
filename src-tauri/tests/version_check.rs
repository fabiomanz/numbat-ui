use std::fs;
use std::process::Command;
use toml::Value;

#[test]
fn test_numbat_version() {
    // 1. Get the version of numbat from Cargo.toml
    let cargo_toml_content = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Value =
        toml::from_str(&cargo_toml_content).expect("Failed to parse Cargo.toml");

    let current_version = cargo_toml["dependencies"]["numbat"]
        .as_str()
        .or_else(|| cargo_toml["dependencies"]["numbat"]["version"].as_str())
        .expect("Failed to find numbat dependency version");

    println!("Current local numbat version: {}", current_version);

    // 2. Get the latest version from crates.io using cargo search
    let output = Command::new("cargo")
        .arg("search")
        .arg("numbat")
        .output()
        .expect("Failed to execute cargo search");

    let output_str = String::from_utf8_lossy(&output.stdout);
    // cargo search output format: numbat = "1.17.0"    # ...

    // Simple parsing to extract the version
    let latest_version_line = output_str
        .lines()
        .find(|line| line.starts_with("numbat = "))
        .expect("Failed to find numbat in cargo search output");
    let parts: Vec<&str> = latest_version_line.split('"').collect();
    let latest_version = parts
        .get(1)
        .expect("Failed to parse version from cargo search output");

    println!("Latest numbat version on crates.io: {}", latest_version);

    // 3. Compare semantically-ish
    // If current_version is "1.17", treat it as "1.17.0" for comparison with "1.17.0"
    let mut current_version_normalized = current_version.to_string();
    if current_version.split('.').count() == 2 {
        current_version_normalized.push_str(".0");
    }

    assert_eq!(
        current_version_normalized, *latest_version,
        "Numbat version is outdated! Upgrade to {}",
        latest_version
    );
}
