//! Resolved application configuration. Defaults are produced here so any
//! crate can synthesize a working `Config` in tests without going through
//! `mx-config`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::keymap::Keymap;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncatePath {
    Start,
    Middle,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub theme: String,
    pub show_hidden: bool,
    pub panel_ratio: u8,
    pub truncate_path: TruncatePath,
    pub date_format: String,
    pub columns: UiColumns,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "classic".into(),
            show_hidden: false,
            panel_ratio: 50,
            truncate_path: TruncatePath::Middle,
            date_format: "%Y-%m-%d %H:%M".into(),
            columns: UiColumns::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiColumns {
    pub size: bool,
    pub modified: bool,
}
impl Default for UiColumns {
    fn default() -> Self {
        Self {
            size: true,
            modified: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    pub chord_timeout_ms: u64,
    pub double_click_ms: u64,
}
impl Default for InputConfig {
    fn default() -> Self {
        Self {
            chord_timeout_ms: 1500,
            double_click_ms: 250,
        }
    }
}
impl InputConfig {
    #[must_use]
    pub fn chord_timeout(&self) -> Duration {
        Duration::from_millis(self.chord_timeout_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpsConfig {
    pub copy_buffer_kib: u64,
    pub preserve_mtime: bool,
    pub preserve_mode: bool,
    pub follow_symlinks: bool,
    pub max_concurrent_workers: u32,
}
impl Default for OpsConfig {
    fn default() -> Self {
        Self {
            copy_buffer_kib: 1024,
            preserve_mtime: true,
            preserve_mode: true,
            follow_symlinks: false,
            max_concurrent_workers: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub file: String, // "auto", "off", or absolute path
}
impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Warn,
            file: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub ui: UiConfig,
    pub input: InputConfig,
    pub ops: OpsConfig,
    pub keymap: Keymap,
    pub theme: Theme,
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui: UiConfig::default(),
            input: InputConfig::default(),
            ops: OpsConfig::default(),
            keymap: Keymap::defaults(),
            theme: Theme::CLASSIC,
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_classic_theme_and_full_keymap() {
        use crate::command::CommandId;
        use crate::input::{KeyChord, KeyCode};
        use crate::keymap::Lookup;

        let c = Config::default();
        assert_eq!(c.theme, Theme::CLASSIC);
        // F5 must be Copy in defaults.
        let f5 = vec![KeyChord::bare(KeyCode::F(5))];
        assert_eq!(c.keymap.lookup(&f5), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn input_chord_timeout_default() {
        assert_eq!(
            InputConfig::default().chord_timeout(),
            std::time::Duration::from_millis(1500),
        );
    }
}
