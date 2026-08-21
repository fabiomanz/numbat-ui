//! Launch-at-login registration (macOS Launch Agent, Windows registry,
//! Linux XDG autostart). The registered command passes `--hidden` so the
//! app starts in the background with only the hotkey active.

use auto_launch::{AutoLaunchBuilder, MacOSLaunchMode};

fn launcher() -> Result<auto_launch::AutoLaunch, String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("Could not determine the app path: {e}"))?;
    AutoLaunchBuilder::new()
        .set_app_name("Numbat UI")
        .set_app_path(&exe.to_string_lossy())
        .set_args(&["--hidden"])
        .set_macos_launch_mode(MacOSLaunchMode::LaunchAgent)
        .build()
        .map_err(|e| format!("Could not set up launch at login: {e}"))
}

pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let launcher = launcher()?;
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| {
        let verb = if enabled { "enable" } else { "disable" };
        format!("Could not {verb} launch at login: {e}")
    })
}

/// Re-registers the launch agent on startup if the config wants it, so the
/// entry stays valid when the app is moved or updated.
pub fn reconcile(wanted: bool) {
    if !wanted {
        return;
    }
    // A bare binary (e.g. a dev build run from the repo) must not hijack a
    // registration that points at the installed .app bundle.
    #[cfg(target_os = "macos")]
    if !std::env::current_exe()
        .unwrap_or_default()
        .to_string_lossy()
        .contains(".app/Contents/MacOS/")
    {
        return;
    }
    if let Err(e) = set_enabled(true) {
        log::warn!("{e}");
    }
}
