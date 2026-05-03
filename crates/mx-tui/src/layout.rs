//! Compute frame regions. Exposed as a struct with named `Rect`s so any
//! future post-render effects pass can scope by region without recomputing
//! geometry.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Minimum width (in columns) at which both panels are shown side by side.
/// Below this we collapse to one panel.
pub const NARROW_THRESHOLD: u16 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLayout {
    pub left_panel:  Rect,
    pub right_panel: Option<Rect>,   // None when collapsed
    pub status:      Rect,
    pub hint:        Rect,
    pub modal:       Option<Rect>,
}

#[must_use]
pub fn compute(area: Rect, panel_ratio: u8, modal_open: bool) -> FrameLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    let body   = chunks[0];
    let status = chunks[1];
    let hint   = chunks[2];

    let (left_panel, right_panel) = if area.width < NARROW_THRESHOLD {
        // Narrow mode in Phase 1 always shows the left panel. Phase 2 will
        // swap which panel is visible based on focus.
        (body, None)
    } else {
        let pr = u16::from(panel_ratio.clamp(1, 99));
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(pr), Constraint::Percentage(100 - pr)])
            .split(body);
        (cols[0], Some(cols[1]))
    };

    let modal = if modal_open { Some(centered_rect(area, 60, 50)) } else { None };

    FrameLayout { left_panel, right_panel, status, hint, modal }
}

fn centered_rect(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = area.width.saturating_mul(pct_w) / 100;
    let h = area.height.saturating_mul(pct_h) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: u16, h: u16) -> Rect { Rect { x: 0, y: 0, width: w, height: h } }

    #[test]
    fn wide_terminal_shows_both_panels() {
        let fl = compute(area(120, 30), 50, false);
        assert!(fl.right_panel.is_some());
        assert_eq!(fl.left_panel.width + fl.right_panel.unwrap().width, 120);
        assert_eq!(fl.status.height, 1);
        assert_eq!(fl.hint.height, 1);
    }

    #[test]
    fn narrow_terminal_collapses_to_one_panel() {
        let fl = compute(area(60, 24), 50, false);
        assert!(fl.right_panel.is_none());
        assert_eq!(fl.left_panel.width, 60);
    }

    #[test]
    fn modal_centered_when_open() {
        let fl = compute(area(120, 30), 50, true);
        let m = fl.modal.expect("modal_open=true must produce a modal rect");
        assert!(m.width  > 0 && m.width  < 120);
        assert!(m.height > 0 && m.height < 30);
    }

    #[test]
    fn extreme_panel_ratio_clamped() {
        let fl_low  = compute(area(120, 30), 0,   false);
        let fl_high = compute(area(120, 30), 200, false);
        // Both should render two panels (clamping to 1..=99).
        assert!(fl_low.right_panel.is_some());
        assert!(fl_high.right_panel.is_some());
    }
}
