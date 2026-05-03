//! Pure render function. Writes the entire frame into `frame.buffer_mut()`
//! based only on `&State` and a `FrameLayout`. No I/O, no thread access.

use mx_core::state::{Modal, PanelSide, State};

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::layout::FrameLayout;
use crate::theme_styles::{frame_style, modal_style, panel_title_style, status_style};

pub fn view(state: &State, layout: &FrameLayout, frame: &mut Frame<'_>) {
    let theme = &state.config.theme;

    // Background fill: a Block with the theme background painted across the
    // full area covers any cells we don't otherwise write to.
    let full = frame.area();
    frame.render_widget(Block::default().style(frame_style(theme)), full);

    render_panel(frame, layout.left_panel, state, PanelSide::Left);
    if let Some(rp) = layout.right_panel {
        render_panel(frame, rp, state, PanelSide::Right);
    }
    render_status(frame, layout.status, state);
    render_hint(frame, layout.hint, state);

    if let (Some(rect), Some(modal)) = (layout.modal, state.modal.as_ref()) {
        render_modal(frame, rect, modal, theme);
    }
}

#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
fn render_panel(frame: &mut Frame<'_>, area: Rect, state: &State, side: PanelSide) {
    use mx_core::state::EntryKind;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let theme = &state.config.theme;
    let panel = &state.panels[side.index()];
    let focused = state.focus == side;

    let title = format!(" {} ", panel.cwd);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(panel_title_style(theme, focused))
        .title(title)
        .title_style(panel_title_style(theme, focused))
        .style(frame_style(theme));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if panel.loading {
        let p = Paragraph::new("loading…").style(frame_style(theme));
        let one_line = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1) / 2,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, one_line);
        return;
    }

    if panel.entries.is_empty() {
        let p = Paragraph::new("(empty)").style(frame_style(theme));
        let one_line = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1) / 2,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, one_line);
        return;
    }

    let visible_h = inner.height as usize;
    let scroll = clamp_scroll(panel.scroll, panel.cursor, visible_h, panel.entries.len());

    let w = inner.width as usize;
    let show_mtime = w >= 50;
    let show_size = w >= 30;
    let mtime_w: usize = if show_mtime { 17 } else { 0 };
    let size_w: usize = if show_size { 8 } else { 0 };
    let name_w = w.saturating_sub(mtime_w + size_w + 2);

    for row in 0..visible_h {
        let entry_index = scroll + row;
        if entry_index >= panel.entries.len() {
            break;
        }
        let entry = &panel.entries[entry_index];

        let is_cursor = entry_index == panel.cursor && focused;
        let is_selected = panel.selection.contains(&entry_index);

        let kind_style = match entry.kind {
            EntryKind::Dir => Style::default().fg(crate::theme_styles::rcolor(theme.dir_fg)),
            EntryKind::Symlink if entry.symlink_broken => {
                Style::default().fg(crate::theme_styles::rcolor(theme.error_fg))
            }
            EntryKind::Symlink => {
                Style::default().fg(crate::theme_styles::rcolor(theme.symlink_fg))
            }
            EntryKind::Unreadable => {
                Style::default().fg(crate::theme_styles::rcolor(theme.error_fg))
            }
            EntryKind::File => Style::default().fg(crate::theme_styles::rcolor(theme.fg)),
        };

        let mut row_style = frame_style(theme).patch(kind_style);
        if is_selected {
            row_style = row_style
                .bg(crate::theme_styles::rcolor(theme.selection_bg))
                .fg(crate::theme_styles::rcolor(theme.selection_fg))
                .add_modifier(Modifier::BOLD);
        }
        if is_cursor {
            row_style = row_style.add_modifier(Modifier::REVERSED);
        }

        let gutter = if is_selected { "•" } else { " " };
        let name_render = render_name(&entry.name, name_w);
        let size_render = if show_size {
            format!(" {}", mx_fs::format::format_size(entry.size))
        } else {
            String::new()
        };
        let mtime_render = if show_mtime {
            format!(
                " {}",
                crate::format::format_mtime(entry.mtime, &state.config.ui.date_format)
            )
        } else {
            String::new()
        };

        let line = Line::from(vec![
            Span::raw(gutter.to_string()),
            Span::raw(name_render),
            Span::raw(size_render),
            Span::raw(mtime_render),
        ]);
        let p = Paragraph::new(line).style(row_style);
        let row_area = Rect {
            x: inner.x,
            y: inner.y + row as u16,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);
    }
}

