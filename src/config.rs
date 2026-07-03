//! Application configuration, stored in its own TOML file so that the
//! numbat CLI's `config.toml` is never touched.
//!
//! On first launch, formatting options are migrated from the numbat CLI
//! config (if one exists), so existing setups keep their formatting.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
pub const DEFAULT_QUICK_PANEL_HOTKEY: &str = "Alt+Space";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_QUICK_PANEL_HOTKEY: &str = "Ctrl+Alt+Space";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct FormattingConfig {
    /// Character used to separate groups of digits (e.g. "_", ",", " ", or "" to disable).
    #[serde(default = "default_digit_separator")]
    pub digit_separator: String,
    /// Minimum number of digits before separators are applied.
    #[serde(default = "default_digit_grouping_threshold")]
    pub digit_grouping_threshold: usize,
    /// Maximum number of significant digits displayed.
    #[serde(default = "default_significant_digits")]
    pub significant_digits: usize,
}

fn default_digit_separator() -> String {
    "_".to_owned()
}

fn default_digit_grouping_threshold() -> usize {
    6
}

fn default_significant_digits() -> usize {
    6
}

impl Default for FormattingConfig {
    fn default() -> Self {
        Self {
            digit_separator: default_digit_separator(),
            digit_grouping_threshold: default_digit_grouping_threshold(),
            significant_digits: default_significant_digits(),
        }
    }
}

impl FormattingConfig {
    pub fn format_options(&self) -> numbat::FormatOptions {
        numbat::FormatOptions {
            digit_separator: self.digit_separator.clone(),
            digit_grouping_threshold: self.digit_grouping_threshold,
            significant_digits: self.significant_digits,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeChoice {
    System,
    Dark,
    Light,
}

impl ThemeChoice {
    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct UiConfig {
    #[serde(default = "default_theme")]
    pub theme: ThemeChoice,
    /// Global hotkey that summons the quick panel, e.g. "Alt+Space".
    #[serde(default = "default_quick_panel_hotkey")]
    pub quick_panel_hotkey: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    /// Start the app (hidden) when logging in, so the hotkey always works.
    #[serde(default)]
    pub launch_at_login: bool,
}

fn default_theme() -> ThemeChoice {
    ThemeChoice::System
}

fn default_quick_panel_hotkey() -> String {
    DEFAULT_QUICK_PANEL_HOTKEY.to_owned()
}

fn default_font_size() -> f32 {
    14.0
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            quick_panel_hotkey: default_quick_panel_hotkey(),
            font_size: default_font_size(),
            launch_at_login: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    #[serde(default)]
    pub formatting: FormattingConfig,
    #[serde(default)]
    pub ui: UiConfig,
}

impl AppConfig {
    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("numbat-ui").join("config.toml"))
    }

    /// Path of the numbat CLI config, used once for migration of formatting options.
    fn numbat_cli_config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("numbat").join("config.toml"))
    }

    /// Loads the configuration, creating it (with migrated formatting
    /// options from the numbat CLI config, if present) on first launch.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        if let Ok(content) = std::fs::read_to_string(&path) {
            match toml::from_str::<Self>(&content) {
                Ok(config) => return config,
                Err(e) => {
                    log::warn!("Failed to parse {}: {e}; using defaults", path.display());
                    return Self::default();
                }
            }
        }

        // First launch: migrate formatting options from the numbat CLI config.
        let mut config = Self::default();
        if let Some(cli_path) = Self::numbat_cli_config_path() {
            if let Ok(content) = std::fs::read_to_string(cli_path) {
                if let Ok(migrated) = toml::from_str::<Self>(&content) {
                    config.formatting = migrated.formatting;
                }
            }
        }

        if let Err(e) = config.save() {
            log::warn!("Failed to write initial config: {e}");
        }
        config
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path().ok_or("Could not locate the system config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {e}"))?;
        }
        let toml_str =
            toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize config: {e}"))?;
        std::fs::write(&path, toml_str).map_err(|e| format!("Failed to write config file: {e}"))
    }

    pub fn format_options(&self) -> numbat::FormatOptions {
        self.formatting.format_options()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn partial_config_uses_defaults() {
        let parsed: AppConfig = toml::from_str(
            r#"
                [formatting]
                significant-digits = 8
            "#,
        )
        .unwrap();
        assert_eq!(parsed.formatting.significant_digits, 8);
        assert_eq!(parsed.formatting.digit_separator, "_");
        assert_eq!(parsed.formatting.digit_grouping_threshold, 6);
        assert_eq!(parsed.ui.theme, ThemeChoice::System);
    }

    #[test]
    fn legacy_numbat_cli_config_parses() {
        // The migration path parses the numbat CLI config with our own type;
        // unknown sections/keys must be tolerated.
        let parsed: AppConfig = toml::from_str(
            r#"
                intro-banner = "short"

                [formatting]
                digit-separator = ","

                [exchange-rates]
                fetching-policy = "on-startup"
            "#,
        )
        .unwrap();
        assert_eq!(parsed.formatting.digit_separator, ",");
    }

    #[test]
    fn invalid_type_is_an_error() {
        assert!(toml::from_str::<AppConfig>(
            r#"
                [formatting]
                digit-separator = 123
            "#,
        )
        .is_err());
    }
}
