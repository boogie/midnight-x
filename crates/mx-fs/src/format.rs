//! Formatters for human-readable display of file metadata.

/// Format a byte count using binary units (`K = 1024`). Output width is
/// always ≤ 7 characters: `"   42"`, `"1.2 K"`, `" 999 K"`, `"  1.5 M"`,
/// `"  9.4 G"`, `" 12.0 T"`. `None` renders as 4 spaces.
const UNITS: [&str; 5] = ["K", "M", "G", "T", "P"];

/// Format a byte count to **exactly 7 visible columns**. `None` → 7 spaces.
///
/// Examples:
/// `"      0"`, `"  1.5 K"`, `" 15.0 K"`, `"  150 K"`, `"  1.5 M"`.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn format_size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else {
        return "       ".to_string();
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
    // Number takes 5 cols, then space, then 1-col unit → 7 total.
    if value >= 100.0 {
        format!("{value:>5.0} {unit}")
    } else if value >= 10.0 {
        format!("{value:>5.1} {unit}")
    } else {
        format!("{value:>5.2} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_always_seven_chars() {
        let cases = [
            None,
            Some(0u64),
            Some(42),
            Some(999),
            Some(1023),
            Some(1024),
            Some(1500),
            Some(15 * 1024),
            Some(150 * 1024),
            Some(1024u64 * 1024),
            Some(1024u64 * 1024 * 1024),
            Some(u64::MAX),
        ];
        for case in cases {
            let s = format_size(case);
            assert_eq!(
                s.chars().count(),
                7,
                "format_size({case:?}) = {s:?} (len {})",
                s.chars().count()
            );
        }
    }

    #[test]
    fn small_files_show_raw_byte_count() {
        assert_eq!(format_size(Some(0)), "      0");
        assert_eq!(format_size(Some(42)), "     42");
        assert_eq!(format_size(Some(999)), "    999");
        assert_eq!(format_size(Some(1023)), "   1023");
    }

    #[test]
    fn kibibytes_use_k_suffix() {
        assert_eq!(format_size(Some(1024)), " 1.00 K");
        assert_eq!(format_size(Some(15 * 1024)), " 15.0 K");
        assert_eq!(format_size(Some(150 * 1024)), "  150 K");
    }

    #[test]
    fn none_is_blank() {
        assert_eq!(format_size(None), "       ");
    }
}
