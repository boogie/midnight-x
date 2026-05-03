//! Map the terminal-agnostic `mx_core::theme::Theme` onto concrete
//! `ratatui::style::Style` instances. This is the single chokepoint for the
//! "no hardcoded `Color::Blue` outside the theme module" rule.

use mx_core::theme::{Color, Theme};
use ratatui::style::{Color as RColor, Modifier, Style};

#[must_use]
pub fn rcolor(c: Color) -> RColor {
    match c {
        Color::Hex(r, g, b) => RColor::Rgb(r, g, b),
        Color::Ansi(n) => RColor::Indexed(n),
        Color::Default => RColor::Reset,
    }
}

#[must_use]
pub fn frame_style(t: &Theme) -> Style {
    Style::default().bg(rcolor(t.bg)).fg(rcolor(t.fg))
}

#[must_use]
pub fn panel_title_style(t: &Theme, focused: bool) -> Style {
    let base = Style::default().bg(rcolor(t.bg));
    if focused {
        base.fg(rcolor(t.accent)).add_modifier(Modifier::BOLD)
    } else {
        base.fg(rcolor(t.fg))
    }
}

#[must_use]
pub fn status_style(t: &Theme) -> Style {
    Style::default()
        .bg(rcolor(t.status_bg))
        .fg(rcolor(t.status_fg))
}

#[must_use]
pub fn modal_style(t: &Theme) -> Style {
    Style::default()
        .bg(rcolor(t.modal_bg))
        .fg(rcolor(t.modal_fg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_frame_style_is_blue_on_grey() {
        let s = frame_style(&Theme::CLASSIC);
        assert_eq!(s.bg, Some(RColor::Rgb(0x00, 0x00, 0xaa)));
        assert_eq!(s.fg, Some(RColor::Rgb(0xcc, 0xcc, 0xcc)));
    }

    #[test]
    fn dark_uses_reset_for_default_color() {
        let s = frame_style(&Theme::DARK);
        assert_eq!(s.bg, Some(RColor::Reset));
        assert_eq!(s.fg, Some(RColor::Reset));
    }

    #[test]
    fn focused_title_uses_accent_and_bold() {
        let s = panel_title_style(&Theme::CLASSIC, true);
        assert_eq!(s.fg, Some(RColor::Rgb(0xff, 0xff, 0x55)));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }
}
