//! Pure state-transition function. The only mutation in the entire app for
//! everything except the side effects in `Command`s.

// Several match arms intentionally share an empty body — they document
// variants we deliberately leave unhandled in Phase 1. Merging them obscures
// the per-variant Phase 2 intent.
#![allow(clippy::match_same_arms)]

use std::time::Instant;

use crate::command::{Command, CommandId};
use crate::event::Event;
use crate::input::{InputEvent, KeyChord};
use crate::keymap::Lookup;
use crate::state::{Modal, State};

/// Pure transition. Takes ownership of `state`, returns the new state and any
/// `Command`s the executor should run.
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn update(mut state: State, event: Event) -> (State, Vec<Command>) {
    let mut cmds = Vec::new();

    match event {
        Event::Input(InputEvent::Key(chord)) => {
            handle_chord(&mut state, chord, &mut cmds);
        }
        Event::Input(_) => { /* paste / mouse stub: ignored in Phase 1 */ }
        Event::Command(id) => {
            handle_command(&mut state, id, &mut cmds);
        }
        Event::Tick { .. } => {
            // Chord timeout flush: if we've held a pending chord too long,
            // resolve whatever shorter prefix matches, or drop it.
            if let Some(since) = state.pending_since {
                let timeout = state.config.input.chord_timeout();
                if Instant::now().saturating_duration_since(since) >= timeout {
                    flush_pending_chord(&mut state, &mut cmds);
                }
            }
        }
        Event::Worker(id, msg) => {
            handle_worker_msg(&mut state, id, msg);
        }
        Event::Resize { .. } => { /* renderer reads new size next frame */ }
    }

    (state, cmds)
}

fn handle_chord(state: &mut State, chord: KeyChord, cmds: &mut Vec<Command>) {
    state.pending_chord.push(chord);
    let lookup = state.config.keymap.lookup(&state.pending_chord);

    match lookup {
        Lookup::Match(id) => {
            // Could still be a prefix of something longer (Esc → Cancel is
            // also a prefix of Esc-1). Wait if so; Tick flushes on timeout.
            if state.config.keymap.is_strict_prefix(&state.pending_chord) {
                state.pending_since = Some(Instant::now());
            } else {
                state.pending_chord.clear();
                state.pending_since = None;
                cmds.extend(dispatch(state, id));
            }
        }
        Lookup::Prefix => {
            state.pending_since = Some(Instant::now());
        }
        Lookup::NoMatch => {
            // Try the previous (shorter) prefix as a complete match — this is
            // the case where pressing Esc then a non-bound key should still
            // fire Esc → Cancel and treat the new key as fresh.
            let popped = state.pending_chord.pop();
            if let Some(new_key) = popped {
                let prev_lookup = state.config.keymap.lookup(&state.pending_chord);
                if let Lookup::Match(id) = prev_lookup {
                    state.pending_chord.clear();
                    state.pending_since = None;
                    cmds.extend(dispatch(state, id));
                    // re-process the key that broke the prefix as fresh
                    state.pending_chord.push(new_key);
                    let again = state.config.keymap.lookup(&state.pending_chord);
                    match again {
                        Lookup::Match(id2) => {
                            state.pending_chord.clear();
                            state.pending_since = None;
                            cmds.extend(dispatch(state, id2));
                        }
                        Lookup::Prefix => {
                            state.pending_since = Some(Instant::now());
                        }
                        Lookup::NoMatch => {
                            state.pending_chord.clear();
                            state.pending_since = None;
                        }
                    }
                } else {
                    // No prefix match either; drop pending entirely.
                    state.pending_chord.clear();
                    state.pending_since = None;
                }
            }
        }
    }
}

fn handle_worker_msg(
    state: &mut State,
    _id: crate::event::WorkerId,
    msg: crate::event::WorkerMsg,
) {
    use crate::event::WorkerMsg;
    match msg {
        WorkerMsg::DirScanned { side, entries } => {
            let panel = &mut state.panels[side.index()];
            let new_entries: std::sync::Arc<[_]> = entries.into();
            panel.entries = new_entries;
            panel.loading = false;
            let last = panel.entries.len().saturating_sub(1);
            if panel.cursor > last {
                panel.cursor = last;
            }
            if panel.scroll > last {
                panel.scroll = last;
            }
        }
        // Phase 3 surfaces these in modals.
        WorkerMsg::Progress { .. }
        | WorkerMsg::Conflict { .. }
        | WorkerMsg::Done
        | WorkerMsg::Failed { .. } => {}
    }
}

fn flush_pending_chord(state: &mut State, cmds: &mut Vec<Command>) {
    if state.pending_chord.is_empty() {
        state.pending_since = None;
        return;
    }
    let resolution = state.config.keymap.lookup(&state.pending_chord);
    state.pending_chord.clear();
    state.pending_since = None;
    if let Lookup::Match(id) = resolution {
        cmds.extend(dispatch(state, id));
    }
    // Lookup::Prefix or NoMatch on flush: drop silently.
}

