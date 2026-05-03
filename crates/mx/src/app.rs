//! Main event loop. Owns the State, the Renderer, the input thread, and the
//! `mpsc` channel. Translates raw `Command`s into terminal-mode changes and
//! exit signaling. Phase 1 handles only the `Quit` command.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use camino::Utf8PathBuf;

use mx_core::command::Command;
use mx_core::config::Config;
use mx_core::event::Event;
use mx_core::state::State;
use mx_core::update;
use mx_tui::input_thread;
use mx_tui::renderer;

const TICK: Duration = Duration::from_millis(100);

/// Run the main event loop. Returns when the user quits.
///
/// # Errors
///
/// Propagates io errors from terminal acquisition / rendering.
pub fn run(config: Config) -> Result<()> {
    let cfg = Arc::new(config);

    let cwd: Utf8PathBuf = std::env::current_dir()
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| Utf8PathBuf::from("/"));

    let mut state = State::new(Arc::clone(&cfg), cwd.clone(), cwd);

    let (tx, rx) = mpsc::channel::<Event>();
    let input = input_thread::spawn(tx);

    let mut renderer = renderer::acquire()?;
    // Render initial frame so the user sees something immediately.
    let _ = renderer.draw(&state, Duration::ZERO);

    let mut last_frame = Instant::now();
    'main: loop {
        let event = match rx.recv_timeout(TICK) {
            Ok(ev) => ev,
            Err(RecvTimeoutError::Timeout) => Event::Tick { dt: TICK },
            Err(RecvTimeoutError::Disconnected) => break 'main,
        };

        let (new_state, cmds) = update::update(state, event);
        state = new_state;

        for cmd in cmds {
            if matches!(cmd, Command::Quit) {
                break 'main;
            }
            // Phase 2 wires StartCopy / StartMove / StartDelete / etc.
            // through an Executor. None of those exist yet, so other
            // Command variants are unreachable in Phase 1 and ignored.
        }

        if state.should_quit {
            break 'main;
        }

        let now = Instant::now();
        let dt = now.saturating_duration_since(last_frame);
        last_frame = now;
        let _ = renderer.draw(&state, dt);
    }

    let _ = renderer::release();
    input.stop();
    Ok(())
}
