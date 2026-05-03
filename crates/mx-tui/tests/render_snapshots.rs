//! Snapshot tests for `view()` rendering. Reviewed via `cargo insta review`.

use std::sync::Arc;

use mx_core::config::Config;
use mx_core::state::{PanelSide, State};
use mx_core::theme::Theme;
use mx_tui::renderer::{buffer_snapshot, draw_to_buffer};
use ratatui::layout::Rect;

fn fresh(theme: Theme) -> State {
    let c = Config { theme, ..Config::default() };
    let mut s = State::new(Arc::new(c), "/Users/boogie".into(), "/tmp".into());
    s.focus = PanelSide::Left;
    s
}

#[test]
fn empty_panels_classic_theme_wide() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("empty_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panels_dark_theme_wide() {
    let s = fresh(Theme::DARK);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("empty_dark_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panel_collapsed_narrow() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 60, height: 24 });
    insta::assert_snapshot!("empty_classic_narrow", buffer_snapshot(&buf));
}

#[test]
fn help_modal_open() {
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(mx_core::state::Modal::Help);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("help_modal_classic_wide", buffer_snapshot(&buf));
}
