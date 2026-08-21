//! Number formatting options, shared by every front end so that a result
//! reads the same on the desktop and on the phone.

use serde::{Deserialize, Serialize};

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
