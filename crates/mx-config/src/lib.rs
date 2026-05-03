//! Midnight X config: parses TOML into types defined in [`mx-core`] and reports
//! warnings for partially-bad input without ever panicking.
//!
//! [`mx-core`]: ../mx_core/index.html

#![forbid(unsafe_code)]

pub mod keymap_str;

/// Canonical default-config TOML. Same string is emitted by
/// `mx --print-default-config`.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default.toml");

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::config::{Config, UiConfig};

    /// The embedded default TOML must deserialize into the same `UiConfig` /
    /// etc. that `mx-core` produces with `Default::default()`.
    #[test]
    fn embedded_default_round_trip() {
        // We can deserialize each [section] separately because Config is a
        // composition of Deserialize sub-structs.
        #[derive(serde::Deserialize)]
        struct Doc {
            ui:      UiConfig,
            input:   mx_core::config::InputConfig,
            ops:     mx_core::config::OpsConfig,
            logging: mx_core::config::LoggingConfig,
        }
        let parsed: Doc = toml::from_str(DEFAULT_CONFIG_TOML)
            .expect("default.toml must parse");
        let c = Config::default();
        assert_eq!(parsed.ui, c.ui);
        assert_eq!(parsed.input, c.input);
        assert_eq!(parsed.ops, c.ops);
        assert_eq!(parsed.logging, c.logging);
    }
}
