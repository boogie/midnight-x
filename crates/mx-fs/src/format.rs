//! Formatters for human-readable display of file metadata.

/// Format a byte count using binary units (`K = 1024`). Output width is
/// always ≤ 7 characters: `"   42"`, `"1.2 K"`, `" 999 K"`, `"  1.5 M"`,
/// `"  9.4 G"`, `" 12.0 T"`. `None` renders as 4 spaces.
const UNITS: [&str; 5] = ["K", "M", "G", "T", "P"];

#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else {
        return "    ".to_string();
    };
    if b < 1024 {
        return format!("{b:>7}");
    }
    let mut value = b as f64 / 1024.0;
    let mut unit = UNITS[0];
    for u in UNITS.iter().skip(1) {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = u;
    }
    if value >= 100.0 {
        format!("{value:>4.0} {unit}")
    } else if value >= 10.0 {
        format!("{value:>4.1} {unit}")
    } else {
        format!("{value:>4.2} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_files_show_raw_byte_count() {
        assert_eq!(format_size(Some(0)), "      0");
        assert_eq!(format_size(Some(42)), "     42");
        assert_eq!(format_size(Some(999)), "    999");
        assert_eq!(format_size(Some(1023)), "   1023");
    }

    #[test]
    fn kibibytes_use_k_suffix() {
        assert_eq!(format_size(Some(1024)), "1.00 K");
        assert_eq!(format_size(Some(2 * 1024)), "2.00 K");
        assert_eq!(format_size(Some(15 * 1024)), "15.0 K");
        assert_eq!(format_size(Some(150 * 1024)), " 150 K");
    }

    #[test]
    fn megabytes_and_gigabytes() {
        assert_eq!(format_size(Some(1024 * 1024)), "1.00 M");
        assert_eq!(format_size(Some(1024 * 1024 * 1024)), "1.00 G");
    }

    #[test]
    fn none_is_blank() {
        assert_eq!(format_size(None), "    ");
    }
}
