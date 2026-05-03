//! Main event loop. Owns the State, the Renderer, the input thread, the
//! Executor, and the `mpsc` channel.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use camino::Utf8PathBuf;

use mx_core::command::Command;
use mx_core::config::Config;
use mx_core::event::Event;
use mx_core::state::{PanelSide, State};
use mx_core::update;
use mx_fs::executor::Executor;
use mx_preview::{PlainTextPreviewer, Previewer};
use mx_tui::input_thread;
use mx_tui::renderer;

const TICK: Duration = Duration::from_millis(100);

/// Run the main event loop.
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
    state.panels[0].loading = true;
    state.panels[1].loading = true;

    let (tx, rx) = mpsc::channel::<Event>();
    let input = input_thread::spawn(tx.clone());

    let mut executor = Executor::new(tx.clone());
    let previewer = PlainTextPreviewer::default();

    schedule_rescan(&mut executor, &state, PanelSide::Left);
    schedule_rescan(&mut executor, &state, PanelSide::Right);

    let mut renderer = renderer::acquire()?;
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
            match cmd {
                Command::Quit => break 'main,
                Command::RescanDir(side) => schedule_rescan(&mut executor, &state, side),
                Command::OpenViewer(path) => run_preview(&previewer, &tx, path),
                Command::StartCopy { .. }
                | Command::StartMove { .. }
                | Command::StartDelete { .. }
                | Command::Mkdir { .. }
                | Command::Rename { .. }
                | Command::CancelWorker(_) => { /* Phase 3 */ }
            }
        }

        if state.should_quit {
            break 'main;
        }

        executor.reap();

        let now = Instant::now();
        let dt = now.saturating_duration_since(last_frame);
        last_frame = now;
        let _ = renderer.draw(&state, dt);
    }

    executor.join_all();
    let _ = renderer::release();
    input.stop();
    Ok(())
}

fn schedule_rescan(executor: &mut Executor, state: &State, side: PanelSide) {
    let panel = &state.panels[side.index()];
    let _ = executor.start_dir_scan(side, panel.cwd.clone(), panel.show_hidden, panel.sort);
}

fn run_preview(previewer: &PlainTextPreviewer, tx: &Sender<Event>, path: Utf8PathBuf) {
    match previewer.preview(&path) {
        Ok(p) => {
            let _ = tx.send(Event::PreviewLoaded {
                path,
                body: p.body,
                truncated: p.truncated,
                binary: p.binary,
            });
        }
        Err(e) => {
            let _ = tx.send(Event::PreviewFailed { path, error: e });
        }
    }
}
