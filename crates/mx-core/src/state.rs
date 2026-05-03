//! Application state. `update()` consumes and returns this; rendering reads it.
//!
//! In Phase 1 several `Modal` variants and `PanelState` fields exist but are
//! never populated by code paths — they're defined here so the type
//! vocabulary is settled and Phase 2 only adds *use sites*, not new types.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;

use camino::Utf8PathBuf;

use crate::config::Config;
use crate::event::WorkerId;
use crate::input::KeyChord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSide {
    Left,
    Right,
}

impl PanelSide {
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
        }
    }
}

/// One row in a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
    /// `Some` for files (and for resolved symlinks-to-files when followed).
    /// `None` for directories and unreadable entries.
    pub size: Option<u64>,
    /// Modification time, if metadata was readable.
    pub mtime: Option<std::time::SystemTime>,
    /// Set when `kind == Symlink`. The string is what `read_link` returned —
    /// it may be relative to the entry's directory.
    pub symlink_target: Option<String>,
    /// True when `read_link` resolved to a file/dir; false when the target
    /// doesn't exist. Renderers display broken links in `error_fg`.
    pub symlink_broken: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
    Unreadable,
}

impl DirEntry {
    /// `..` row at the top of a panel (above the cwd's children).
    #[must_use]
    pub fn parent() -> Self {
        Self {
            name: "..".to_string(),
            kind: EntryKind::Dir,
            size: None,
            mtime: None,
            symlink_target: None,
            symlink_broken: false,
        }
    }

    #[must_use]
    pub fn dir(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: EntryKind::Dir,
            size: None,
            mtime: None,
            symlink_target: None,
            symlink_broken: false,
        }
    }

    #[must_use]
    pub fn file(name: impl Into<String>, size: u64) -> Self {
        Self {
            name: name.into(),
            kind: EntryKind::File,
            size: Some(size),
            mtime: None,
            symlink_target: None,
            symlink_broken: false,
        }
    }

    #[must_use]
    pub fn symlink(name: impl Into<String>, target: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: EntryKind::Symlink,
            size: None,
            mtime: None,
            symlink_target: Some(target.into()),
            symlink_broken: false,
        }
    }

    #[must_use]
    pub fn is_dir_like(&self) -> bool {
        matches!(self.kind, EntryKind::Dir)
    }

    /// True if the entry should be hidden when `show_hidden = false`.
    /// `..` is never hidden.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.name != ".." && self.name.starts_with('.')
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortMode {
    #[default]
    ByName,
    BySize,
    ByModified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelState {
    pub cwd: Utf8PathBuf,
    pub entries: Arc<[DirEntry]>,
    pub cursor: usize,
    pub scroll: usize,
    pub selection: BTreeSet<usize>,
    pub sort: SortMode,
    pub show_hidden: bool,
    pub loading: bool,
}

impl PanelState {
    #[must_use]
    pub fn empty(cwd: impl Into<Utf8PathBuf>) -> Self {
        Self {
            cwd: cwd.into(),
            entries: Arc::new([]),
            cursor: 0,
            scroll: 0,
            selection: BTreeSet::new(),
            sort: SortMode::default(),
            show_hidden: false,
            loading: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
}

/// Modal stack is `Option` in v1: at most one open at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    Confirm(ConfirmDialog),
    Input(InputDialog),
    Progress(ProgressDialog),
    Error(ErrorDialog),
    Help,
    QuitConfirm,
    Viewer(ViewerDialog),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerDialog {
    pub path: Utf8PathBuf,
    pub body: String,
    pub scroll: usize,
    pub truncated: bool,
    pub binary: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDialog {
    pub title: String,
    pub body: String,
    pub default_yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDialog {
    pub title: String,
    pub prompt: String,
    pub value: String,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressDialog {
    pub title: String,
    pub current_path: Utf8PathBuf,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub worker_id: WorkerId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDialog {
    pub title: String,
    pub body: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerState {
    pub id: WorkerId,
    pub kind: WorkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind {
    Copy,
    Move,
    Delete,
    /// Dir scan is keyed by destination panel so the executor and the
    /// `WorkerMsg::DirScanned` handler know whose `entries` to swap.
    DirScan(PanelSide),
}

#[derive(Debug, Clone)]
pub struct State {
    pub panels: [PanelState; 2],
    pub focus: PanelSide,
    pub modal: Option<Modal>,
    pub status: StatusLine,
    pub workers: HashMap<WorkerId, WorkerState>,
    pub config: Arc<Config>,
    pub pending_chord: Vec<KeyChord>,
    pub pending_since: Option<Instant>,
    pub should_quit: bool,
}

impl State {
    #[must_use]
    pub fn new(config: Arc<Config>, left_cwd: Utf8PathBuf, right_cwd: Utf8PathBuf) -> Self {
        Self {
            panels: [PanelState::empty(left_cwd), PanelState::empty(right_cwd)],
            focus: PanelSide::Left,
            modal: None,
            status: StatusLine::default(),
            workers: HashMap::new(),
            config,
            pending_chord: Vec::new(),
            pending_since: None,
            should_quit: false,
        }
    }

    #[must_use]
    pub fn focused(&self) -> &PanelState {
        &self.panels[self.focus.index()]
    }

    pub fn focused_mut(&mut self) -> &mut PanelState {
        let i = self.focus.index();
        &mut self.panels[i]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> State {
        State::new(Arc::new(Config::default()), "/".into(), "/".into())
    }

    #[test]
    fn new_state_starts_clean() {
        let s = st();
        assert_eq!(s.focus, PanelSide::Left);
        assert!(s.modal.is_none());
        assert!(!s.should_quit);
        assert!(s.pending_chord.is_empty());
    }

    #[test]
    fn panel_side_other_and_index() {
        assert_eq!(PanelSide::Left.other(), PanelSide::Right);
        assert_eq!(PanelSide::Right.index(), 1);
    }

    #[test]
    fn focused_panel_follows_focus() {
        let mut s = st();
        s.panels[0].cursor = 7;
        s.panels[1].cursor = 3;
        assert_eq!(s.focused().cursor, 7);
        s.focus = PanelSide::Right;
        assert_eq!(s.focused().cursor, 3);
    }

    #[test]
    fn dir_entry_constructors() {
        let e = DirEntry::file("a.txt", 1234);
        assert_eq!(e.name, "a.txt");
        assert_eq!(e.kind, EntryKind::File);
        assert_eq!(e.size, Some(1234));
        assert!(e.symlink_target.is_none());

        let d = DirEntry::dir("src");
        assert_eq!(d.kind, EntryKind::Dir);
        assert!(d.size.is_none());

        let s = DirEntry::symlink("link", "/etc/hosts");
        assert_eq!(s.kind, EntryKind::Symlink);
        assert_eq!(s.symlink_target.as_deref(), Some("/etc/hosts"));
    }

    #[test]
    fn parent_dir_entry() {
        let p = DirEntry::parent();
        assert_eq!(p.name, "..");
        assert_eq!(p.kind, EntryKind::Dir);
    }
}
