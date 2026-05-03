//! Snapshot tests for `view()` rendering. Reviewed via `cargo insta review`.

use std::sync::Arc;

use mx_core::config::Config;
use mx_core::state::{PanelSide, State};
use mx_core::theme::Theme;
use mx_tui::renderer::{buffer_snapshot, draw_to_buffer};
use ratatui::layout::Rect;

fn fresh(theme: Theme) -> State {
    let c = Config {
        theme,
        ..Config::default()
    };
    // Use HOME-independent paths so snapshots don't depend on $HOME
    // (shorten_home would otherwise rewrite the panel title differently
    // on CI vs local).
    let mut s = State::new(Arc::new(c), "/proj".into(), "/tmp".into());
    s.focus = PanelSide::Left;
    s
}

#[test]
fn empty_panels_classic_theme_wide() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("empty_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panels_dark_theme_wide() {
    let s = fresh(Theme::DARK);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("empty_dark_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panel_collapsed_narrow() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 24,
        },
    );
    insta::assert_snapshot!("empty_classic_narrow", buffer_snapshot(&buf));
}

fn populated_state(theme: Theme) -> State {
    use mx_core::state::DirEntry;
    let c = Config {
        theme,
        ..Config::default()
    };
    let mut s = State::new(Arc::new(c), "/proj".into(), "/tmp".into());
    s.focus = PanelSide::Left;
    let entries = vec![
        DirEntry::parent(),
        DirEntry::dir("src"),
        DirEntry::dir("target"),
        DirEntry::file("Cargo.toml", 1234),
        DirEntry::file("README.md", 4321),
        DirEntry::file("rust-toolchain.toml", 80),
        DirEntry::symlink("link-to-src", "src"),
    ];
    s.panels[0].entries = entries.into();
    s.panels[0].cursor = 3;
    s.panels[0].selection.insert(4);
    s
}

#[test]
fn populated_classic_wide_with_selection_and_cursor() {
    let s = populated_state(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("populated_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn populated_dark_wide() {
    let s = populated_state(Theme::DARK);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("populated_dark_wide", buffer_snapshot(&buf));
}

#[test]
fn populated_classic_narrow_drops_columns() {
    let s = populated_state(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 24,
        },
    );
    insta::assert_snapshot!("populated_classic_narrow", buffer_snapshot(&buf));
}

#[test]
fn help_modal_open() {
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(mx_core::state::Modal::Help);
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("help_modal_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn confirm_modal_two_buttons() {
    use mx_core::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Confirm(ConfirmDialog {
        title: "Confirm delete".into(),
        body: "Delete /tmp/x.txt?".into(),
        buttons: vec![ConfirmButton::No, ConfirmButton::Yes],
        focused: 0,
        kind: ConfirmKind::Delete {
            paths: vec!["/tmp/x.txt".into()],
        },
    }));
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("confirm_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn input_modal_with_value() {
    use mx_core::state::{InputDialog, InputKind, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Input(InputDialog {
        title: "Make directory".into(),
        prompt: "name:".into(),
        value: "newdir".into(),
        cursor: 6,
        kind: InputKind::Mkdir {
            parent: "/tmp".into(),
        },
    }));
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("input_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn progress_modal_at_50_percent() {
    use mx_core::event::WorkerId;
    use mx_core::state::{Modal, ProgressDialog};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Progress(ProgressDialog {
        title: "Working…".into(),
        current_path: "/tmp/big.bin".into(),
        bytes_done: 50_000_000,
        bytes_total: 100_000_000,
        worker_id: WorkerId(1),
    }));
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("progress_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn error_modal_with_details() {
    use mx_core::state::{ErrorDialog, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Error(ErrorDialog {
        title: "Operation failed".into(),
        body: "3 errors".into(),
        details: vec![
            "/x/a: permission denied".into(),
            "/x/b: not found".into(),
            "/x/c: io: disk full".into(),
        ],
    }));
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("error_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn viewer_modal_with_content() {
    use mx_core::state::{Modal, ViewerDialog};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Viewer(ViewerDialog {
        path: "/x/hello.txt".into(),
        body: "line one\nline two\nline three".into(),
        scroll: 0,
        truncated: false,
        binary: false,
        loading: false,
    }));
    let buf = draw_to_buffer(
        &s,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        },
    );
    insta::assert_snapshot!("viewer_classic_wide", buffer_snapshot(&buf));
}
