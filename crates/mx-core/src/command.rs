//! `CommandId` is the named-action vocabulary that keymap, scripting, and any
//! future command palette dispatch through. `update()` matches on `CommandId`,
//! never on raw `KeyCode`s.

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

use crate::event::WorkerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandId {
    // Navigation
    CursorUp,
    CursorDown,
    CursorPageUp,
    CursorPageDown,
    CursorHome,
    CursorEnd,
    EnterDir,
    ParentDir,
    FocusOther,
    SwapPanels,

    // Selection
    ToggleSelect,
    SelectAll,
    SelectNone,
    InvertSelection,

    // File ops
    Copy,
    Move,
    Delete,
    Mkdir,
    Rename,

    // View
    View,
    ToggleHidden,
    CycleSort,

    // App
    Help,
    QuitConfirm,
    Quit,
    Cancel,

    // Refresh
    RescanFocused,
    RescanBoth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RescanDir(crate::state::PanelSide),
    StartCopy {
        src: Vec<Utf8PathBuf>,
        dst: Utf8PathBuf,
    },
    StartMove {
        src: Vec<Utf8PathBuf>,
        dst: Utf8PathBuf,
    },
    StartDelete {
        paths: Vec<Utf8PathBuf>,
    },
    Mkdir {
        parent: Utf8PathBuf,
        name: String,
    },
    Rename {
        from: Utf8PathBuf,
        to: Utf8PathBuf,
    },
    CancelWorker(WorkerId),
    ResolveConflict(WorkerId, OverwritePolicy),
    OpenViewer(Utf8PathBuf),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwritePolicy {
    /// Overwrite this one file.
    Yes,
    /// Skip this file.
    No,
    /// Overwrite this and every subsequent conflict.
    YesAll,
    /// Skip this and every subsequent conflict.
    NoAll,
    /// Cancel the whole operation.
    Cancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_id_deserializes_from_snake_case() {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct W {
            id: CommandId,
        }
        let w: W = toml::from_str(r#"id = "quit_confirm""#).unwrap();
        assert_eq!(w.id, CommandId::QuitConfirm);
    }

    #[test]
    fn command_id_round_trips_through_toml() {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct W {
            id: CommandId,
        }
        let w = W {
            id: CommandId::CycleSort,
        };
        let s = toml::to_string(&w).unwrap();
        assert!(s.contains("cycle_sort"), "got {s}");
        let back: W = toml::from_str(&s).unwrap();
        assert_eq!(w, back);
    }
}
