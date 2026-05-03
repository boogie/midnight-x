//! Pure render function. Writes the entire frame into `frame.buffer_mut()`
//! based only on `&State` and a `FrameLayout`. No I/O, no thread access.

use mx_core::state::{Modal, PanelSide, State};

use ratatui::layout::{Alignment, Rect};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
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

    if let (Some(rect), Some(_)) = (layout.modal, state.modal.as_ref()) {
        render_modal(frame, rect, state);
    }
    let _ = theme;
}

#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
fn render_panel(frame: &mut Frame<'_>, area: Rect, state: &State, side: PanelSide) {
    use mx_core::state::EntryKind;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let theme = &state.config.theme;
    let panel = &state.panels[side.index()];
    let focused = state.focus == side;

    let title = format!(" {} ", crate::format::shorten_home(&panel.cwd));
    let border_style = panel_title_style(theme, focused);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(border_style)
        .title(title)
        .title_alignment(Alignment::Center)
        .title_style(border_style)
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

    let w = inner.width as usize;
    let show_mtime = w >= 50;
    let show_size = w >= 30;
    let mtime_w: usize = if show_mtime { 16 } else { 0 };
    let size_w: usize = if show_size { 7 } else { 0 };
    // 1 col gutter + 1 col separator before size + 1 col separator before mtime
    let separators = 1 + usize::from(show_size) + usize::from(show_mtime);
    let name_w = w.saturating_sub(mtime_w + size_w + separators);

    // Header row at the top of the inner area.
    let header_y = inner.y;
    {
        let mut header = String::new();
        header.push(' '); // gutter
        header.push_str(&render_center("Name", name_w));
        if show_size {
            header.push(' '); // separator (drawn by overlay below)
            header.push_str(&render_center("Size", size_w));
        }
        if show_mtime {
            header.push(' ');
            header.push_str(&render_center("Modified", mtime_w));
        }
        let p = Paragraph::new(header).style(border_style);
        let row_area = Rect {
            x: inner.x,
            y: header_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);
    }

    // Body sits between the header and a per-panel footer (focused-entry
    // detail). When the panel is too short to fit divider+footer, the
    // footer is dropped and the body grows to use that space instead.
    let want_footer = inner.height >= 4;
    let header_h: u16 = 1;
    let footer_h: u16 = u16::from(want_footer);
    let divider_h: u16 = u16::from(want_footer);
    let body_y = inner.y + header_h;
    let visible_h = inner.height.saturating_sub(header_h + divider_h + footer_h) as usize;
    let scroll = clamp_scroll(panel.scroll, panel.cursor, visible_h, panel.entries.len());

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
        let mut text = String::new();
        text.push_str(gutter);
        text.push_str(&name_render);
        if show_size {
            text.push(' '); // separator slot — drawn by overlay
            text.push_str(&size_cell(entry));
        }
        if show_mtime {
            text.push(' ');
            text.push_str(&crate::format::format_mtime(
                entry.mtime,
                &state.config.ui.date_format,
            ));
        }

        let line = Line::from(vec![Span::raw(text)]);
        let p = Paragraph::new(line).style(row_style);
        let row_area = Rect {
            x: inner.x,
            y: body_y + row as u16,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);
    }

    // Inner divider + per-panel footer (focused-entry detail). Drawn before
    // the column-separator overlay so the overlay's `┴` caps land cleanly.
    if want_footer {
        let divider_y = inner.y + inner.height - 2;
        let footer_y = inner.y + inner.height - 1;

        // Horizontal divider in border style.
        let divider: String = "─".repeat(inner.width as usize);
        let p = Paragraph::new(divider).style(border_style);
        let row_area = Rect {
            x: inner.x,
            y: divider_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);

        // Footer with focused entry's full info.
        let footer_text = panel
            .entries
            .get(panel.cursor)
            .map(|e| {
                format_footer(
                    e,
                    inner.width as usize,
                    &state.config.ui.date_format,
                )
            })
            .unwrap_or_default();
        let p = Paragraph::new(footer_text).style(frame_style(theme));
        let row_area = Rect {
            x: inner.x,
            y: footer_y,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);
    }

    // Vertical column separators — overlay them now so they sit cleanly on
    // top of the header / body, and connect to the panel border with caps.
    // Use a clean style that *clears* any reverse-video / bold the cursor
    // or selection rows might have left at this column.
    let sep_style = border_style.remove_modifier(Modifier::all());
    let inner_x = inner.x as usize;
    let mut sep_xs: Vec<u16> = Vec::with_capacity(2);
    if show_size {
        sep_xs.push((inner_x + 1 + name_w) as u16);
    }
    if show_mtime {
        sep_xs.push((inner_x + 1 + name_w + 1 + size_w) as u16);
    }
    // Where vertical separators end. With a footer the separators stop at
    // the divider row (single-line `┴` cap). Without a footer they extend
    // through the bottom border (`╧` cap on the double border).
    let sep_end_y = if want_footer {
        inner.y + inner.height - 2 // divider row
    } else {
        area.y + area.height - 1 // bottom border
    };
    for x in sep_xs {
        // Top cap — only replace plain border `═`, never the cwd title.
        if let Some(cell) = frame.buffer_mut().cell_mut((x, area.y)) {
            if cell.symbol() == "═" {
                cell.set_symbol("╤"); // single-column meeting double horizontal
                cell.set_style(sep_style);
            }
        }
        // Bottom cap of the separator.
        if let Some(cell) = frame.buffer_mut().cell_mut((x, sep_end_y)) {
            let prev = cell.symbol().to_string();
            let new = match prev.as_str() {
                "─" => "┴", // landing on single-line divider
                "═" => "╧", // landing on double-line bottom border
                _ => prev.as_str(),
            };
            cell.set_symbol(new);
            cell.set_style(sep_style);
        }
        // Vertical line from header through body (and divider, if no footer).
        for y in inner.y..sep_end_y {
            if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
                cell.set_symbol("│");
                cell.set_style(sep_style);
            }
        }
    }
}

