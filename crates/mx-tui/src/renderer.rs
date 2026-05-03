//! Single rendering chokepoint. Owns the `terminal.draw(|frame| …)` closure,
//! exposes a deterministic `draw_to_buffer` for snapshot tests, and runs the
//! four explicit phases (layout → widgets → effects → cursor) so the
//! tachyonfx animation layer can drop in later.

use std::io;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};

use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal;

use mx_core::state::State;

use crate::layout::{compute, FrameLayout};
use crate::view::view;

/// Boxed effect — empty in Phase 1; tachyonfx will populate this trait.
pub trait Effect: Send {
    fn process(&mut self, dt: Duration, buf: &mut Buffer, layout: &FrameLayout);
}

pub struct Renderer<B: Backend> {
    pub terminal: Terminal<B>,
    pub effects:  Vec<Box<dyn Effect>>,
}

impl<B: Backend> Renderer<B> {
    pub fn new(terminal: Terminal<B>) -> Self {
        Self { terminal, effects: Vec::new() }
    }

    /// Run the four-phase render. `dt` is the elapsed wall-clock duration
    /// since the last `draw` (used by `Effect`s; ignored in Phase 1).
    ///
    /// # Errors
    ///
    /// Propagates any `io::Error` from terminal size queries or `draw`.
    pub fn draw(&mut self, state: &State, dt: Duration) -> io::Result<()> {
        let size = self.terminal.size()?;
        let area = Rect { x: 0, y: 0, width: size.width, height: size.height };
        let layout = compute(area, state.config.ui.panel_ratio, state.modal.is_some());

        // Split-borrow self into its two disjoint fields so the closure can
        // mutate `effects` while `terminal.draw` holds `terminal`.
        let Self { terminal, effects } = self;
        terminal.draw(|frame| {
            // Phase 1: widgets
            view(state, &layout, frame);
            // Phase 2: effects (no-op in Phase 1; tachyonfx-compat seam)
            for ef in effects.iter_mut() {
                ef.process(dt, frame.buffer_mut(), &layout);
            }
            // Phase 3: cursor — Phase 1 has no text-input modal active.
        })?;
        Ok(())
    }
}

/// Snapshot-test helper: render to a fixed-size off-screen buffer.
///
/// # Panics
///
/// Panics if the in-memory `TestBackend` fails to construct or to draw —
/// neither path can fail in practice, but we surface the panic rather than
/// hide it behind an `io::Result` for tests.
#[must_use]
pub fn draw_to_buffer(state: &State, area: Rect) -> Buffer {
    let backend = TestBackend::new(area.width, area.height);
    let term = Terminal::new(backend).expect("TestBackend Terminal cannot fail");
    let mut renderer = Renderer::new(term);
    renderer.draw(state, Duration::ZERO).expect("TestBackend draw cannot fail");
    renderer.terminal.backend().buffer().clone()
}

/// Acquire the terminal: enable raw mode + alternate screen + hide cursor.
/// Returns a `Renderer` wrapped around a real `CrosstermBackend`.
///
/// # Errors
///
/// Propagates `io::Error` from raw-mode and alt-screen syscalls.
pub fn acquire() -> io::Result<Renderer<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(out);
    let term = Terminal::new(backend)?;
    Ok(Renderer::new(term))
}

/// Inverse of `acquire`: restore terminal to its prior mode. Idempotent.
///
/// # Errors
///
/// Currently always returns `Ok(())`; the inner calls are best-effort.
pub fn release() -> io::Result<()> {
    let mut out = io::stdout();
    let _ = execute!(out, LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = disable_raw_mode();
    Ok(())
}

/// Format a `Buffer` as a stable, diffable string for snapshot tests.
/// One line per row of cells, plus a separator and a per-cell-style table
/// listing only cells whose style differs from default.
#[must_use]
pub fn buffer_snapshot(buf: &Buffer) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let area = buf.area();
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out.push_str("---\n");
    let default = ratatui::style::Style::default();
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            let s = cell.style();
            if s != default {
                let _ = writeln!(out, "({x},{y}) {s:?}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::config::Config;
    use mx_core::state::State;
    use std::sync::Arc;

    #[test]
    fn draw_to_buffer_is_deterministic() {
        let s = State::new(Arc::new(Config::default()), "/".into(), "/".into());
        let area = Rect { x: 0, y: 0, width: 100, height: 30 };
        let a = draw_to_buffer(&s, area);
        let b = draw_to_buffer(&s, area);
        assert_eq!(a, b, "same state must produce identical buffers");
    }

    #[test]
    fn empty_state_produces_two_panel_titles() {
        let s = State::new(Arc::new(Config::default()), "/foo".into(), "/bar".into());
        let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
        let snap = buffer_snapshot(&buf);
        assert!(snap.contains("/foo"));
        assert!(snap.contains("/bar"));
    }

    #[test]
    fn narrow_state_only_shows_focused_panel_title() {
        let s = State::new(Arc::new(Config::default()), "/foo".into(), "/bar".into());
        let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 60, height: 24 });
        let snap = buffer_snapshot(&buf);
        assert!(snap.contains("/foo"));
        assert!(!snap.contains("/bar"));
    }
}