fn dispatch(state: &mut State, id: CommandId) -> Vec<Command> {
    // If a modal swallows the input, handle that first.
    if let Some(m) = &state.modal {
        match (m, id) {
            (_, CommandId::Cancel) | (Modal::Help, CommandId::Help) => {
                state.modal = None;
                return Vec::new();
            }
            (Modal::QuitConfirm, CommandId::QuitConfirm | CommandId::Quit) => {
                state.should_quit = true;
                state.modal = None;
                return vec![Command::Quit];
            }
            // Anything else while a modal is open: ignored in Phase 1.
            _ => return Vec::new(),
        }
    }
    handle_command_no_modal(state, id)
}

fn handle_command(state: &mut State, id: CommandId, cmds: &mut Vec<Command>) {
    cmds.extend(dispatch(state, id));
}

fn handle_command_no_modal(state: &mut State, id: CommandId) -> Vec<Command> {
    const PAGE: usize = 10;
    match id {
        CommandId::Quit => {
            state.should_quit = true;
            return vec![Command::Quit];
        }
        CommandId::QuitConfirm => {
            if state.workers.is_empty() {
                state.should_quit = true;
                return vec![Command::Quit];
            }
            state.modal = Some(Modal::QuitConfirm);
        }
        CommandId::Cancel => { /* nothing to cancel without a modal */ }
        CommandId::Help => {
            state.modal = Some(Modal::Help);
        }
        CommandId::FocusOther => {
            state.focus = state.focus.other();
        }
        CommandId::SwapPanels => {
            state.panels.swap(0, 1);
        }

        CommandId::CursorUp       => move_cursor(state, -1),
        CommandId::CursorDown     => move_cursor(state, 1),
        CommandId::CursorPageUp => {
            #[allow(clippy::cast_possible_wrap)]
            move_cursor(state, -(PAGE as isize));
        }
        CommandId::CursorPageDown => {
            #[allow(clippy::cast_possible_wrap)]
            move_cursor(state, PAGE as isize);
        }
        CommandId::CursorHome     => set_cursor(state, 0),
        CommandId::CursorEnd     => {
            let last = state.focused().entries.len().saturating_sub(1);
            set_cursor(state, last);
        }

        // Phase 2 / 3 work below.
        CommandId::EnterDir
        | CommandId::ParentDir
        | CommandId::ToggleSelect
        | CommandId::SelectAll
        | CommandId::SelectNone
        | CommandId::InvertSelection
        | CommandId::Copy
        | CommandId::Move
        | CommandId::Delete
        | CommandId::Mkdir
        | CommandId::Rename
        | CommandId::View
        | CommandId::ToggleHidden
        | CommandId::CycleSort
        | CommandId::RescanFocused
        | CommandId::RescanBoth => {
            // Implemented in later Phase 2 tasks.
        }
    }
    Vec::new()
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn move_cursor(state: &mut State, delta: isize) {
    let panel = state.focused_mut();
    if panel.entries.is_empty() {
        return;
    }
    let last = panel.entries.len() - 1;
    let cur = panel.cursor as isize;
    let next = (cur + delta).clamp(0, last as isize) as usize;
    panel.cursor = next;
}

fn set_cursor(state: &mut State, i: usize) {
    let panel = state.focused_mut();
    if panel.entries.is_empty() {
        return;
    }
    let last = panel.entries.len() - 1;
    panel.cursor = i.min(last);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::event::Event;
    use crate::input::{InputEvent, KeyChord, KeyCode, KeyModifiers};
    use crate::state::{Modal, PanelSide, State};
    use std::sync::Arc;
    use std::time::Duration;

    fn st() -> State {
        State::new(Arc::new(Config::default()), "/".into(), "/".into())
    }

    fn key(c: KeyCode) -> Event {
        Event::Input(InputEvent::Key(KeyChord::bare(c)))
    }
    fn ctrl_key(c: char) -> Event {
        Event::Input(InputEvent::Key(KeyChord::new(
            KeyCode::Char(c),
            KeyModifiers::ctrl(),
        )))
    }

    #[test]
    fn ctrl_q_with_no_workers_quits_immediately() {
        let s = st();
        let (s, cmds) = update(s, ctrl_key('q'));
        assert!(s.should_quit);
        assert_eq!(cmds, vec![Command::Quit]);
    }

    #[test]
    fn f10_opens_quit_confirm_when_workers_present() {
        let mut s = st();
        s.workers.insert(
            crate::event::WorkerId(1),
            crate::state::WorkerState {
                id: crate::event::WorkerId(1),
                kind: crate::state::WorkerKind::Copy,
            },
        );
        let (s, cmds) = update(s, key(KeyCode::F(10)));
        assert!(matches!(s.modal, Some(Modal::QuitConfirm)));
        assert!(!s.should_quit);
        assert!(cmds.is_empty());
    }

    #[test]
    fn esc_closes_modal_when_open() {
        let mut s = st();
        s.modal = Some(Modal::Help);
        let (s, _) = update(s, key(KeyCode::Esc));
        // Esc-alone is also a prefix of esc-N. The chord engine waits.
        assert!(
            s.modal.is_some(),
            "esc must wait for chord_timeout before closing"
        );
        // Now flush via a Tick after chord_timeout.
        let one_sec_ago = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("monotonic clock supports 1s subtraction");
        let s2 = State {
            pending_since: Some(one_sec_ago),
            ..s
        };
        let (s3, _) = update(
            s2,
            Event::Tick {
                dt: Duration::from_millis(100),
            },
        );
        assert!(
            s3.modal.is_none(),
            "after timeout, esc resolves and closes the modal"
        );
    }

    #[test]
    fn tab_toggles_focus() {
        let s = st();
        let (s, _) = update(s, key(KeyCode::Tab));
        assert_eq!(s.focus, PanelSide::Right);
        let (s, _) = update(s, key(KeyCode::Tab));
        assert_eq!(s.focus, PanelSide::Left);
    }

    #[test]
    fn f1_opens_and_closes_help() {
        let s = st();
        let (s, _) = update(s, key(KeyCode::F(1)));
        assert!(matches!(s.modal, Some(Modal::Help)));
        let (s, _) = update(s, key(KeyCode::F(1)));
        assert!(s.modal.is_none());
    }

    #[test]
    fn esc_5_chord_fires_copy() {
        let s = st();
        // First Esc → pending (because Esc is a prefix of Esc-N).
        let (s, _) = update(s, key(KeyCode::Esc));
        assert_eq!(s.pending_chord.len(), 1);
        // Then '5' completes Esc-5 = Copy. Phase 1 update() doesn't *do*
        // anything with Copy yet, but the pending chord must clear.
        let (s, _) = update(s, key(KeyCode::Char('5')));
        assert!(s.pending_chord.is_empty());
    }

    #[test]
    fn unknown_key_does_not_panic() {
        let s = st();
        let (_, cmds) = update(s, key(KeyCode::F(11)));
        assert!(cmds.is_empty());
    }

    fn st_with_entries(n: usize) -> State {
        let mut s = st();
        let entries: Vec<crate::state::DirEntry> = (0..n)
            .map(|i| crate::state::DirEntry::file(format!("f{i:03}"), i as u64))
            .collect();
        s.panels[0].entries = entries.into();
        s
    }

    #[test]
    fn cursor_down_increments_within_bounds() {
        let s = st_with_entries(5);
        let (s, _) = update(s, Event::Command(CommandId::CursorDown));
        assert_eq!(s.panels[0].cursor, 1);
        let (s, _) = update(s, Event::Command(CommandId::CursorDown));
        assert_eq!(s.panels[0].cursor, 2);
    }

    #[test]
    fn cursor_down_clamps_at_last_entry() {
        let mut s = st_with_entries(3);
        s.panels[0].cursor = 2;
        let (s, _) = update(s, Event::Command(CommandId::CursorDown));
        assert_eq!(s.panels[0].cursor, 2);
    }

    #[test]
    fn cursor_up_clamps_at_zero() {
        let s = st_with_entries(5);
        let (s, _) = update(s, Event::Command(CommandId::CursorUp));
        assert_eq!(s.panels[0].cursor, 0);
    }

    #[test]
    fn cursor_home_and_end() {
        let mut s = st_with_entries(20);
        s.panels[0].cursor = 5;
        let (s, _) = update(s, Event::Command(CommandId::CursorEnd));
        assert_eq!(s.panels[0].cursor, 19);
        let (s, _) = update(s, Event::Command(CommandId::CursorHome));
        assert_eq!(s.panels[0].cursor, 0);
    }

    #[test]
    fn page_down_then_page_up_returns_to_start() {
        let s = st_with_entries(50);
        let (s, _) = update(s, Event::Command(CommandId::CursorPageDown));
        assert_eq!(s.panels[0].cursor, 10);
        let (s, _) = update(s, Event::Command(CommandId::CursorPageUp));
        assert_eq!(s.panels[0].cursor, 0);
    }

    #[test]
    fn dir_scanned_replaces_panel_entries_and_clamps_cursor() {
        use crate::event::{WorkerId, WorkerMsg};
        let mut s = st();
        s.panels[0].cursor = 999;
        let entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("a.txt", 10),
            crate::state::DirEntry::file("b.txt", 20),
        ];
        let ev = Event::Worker(
            WorkerId(1),
            WorkerMsg::DirScanned { side: PanelSide::Left, entries },
        );
        let (s, cmds) = update(s, ev);
        assert_eq!(s.panels[0].entries.len(), 3);
        assert_eq!(s.panels[0].cursor, 2);
        assert!(!s.panels[0].loading);
        assert!(cmds.is_empty());
    }
}
