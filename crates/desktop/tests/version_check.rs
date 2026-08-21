use std::process::Command;

#[test]
fn test_numbat_version() {
    // 1. The version of numbat we build against, exported by build.rs from
    //    the workspace manifest.
    let current_version = env!("NUMBAT_VERSION");

    println!("Current local numbat version: {}", current_version);

    // 2. Get the latest version from crates.io using the API (cargo search is flaky in CI)
    let output = Command::new("curl")
        .arg("-s")
        .arg("https://crates.io/api/v1/crates/numbat")
        .arg("-H")
        .arg("User-Agent: numbat-ui-test (github.com/numbat/numbat-ui)")
        .output()
        .expect("Failed to execute curl");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("curl failed with status {}: {}", output.status, stderr);
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse JSON safely
    let json: serde_json::Value = serde_json::from_str(&output_str).expect(&format!(
        "Failed to parse JSON from crates.io response: {}",
        output_str
    ));

    let latest_version = json["crate"]["max_version"]
        .as_str()
        .expect("Failed to find crate.max_version in crates.io response");

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
