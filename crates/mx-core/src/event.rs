//! Event vocabulary delivered to the main loop. Three producers feed
//! `Event`s through the same `mpsc` channel: the input thread, worker
//! threads, and the tick driver. `update()` is the sole consumer.

use std::time::Duration;

use camino::Utf8PathBuf;

use crate::command::CommandId;
use crate::errors::FsError;
use crate::input::InputEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkerId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    FileOverFile,
    FileOverDir,
    DirOverFile,
    DirOverDir,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerMsg {
    Progress {
        bytes_done: u64,
        bytes_total: u64,
        current_path: Utf8PathBuf,
    },
    DirScanned {
        side: crate::state::PanelSide,
        entries: Vec<crate::state::DirEntry>,
    },
    Conflict {
        src: Utf8PathBuf,
        dst: Utf8PathBuf,
        kind: ConflictKind,
    },
    Done,
    Failed {
        errors: Vec<(Utf8PathBuf, FsError)>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Input(InputEvent),
    Command(CommandId),
    Worker(WorkerId, WorkerMsg),
    Tick {
        dt: Duration,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    PreviewLoaded {
        path: camino::Utf8PathBuf,
        body: String,
        truncated: bool,
        binary: bool,
    },
    PreviewFailed {
        path: camino::Utf8PathBuf,
        error: FsError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyChord, KeyCode};

    #[test]
    fn event_can_wrap_input_command_and_tick() {
        let _: Event = Event::Input(InputEvent::Key(KeyChord::bare(KeyCode::Esc)));
        let _: Event = Event::Command(CommandId::Quit);
        let _: Event = Event::Tick {
            dt: Duration::from_millis(100),
        };
    }

    #[test]
    fn worker_id_is_eq() {
        assert_eq!(WorkerId(7), WorkerId(7));
        assert_ne!(WorkerId(7), WorkerId(8));
    }
}