fn size_cell(entry: &mx_core::state::DirEntry) -> String {
    use mx_core::state::EntryKind;
    if entry.name == ".." {
        "   <Up>".to_string() // 7 chars: 3-space pad + "<Up>"
    } else if entry.kind == EntryKind::Dir {
        "  <Dir>".to_string() // 7 chars: 2-space pad + "<Dir>"
    } else {
        mx_fs::format::format_size(entry.size)
    }
}

fn format_footer(entry: &mx_core::state::DirEntry, width: usize, date_fmt: &str) -> String {
    use mx_core::state::EntryKind;
    let size_str = match entry.kind {
        EntryKind::Dir if entry.name == ".." => "<Up>".to_string(),
        EntryKind::Dir => "<Dir>".to_string(),
        _ => mx_fs::format::format_size(entry.size).trim_start().to_string(),
    };
    let mtime_str = if entry.mtime.is_some() {
        crate::format::format_mtime(entry.mtime, date_fmt)
    } else {
        String::new()
    };

    // Right side: " <size>  <mtime> "
    let right = if mtime_str.is_empty() {
        format!(" {size_str} ")
    } else {
        format!(" {size_str}  {mtime_str} ")
    };
    let right_w = right.chars().count();

    // Left side: " <name>"
    let mut left = String::with_capacity(width);
    left.push(' ');
    let name_room = width.saturating_sub(right_w + 1);
    left.push_str(&render_name(&entry.name, name_room));

    let mut out = left;
    out.push_str(&right);
    if out.chars().count() > width {
        out = out.chars().take(width).collect();
    }
    out
}

