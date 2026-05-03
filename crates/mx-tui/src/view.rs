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

fn render_panel(frame: &mut Frame<'_>, area: Rect, state: &State, side: PanelSide) {
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

    // Phase 1: panels are empty. Render a centered "(empty)" line so the
    // user can see the panel exists at all. Phase 2 replaces this with the
    // entry list.
    let placeholder = Paragraph::new("(empty)").style(frame_style(theme));
    let one_line = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1) / 2,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(placeholder, one_line);
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let text = if state.status.text.is_empty() {
        format!(" {} files, {} dirs           ", 0, 0)
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
