//! Theme is data. The mapping from `Theme` slots to `ratatui::style::Style`
//! lives in `mx-tui::theme_styles`; this crate has no terminal dependency.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    /// Hex `#RRGGBB`.
    Hex(u8, u8, u8),
    /// 16-color ANSI index 0..15.
    Ansi(u8),
    /// Take the terminal's default foreground/background.
    Default,
}

impl Color {
    /// Parse `"#RRGGBB"`, `"ansi:N"` (0..15), or `"default"`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("default") {
            return Some(Color::Default);
        }
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(Color::Hex(r, g, b));
            }
        }
        if let Some(idx) = s.strip_prefix("ansi:") {
            let n: u8 = idx.parse().ok()?;
            if n < 16 {
                return Some(Color::Ansi(n));
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub error_fg: Color,
    pub dir_fg: Color,
    pub symlink_fg: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub modal_bg: Color,
    pub modal_fg: Color,
}

impl Theme {
    /// Classic Midnight Commander blue. The default at first run.
    pub const CLASSIC: Self = Self {
        bg: Color::Hex(0x00, 0x00, 0xaa),
        fg: Color::Hex(0xcc, 0xcc, 0xcc),
        accent: Color::Hex(0xff, 0xff, 0x55),
        selection_bg: Color::Hex(0x00, 0x55, 0x77),
        selection_fg: Color::Hex(0xff, 0xff, 0xff),
        error_fg: Color::Hex(0xff, 0x55, 0x55),
        dir_fg: Color::Hex(0xff, 0xff, 0xff),
        symlink_fg: Color::Hex(0x55, 0xff, 0xff),
        status_bg: Color::Hex(0x00, 0x00, 0x77),
        status_fg: Color::Hex(0xff, 0xff, 0xff),
        modal_bg: Color::Hex(0x00, 0x00, 0x77),
        modal_fg: Color::Hex(0xff, 0xff, 0xff),
    };

    /// Terminal-default-bg theme with subtle accents.
    pub const DARK: Self = Self {
        bg: Color::Default,
        fg: Color::Default,
        accent: Color::Hex(0x7d, 0xc4, 0xff),
        selection_bg: Color::Hex(0x33, 0x33, 0x33),
        selection_fg: Color::Default,
        error_fg: Color::Hex(0xff, 0x6b, 0x6b),
        dir_fg: Color::Hex(0x9e, 0xc1, 0xff),
        symlink_fg: Color::Hex(0x82, 0xe6, 0xe6),
        status_bg: Color::Hex(0x22, 0x22, 0x22),
        status_fg: Color::Default,
        modal_bg: Color::Hex(0x1c, 0x1c, 0x1c),
        modal_fg: Color::Default,
    };

    /// Look up a named built-in. `None` → caller falls back to default + warning.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "classic" => Some(Self::CLASSIC),
            "dark" => Some(Self::DARK),
            _ => None,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::CLASSIC
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color() {
        assert_eq!(Color::parse("#ff0000"), Some(Color::Hex(0xff, 0x00, 0x00)));
        assert_eq!(Color::parse("#FFFFFF"), Some(Color::Hex(0xff, 0xff, 0xff)));
        assert_eq!(Color::parse("#abc"), None); // 3-digit not supported
        assert_eq!(Color::parse("ff0000"), None); // missing #
    }

    #[test]
    fn parse_ansi_color() {
        assert_eq!(Color::parse("ansi:0"), Some(Color::Ansi(0)));
        assert_eq!(Color::parse("ansi:15"), Some(Color::Ansi(15)));
        assert_eq!(Color::parse("ansi:16"), None); // out of range
    }

    #[test]
    fn parse_default_color() {
        assert_eq!(Color::parse("default"), Some(Color::Default));
        assert_eq!(Color::parse("DEFAULT"), Some(Color::Default));
    }

    #[test]
    fn named_themes_resolve() {
        assert!(Theme::named("classic").is_some());
        assert!(Theme::named("dark").is_some());
        assert!(Theme::named("solarized").is_none());
    }

    #[test]
    fn default_is_classic() {
        assert_eq!(Theme::default(), Theme::CLASSIC);
    }

    #[test]
    fn classic_is_blue() {
        // The MC heritage check: classic background must be the deep blue.
        assert_eq!(Theme::CLASSIC.bg, Color::Hex(0x00, 0x00, 0xaa));
    }
}