fn render_center(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() >= width {
        return chars.into_iter().take(width).collect();
    }
    let pad = width - chars.len();
    let left = pad / 2;
    let right = pad - left;
    let mut s = String::with_capacity(width);
    for _ in 0..left {
        s.push(' ');
    }
    s.push_str(text);
    for _ in 0..right {
        s.push(' ');
    }
    s
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
    let (files, dirs, bytes) = panel
        .entries
        .iter()
        .filter(|e| e.name != "..")
        .fold((0u64, 0u64, 0u64), |(f, d, b), e| match e.kind {
            mx_core::state::EntryKind::Dir => (f, d + 1, b),
            mx_core::state::EntryKind::Symlink | mx_core::state::EntryKind::File => {
                (f + 1, d, b + e.size.unwrap_or(0))
            }
            mx_core::state::EntryKind::Unreadable => (f, d, b),
        });
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

fn render_modal(frame: &mut Frame<'_>, area: Rect, state: &State) {
    use ratatui::style::Modifier;
    use ratatui::widgets::Padding;

    let theme = &state.config.theme;
    let modal = state
        .modal
        .as_ref()
        .expect("render_modal called without a modal");
    frame.render_widget(Clear, area);

    let title = modal_title(modal);
    let mstyle = modal_style(theme);

    // Outer block: rounded single-line border, internal 2-col + 1-row
    // padding, centred bold title on the top border.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title)
        .title_alignment(Alignment::Center)
        .title_style(mstyle.add_modifier(Modifier::BOLD))
        .style(mstyle)
        .padding(Padding::new(2, 2, 1, 1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Reserve last row of the inner area for the button strip when the
    // modal owns buttons. A 1-row gap separates body from buttons.
    let buttons = modal_buttons(modal);
    let body_h = if buttons.is_empty() {
        inner.height
    } else {
        inner.height.saturating_sub(2)
    };
    let body_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: body_h,
    };

    let center_body = matches!(modal, Modal::Confirm(_) | Modal::QuitConfirm);
    let body = body_text(modal, body_area, &state.config.keymap);
    let p = Paragraph::new(body).style(mstyle);
    let p = if center_body {
        p.alignment(Alignment::Center)
    } else {
        p
    };
    frame.render_widget(p, body_area);

    if !buttons.is_empty() {
        let row = Rect {
            x: inner.x,
            y: inner.y + body_h + 1, // 1-row gap below body
            width: inner.width,
            height: 1,
        };
        let line = button_row(theme, &buttons);
        let p = Paragraph::new(line)
            .style(mstyle)
            .alignment(Alignment::Center);
        frame.render_widget(p, row);
    }
}

fn modal_title(modal: &Modal) -> &'static str {
    match modal {
        Modal::Help => " Help ",
        Modal::QuitConfirm => " Quit? ",
        Modal::Confirm(d) => match d.kind {
            mx_core::state::ConfirmKind::Delete { .. }    => " Delete ",
            mx_core::state::ConfirmKind::StartCopy { .. } => " Copy ",
            mx_core::state::ConfirmKind::StartMove { .. } => " Move ",
            mx_core::state::ConfirmKind::Conflict { .. }  => " Overwrite? ",
            mx_core::state::ConfirmKind::QuitWithWorkers  => " Quit? ",
        },
        Modal::Op(d) => match d.kind {
            mx_core::state::OpKind::Copy { .. } => " Copy ",
            mx_core::state::OpKind::Move { .. } => " Move ",
        },
        Modal::Input(_) => " Input ",
        Modal::Progress(_) => " Working… ",
        Modal::Error(_) => " Error ",
        Modal::Viewer(_) => " View (Esc/F3 close, ↑↓ scroll) ",
    }
}

fn body_text(modal: &Modal, area: Rect, keymap: &mx_core::keymap::Keymap) -> String {
    match modal {
        Modal::Help => render_help_body(keymap),
        Modal::QuitConfirm => "Workers are still running.\nQuit anyway?".to_string(),
        Modal::Error(d) => render_error_body(d),
        Modal::Confirm(d) => render_confirm_body(d),
        Modal::Input(d) => render_input_body(d),
        Modal::Progress(d) => render_progress_body(d, area),
        Modal::Viewer(d) => render_viewer_body(d, area),
        Modal::Op(d) => render_op_body(d),
    }
}

#[derive(Clone, Copy)]
struct ButtonSpec {
    label: &'static str,
    focused: bool,
}

fn modal_buttons(modal: &Modal) -> Vec<ButtonSpec> {
    use mx_core::state::{ConfirmButton, OpFocus};
    let label = |b: ConfirmButton| match b {
        ConfirmButton::Yes => "Yes",
        ConfirmButton::No => "No",
        ConfirmButton::YesAll => "Yes-All",
        ConfirmButton::NoAll => "No-All",
        ConfirmButton::Cancel => "Cancel",
        ConfirmButton::Ok => "OK",
        ConfirmButton::Delete => "Delete",
        ConfirmButton::Copy => "Copy",
        ConfirmButton::Move => "Move",
    };
    match modal {
        Modal::Confirm(d) => d
            .buttons
            .iter()
            .enumerate()
            .map(|(i, b)| ButtonSpec {
                label: label(*b),
                focused: i == d.focused,
            })
            .collect(),
        Modal::Op(d) => d
            .buttons
            .iter()
            .enumerate()
            .map(|(i, b)| ButtonSpec {
                label: label(*b),
                focused: matches!(d.focus, OpFocus::Button(idx) if idx == i),
            })
            .collect(),
        Modal::QuitConfirm => vec![
            ButtonSpec { label: "Yes", focused: false },
            ButtonSpec { label: "No",  focused: true },
        ],
        Modal::Error(_) => vec![ButtonSpec { label: "OK", focused: true }],
        // Help / Input / Progress / Viewer use their own footer hint text;
        // no separate button strip.
        _ => Vec::new(),
    }
}

fn button_row<'a>(theme: &mx_core::theme::Theme, buttons: &[ButtonSpec]) -> ratatui::text::Line<'a> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::Span;
    let mut spans: Vec<Span<'a>> = Vec::with_capacity(buttons.len() * 2);
    for (i, b) in buttons.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        let label = format!(" {} ", b.label);
        let style = if b.focused {
            Style::default()
                .bg(crate::theme_styles::rcolor(theme.accent))
                .fg(crate::theme_styles::rcolor(theme.modal_bg))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(crate::theme_styles::rcolor(theme.modal_bg))
                .fg(crate::theme_styles::rcolor(theme.modal_fg))
        };
        spans.push(Span::styled(label, style));
    }
    ratatui::text::Line::from(spans)
}

