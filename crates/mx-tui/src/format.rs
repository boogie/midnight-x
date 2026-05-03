//! Display formatters used by the panel renderer.

use std::time::SystemTime;

use camino::Utf8Path;
use chrono::{DateTime, Local};

/// Format an mtime per the user's `date_format`. Returns blanks when the
/// metadata wasn't available.
#[must_use]
pub fn format_mtime(t: Option<SystemTime>, fmt: &str) -> String {
    let Some(t) = t else {
        return "                ".to_string();
    };
    let dt: DateTime<Local> = t.into();
    dt.format(fmt).to_string()
}

/// Replace a leading `$HOME` prefix with `~` so titles read more compactly.
/// Falls back to the original path when `$HOME` isn't set or doesn't match.
#[must_use]
pub fn shorten_home(path: &Utf8Path) -> String {
    let Ok(home) = std::env::var("HOME") else {
        return path.as_str().to_string();
    };
    if home.is_empty() {
        return path.as_str().to_string();
    }
    let s = path.as_str();
    if s == home {
        return "~".to_string();
    }
    let with_slash = format!("{home}/");
    if let Some(rest) = s.strip_prefix(&with_slash) {
        format!("~/{rest}")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn none_renders_as_padding() {
        let s = format_mtime(None, "%Y-%m-%d %H:%M");
        assert_eq!(s.len(), 16);
        assert!(s.chars().all(|c| c == ' '));
    }

    #[test]
    fn known_epoch_round_trips() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_164_645);
        let s = format_mtime(Some(t), "%Y-%m-%d");
        assert!(s.starts_with("2024"));
    }

    // The HOME-substitution tests share process state via `std::env::set_var`,
    // which makes them order-sensitive when run in parallel. Serial them by
    // setting HOME inside each test before assertions.
    #[test]
    fn shorten_home_replaces_home_prefix() {
        // SAFETY-ish: tests within a single binary run in threads; setting
        // HOME may race other tests. Reset before assertions.
        std::env::set_var("HOME", "/Users/boogie");
        let p = camino::Utf8Path::new("/Users/boogie/Workspace/neon-commander");
        assert_eq!(super::shorten_home(p), "~/Workspace/neon-commander");
    }

    #[test]
    fn shorten_home_exact_home_yields_tilde() {
        std::env::set_var("HOME", "/Users/boogie");
        let p = camino::Utf8Path::new("/Users/boogie");
        assert_eq!(super::shorten_home(p), "~");
    }

    #[test]
    fn shorten_home_unrelated_path_unchanged() {
        std::env::set_var("HOME", "/Users/boogie");
        let p = camino::Utf8Path::new("/etc/hosts");
        assert_eq!(super::shorten_home(p), "/etc/hosts");
    }
}
