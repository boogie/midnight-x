//! Display formatters used by the panel renderer.

use std::time::SystemTime;

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
}