fn render_viewer_body(d: &mx_core::state::ViewerDialog, area: Rect) -> String {
    if d.loading {
        return "loading…".to_string();
    }
    let total: Vec<&str> = d.body.lines().collect();
    let visible = area.height.saturating_sub(2) as usize;
    let max_scroll = total.len().saturating_sub(visible);
    let scroll = d.scroll.min(max_scroll);
    let slice = total
        .get(scroll..(scroll + visible).min(total.len()))
        .unwrap_or(&[]);
    let mut out = slice.join("\n");
    if d.truncated {
        out.push_str("\n[truncated]");
    }
    out
}

fn render_help_body(keymap: &mx_core::keymap::Keymap) -> String {
    use mx_core::command::CommandId;
    use mx_core::keymap::sequence_to_string;
    use std::fmt::Write as _;

    let groups: Vec<(&'static str, Vec<CommandId>)> = vec![
        (
            "Navigation",
            vec![
                CommandId::CursorUp,
                CommandId::CursorDown,
                CommandId::CursorPageUp,
                CommandId::CursorPageDown,
                CommandId::CursorHome,
                CommandId::CursorEnd,
                CommandId::EnterDir,
                CommandId::ParentDir,
                CommandId::FocusOther,
                CommandId::SwapPanels,
            ],
        ),
        (
            "Selection",
            vec![
                CommandId::ToggleSelect,
                CommandId::SelectAll,
                CommandId::SelectNone,
                CommandId::InvertSelection,
            ],
        ),
        (
            "View",
            vec![
                CommandId::View,
                CommandId::ToggleHidden,
                CommandId::CycleSort,
            ],
        ),
        (
            "File ops",
            vec![
                CommandId::Copy,
                CommandId::Move,
                CommandId::Delete,
                CommandId::Mkdir,
                CommandId::Rename,
            ],
        ),
        (
            "App",
            vec![
                CommandId::Help,
                CommandId::QuitConfirm,
                CommandId::Quit,
                CommandId::Cancel,
                CommandId::RescanFocused,
                CommandId::RescanBoth,
            ],
        ),
    ];

    let mut out = String::new();
    for (group, cmds) in &groups {
        out.push_str(group);
        out.push('\n');
        for c in cmds {
            let label = command_label(*c);
            let bindings: Vec<String> = keymap
                .bindings_view()
                .iter()
                .filter(|(_, cmd)| cmd == c)
                .map(|(seq, _)| sequence_to_string(seq))
                .collect();
            if bindings.is_empty() {
                continue;
            }
            let _ = writeln!(out, "  {:<22} {}", label, bindings.join(", "));
        }
        out.push('\n');
    }
    out.push_str("(Esc closes this help.)");
    out
}