fn render_name(name: &str, width: usize) -> String {
    use std::fmt::Write as _;
    let chars: Vec<char> = name.chars().collect();
    let mut s = String::with_capacity(width);
    if chars.len() <= width {
        s.push_str(name);
        for _ in chars.len()..width {
            s.push(' ');
        }
    } else if width >= 1 {
        let take = width.saturating_sub(1);
        for c in chars.iter().take(take) {
            s.push(*c);
        }
        let _ = write!(s, "…");
    }
    s
}

/// Choose a `scroll` so that `cursor` is on-screen and we don't scroll past
/// the end.
fn clamp_scroll(scroll: usize, cursor: usize, visible_h: usize, len: usize) -> usize {
    if visible_h == 0 || len == 0 {
        return 0;
    }
    let max_scroll = len.saturating_sub(visible_h);
    let mut s = scroll.min(max_scroll);
    if cursor < s {
        s = cursor;
    }
    if cursor >= s + visible_h {
        s = cursor + 1 - visible_h;
    }
    s.min(max_scroll)
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let panel = state.focused();
    let (files, dirs, bytes) = panel.entries.iter().fold(
        (0u64, 0u64, 0u64),
        |(f, d, b), e| match e.kind {
            mx_core::state::EntryKind::Dir => (f, d + 1, b),
            mx_core::state::EntryKind::Symlink | mx_core::state::EntryKind::File => {
                (f + 1, d, b + e.size.unwrap_or(0))
            }
            mx_core::state::EntryKind::Unreadable => (f, d, b),
        },
    );
    let text = if state.status.text.is_empty() {
        format!(
            " {files} files, {dirs} dirs, {} ",
            mx_fs::format::format_size(Some(bytes)).trim_start()
        )
    } else {
        format!(" {}", state.status.text)
    };
    frame.render_widget(Paragraph::new(text).style(status_style(theme)), area);
}

fn render_hint(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let hint = " F1 Help  F3 View  F5 Copy  F6 Move  F7 Mkdir  F8 Del  F10 Quit ";
    frame.render_widget(Paragraph::new(hint).style(status_style(theme)), area);
    let _ = state; // reserved for context-sensitive hints in Phase 2
}

fn render_modal(frame: &mut Frame<'_>, area: Rect, modal: &Modal, theme: &mx_core::theme::Theme) {
    frame.render_widget(Clear, area);
    let title = match modal {
        Modal::Help => " Help ",
        Modal::QuitConfirm => " Quit? ",
        Modal::Confirm(_) => " Confirm ",
        Modal::Input(_) => " Input ",
        Modal::Progress(_) => " Working… ",
        Modal::Error(_) => " Error ",
    };
    let body = match modal {
        Modal::Help => "Phase 1 help: F10/Ctrl-Q quits, Tab toggles focus, Esc cancels.\n\n\
             (Press F1 again or Esc to dismiss.)"
            .to_string(),
        Modal::QuitConfirm => {
            "Workers are still running. Quit anyway?\n\n[ Yes ]   [ No ]".to_string()
        }
        Modal::Error(d) => d.body.clone(),
        Modal::Confirm(d) => d.body.clone(),
        Modal::Input(d) => format!("{}\n> {}", d.prompt, d.value),
        Modal::Progress(d) => {
            format!(
                "{}\n{} / {} bytes",
                d.current_path, d.bytes_done, d.bytes_total
            )
        }
    };
    let p = Paragraph::new(body)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(modal_style(theme))
                .title_style(modal_style(theme)),
        )
        .style(modal_style(theme));
    frame.render_widget(p, area);
}
