//! Midnight X config: parses TOML into types defined in [`mx-core`] and reports
//! warnings for partially-bad input without ever panicking.
//!
//! [`mx-core`]: ../mx_core/index.html

#![forbid(unsafe_code)]

pub mod keymap_str;

use std::fs;
use std::path::Path;

use mx_core::command::CommandId;
use mx_core::config::{Config, InputConfig, LoggingConfig, OpsConfig, UiConfig};
use mx_core::errors::ConfigWarning;
use mx_core::keymap::Keymap;
use mx_core::theme::Theme;

use serde::Deserialize;
use thiserror::Error;

/// Canonical default-config TOML. Same string is emitted by
/// `mx --print-default-config`.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default.toml");

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file unreadable: {0}")]
    Io(String),
    #[error("config file does not parse as TOML: {0}")]
    Parse(String),
}

/// Load a config file. `path = None` → defaults silently. A non-existent
/// default-location file is also treated as "use defaults silently."
///
/// **Never panics.** Per-field problems become `ConfigWarning`s so the user
/// can still launch the app with a partially-broken config.
///
/// # Errors
///
/// Returns `ConfigError::Io` when an explicit path was given but unreadable;
/// `ConfigError::Parse` when the TOML doesn't parse at all.
pub fn load(path: Option<&Path>) -> Result<(Config, Vec<ConfigWarning>), ConfigError> {
    let raw = match path {
        Some(p) => fs::read_to_string(p).map_err(|e| ConfigError::Io(e.to_string()))?,
        None => return Ok((Config::default(), Vec::new())),
    };
    parse_str(&raw)
}

/// Parse already-loaded TOML text. Useful for tests.
///
/// # Errors
///
/// Returns `ConfigError::Parse` when the input is not valid TOML.
pub fn parse_str(s: &str) -> Result<(Config, Vec<ConfigWarning>), ConfigError> {
    let doc: TopLevel = toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))?;
    Ok(resolve(doc))
}

#[derive(Debug, Default, Deserialize)]
struct TopLevel {
    ui:      Option<UiConfig>,
    input:   Option<InputConfig>,
    ops:     Option<OpsConfig>,
    logging: Option<LoggingConfig>,
    keymap:  Option<toml::Table>,
}

fn resolve(doc: TopLevel) -> (Config, Vec<ConfigWarning>) {
    let mut warnings = Vec::new();

    let mut ui = doc.ui.unwrap_or_default();
    if !(1..=99).contains(&ui.panel_ratio) {
        warnings.push(ConfigWarning::new(
            "ui.panel_ratio",
            format!("clamped {} → 50 (must be 1..=99)", ui.panel_ratio),
        ));
        ui.panel_ratio = 50;
    }

    let theme = Theme::named(&ui.theme).unwrap_or_else(|| {
        warnings.push(ConfigWarning::new(
            "ui.theme",
            format!("unknown theme \"{}\" — using \"classic\"", ui.theme),
        ));
        Theme::CLASSIC
    });

    let input = doc.input.unwrap_or_default();
    let ops = doc.ops.unwrap_or_default();
    let logging = doc.logging.unwrap_or_default();

    let mut keymap = Keymap::defaults();
    if let Some(table) = doc.keymap {
        for (key, value) in table {
            let seq = match keymap_str::parse_sequence(&key) {
                Ok(s) => s,
                Err(e) => {
                    warnings.push(ConfigWarning::new(
                        format!("keymap.\"{key}\""),
                        format!("ignored: {e}"),
                    ));
                    continue;
                }
            };
            let cmd_str = value.as_str().map(str::to_owned);
            match cmd_str.as_deref() {
                Some("<unbind>") => { keymap.unbind(&seq); }
                Some(s) => {
                    match toml::from_str::<CmdHolder>(&format!("v = \"{s}\"")) {
                        Ok(holder) => keymap.set(seq, holder.v),
                        Err(_) => warnings.push(ConfigWarning::new(
                            format!("keymap.\"{key}\""),
                            format!("unknown command \"{s}\""),
                        )),
                    }
                }
                None => warnings.push(ConfigWarning::new(
                    format!("keymap.\"{key}\""),
                    "value must be a string",
                )),
            }
        }
    }

    let config = Config { ui, input, ops, keymap, theme, logging };
    (config, warnings)
}

#[derive(Deserialize)]
struct CmdHolder { v: CommandId }

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::command::CommandId;
    use mx_core::input::{KeyChord, KeyCode};
    use mx_core::keymap::Lookup;

    #[test]
    fn embedded_default_yields_default_config() {
        let (c, warnings) = parse_str(DEFAULT_CONFIG_TOML).unwrap();
        assert!(warnings.is_empty(), "default config must yield no warnings: {warnings:?}");
        assert_eq!(c.ui, UiConfig::default());
        assert_eq!(c.theme, Theme::CLASSIC);
    }

    #[test]
    fn unknown_theme_warns_and_falls_back() {
        let toml = r#"[ui]
theme = "midnight"
"#;
        let (c, warnings) = parse_str(toml).unwrap();
        assert_eq!(c.theme, Theme::CLASSIC);
        assert!(warnings.iter().any(|w| w.key == "ui.theme"));
    }

    #[test]
    fn out_of_range_ratio_clamps_and_warns() {
        let toml = "[ui]\npanel_ratio = 200\n";
        let (c, warnings) = parse_str(toml).unwrap();
        assert_eq!(c.ui.panel_ratio, 50);
        assert!(warnings.iter().any(|w| w.key == "ui.panel_ratio"));
    }

    #[test]
    fn keymap_unbind_removes_default() {
        let toml = r#"[keymap]
"f5" = "<unbind>"
"#;
        let (c, _) = parse_str(toml).unwrap();
        let f5 = vec![KeyChord::bare(KeyCode::F(5))];
        assert_eq!(c.keymap.lookup(&f5), Lookup::NoMatch);
    }

    #[test]
    fn keymap_override_replaces_default() {
        let toml = r#"[keymap]
"ctrl-c" = "copy"
"#;
        let (c, warnings) = parse_str(toml).unwrap();
        assert!(warnings.is_empty());
        let chord = vec![KeyChord::new(
            KeyCode::Char('c'),
            mx_core::input::KeyModifiers::ctrl(),
        )];
        assert_eq!(c.keymap.lookup(&chord), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn unknown_command_warns_does_not_fail() {
        let toml = r#"[keymap]
"ctrl-c" = "explode_universe"
"#;
        let (_, warnings) = parse_str(toml).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("unknown command")));
    }

    #[test]
    fn malformed_toml_is_a_parse_error() {
        let bad = "this isn't = toml\n[oops";
        assert!(matches!(parse_str(bad), Err(ConfigError::Parse(_))));
    }
}