fn command_label(c: mx_core::command::CommandId) -> &'static str {
    use mx_core::command::CommandId;
    match c {
        CommandId::CursorUp => "Move cursor up",
        CommandId::CursorDown => "Move cursor down",
        CommandId::CursorPageUp => "Page up",
        CommandId::CursorPageDown => "Page down",
        CommandId::CursorHome => "Top",
        CommandId::CursorEnd => "Bottom",
        CommandId::EnterDir => "Enter directory",
        CommandId::ParentDir => "Parent directory",
        CommandId::FocusOther => "Other panel",
        CommandId::SwapPanels => "Swap panels",
        CommandId::ToggleSelect => "Toggle select",
        CommandId::SelectAll => "Select all",
        CommandId::SelectNone => "Select none",
        CommandId::InvertSelection => "Invert selection",
        CommandId::View => "View file",
        CommandId::ToggleHidden => "Toggle hidden",
        CommandId::CycleSort => "Cycle sort mode",
        CommandId::Copy => "Copy",
        CommandId::Move => "Move",
        CommandId::Delete => "Delete",
        CommandId::Mkdir => "Make directory",
        CommandId::Rename => "Rename",
        CommandId::Help => "Help",
        CommandId::QuitConfirm => "Quit (confirm)",
        CommandId::Quit => "Quit",
        CommandId::Cancel => "Cancel / close modal",
        CommandId::RescanFocused => "Rescan",
        CommandId::RescanBoth => "Rescan both",
    }
}

fn render_confirm_body(d: &mx_core::state::ConfirmDialog) -> String {
    // Buttons are rendered separately by the modal renderer's button row.
    d.body.clone()
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn render_progress_body(d: &mx_core::state::ProgressDialog, area: Rect) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    out.push_str(d.current_path.as_str());
    out.push('\n');
    let inner_w = area.width.saturating_sub(4) as usize;
    let bar_w = inner_w.saturating_sub(8);
    if d.bytes_total > 0 && bar_w > 0 {
        let ratio = (d.bytes_done.min(d.bytes_total)) as f64 / d.bytes_total as f64;
        let filled = (ratio * bar_w as f64) as usize;
        out.push('[');
        for _ in 0..filled {
            out.push('█');
        }
        for _ in filled..bar_w {
            out.push('─');
        }
        let pct = (d.bytes_done * 100) / d.bytes_total.max(1);
        let _ = write!(out, "] {pct:>3}%");
    } else {
        out.push('…');
    }
    out.push_str("\n\n");
    out.push_str(" Esc / Ctrl-C = Cancel ");
    out
}

fn render_error_body(d: &mx_core::state::ErrorDialog) -> String {
    let mut out = String::new();
    out.push_str(&d.body);
    if !d.details.is_empty() {
        out.push_str("\n\nDetails:\n");
        for line in &d.details {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn render_op_body(d: &mx_core::state::OpDialog) -> String {
    use mx_core::state::OpFocus;
    let mut out = String::new();
    out.push_str(&d.prompt);
    out.push('\n');
    // Path field: bracket the field on both sides; show inline `▏` cursor
    // when focused so the user always sees where editing happens.
    let cursor = d.cursor.min(d.target.len());
    out.push('[');
    if matches!(d.focus, OpFocus::Path) {
        out.push_str(&d.target[..cursor]);
        out.push('▏');
        out.push_str(&d.target[cursor..]);
    } else {
        out.push_str(&d.target);
    }
    out.push(']');
    out
}

fn render_input_body(d: &mx_core::state::InputDialog) -> String {
    let mut out = String::new();
    if !d.title.is_empty() {
        out.push_str(&d.title);
        out.push('\n');
        out.push('\n');
    }
    out.push_str(&d.prompt);
    out.push('\n');
    let cursor = d.cursor.min(d.value.len());
    out.push_str("> ");
    out.push_str(&d.value[..cursor]);
    out.push('▏');
    out.push_str(&d.value[cursor..]);
    out.push_str("\n\n");
    out.push_str(" Enter = OK   Esc = Cancel ");
    out
}
