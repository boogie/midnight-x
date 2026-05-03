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
use crate::state::{Modal, PanelSide, State};

/// Conservative guess at typical terminal viewport height (in rows). Used
/// only by the post-`..` cursor centering heuristic; the renderer owns the
/// real geometry. Larger than [`PAGE`](handle_command_no_modal) on purpose.
const FOCUS_VIEWPORT_HINT: usize = 24;

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
            handle_worker_msg(&mut state, id, msg, &mut cmds);
        }
        Event::Resize { .. } => { /* renderer reads new size next frame */ }
        Event::PreviewLoaded {
            path,
            body,
            truncated,
            binary,
        } => {
            if let Some(crate::state::Modal::Viewer(v)) = &mut state.modal {
                if v.path == path {
                    v.body = body;
                    v.truncated = truncated;
                    v.binary = binary;
                    v.loading = false;
                }
            }
        }
        Event::PreviewFailed { path, error } => {
            if let Some(crate::state::Modal::Viewer(v)) = &mut state.modal {
                if v.path == path {
                    v.body = format!("error: {error}");
                    v.loading = false;
                }
            }
        }
        Event::OpFailed { title, path, error } => {
            use crate::state::{ErrorDialog, Modal};
            state.modal = Some(Modal::Error(ErrorDialog {
                title,
                body: format!("{path}: {error}"),
                details: Vec::new(),
            }));
        }
    }

    (state, cmds)
}

fn handle_chord(state: &mut State, chord: KeyChord, cmds: &mut Vec<Command>) {
    // Typing into an Input modal short-circuits the keymap. Anything we
    // don't consume (Esc, Tab, Enter, function keys) bubbles up.
    if let Some(Modal::Input(d)) = state.modal.as_mut() {
        if input_modal_consume_key(d, chord) {
            return;
        }
    }
    // Same for the path field of an OpDialog.
    if let Some(Modal::Op(d)) = state.modal.as_mut() {
        if op_modal_consume_key(d, chord) {
            return;
        }
    }

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
    id: crate::event::WorkerId,
    msg: crate::event::WorkerMsg,
    cmds: &mut Vec<Command>,
) {
    use crate::event::WorkerMsg;
    use crate::state::{ErrorDialog, Modal, ProgressDialog};
    match msg {
        WorkerMsg::DirScanned { side, entries } => {
            let panel = &mut state.panels[side.index()];
            let new_entries: std::sync::Arc<[_]> = entries.into();
            panel.entries = new_entries;
            panel.loading = false;
            // Honor any "land on this entry name" request, e.g. after `..`.
            // When the entry is found, also center the viewport on it so the
            // user gets context above and below — not the cursor jammed at
            // the bottom of the list.
            if let Some(want) = panel.pending_focus_name.take() {
                if let Some(idx) = panel.entries.iter().position(|e| e.name == want) {
                    panel.cursor = idx;
                    // Renderer owns the actual viewport height; use a
                    // conservative half-page heuristic so the focused entry
                    // ends up roughly in the middle of typical terminals.
                    panel.scroll = idx.saturating_sub(FOCUS_VIEWPORT_HINT / 2);
                }
            }
            let last = panel.entries.len().saturating_sub(1);
            if panel.cursor > last {
                panel.cursor = last;
            }
            if panel.scroll > last {
                panel.scroll = last;
            }
        }
        WorkerMsg::Progress {
            bytes_done,
            bytes_total,
            current_path,
        } => {
            let need_open =
                !matches!(state.modal, Some(Modal::Progress(ref p)) if p.worker_id == id);
            if need_open {
                state.modal = Some(Modal::Progress(ProgressDialog {
                    title: "Working…".into(),
                    current_path,
                    bytes_done,
                    bytes_total,
                    worker_id: id,
                }));
            } else if let Some(Modal::Progress(p)) = state.modal.as_mut() {
                p.bytes_done = bytes_done;
                p.bytes_total = bytes_total;
                p.current_path = current_path;
            }
        }
        WorkerMsg::Done => {
            if let Some(Modal::Progress(p)) = state.modal.as_ref() {
                if p.worker_id == id {
                    state.modal = None;
                }
            }
            schedule_post_op_rescan(state, id, cmds);
        }
        WorkerMsg::Failed { errors } => {
            state.modal = Some(Modal::Error(ErrorDialog {
                title: "Operation failed".into(),
                body: format!("{} error(s)", errors.len()),
                details: errors
                    .iter()
                    .take(20)
                    .map(|(p, e)| format!("{p}: {e}"))
                    .collect(),
            }));
            // Even on failure, the panels may have changed. Rescan affected
            // sides so the UI reflects partial successes.
            schedule_post_op_rescan(state, id, cmds);
        }
        WorkerMsg::Conflict { src, dst, kind } => {
            use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind};
            let body = format!("{src}\nalready exists at\n{dst}\n({kind:?})");
            state.modal = Some(Modal::Confirm(ConfirmDialog {
                title: "Overwrite?".into(),
                body,
                buttons: vec![
                    ConfirmButton::Yes,
                    ConfirmButton::No,
                    ConfirmButton::YesAll,
                    ConfirmButton::NoAll,
                    ConfirmButton::Cancel,
                ],
                focused: 1,
                kind: ConfirmKind::Conflict { worker: id },
            }));
        }
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

#[allow(clippy::too_many_lines)] // single dispatch chokepoint; splitting hurts readability
fn dispatch(state: &mut State, id: CommandId) -> Vec<Command> {
    // Input modal: anything not consumed by typing (Esc, Tab, Enter) lands
    // here. Esc/Cancel closes; Enter (=EnterDir) submits.
    if let Some(Modal::Input(_)) = state.modal.as_ref() {
        match id {
            CommandId::Cancel => {
                state.modal = None;
                return Vec::new();
            }
            CommandId::EnterDir => {
                return resolve_input(state);
            }
            _ => return Vec::new(),
        }
    }

    // Viewer modal handles scroll commands directly.
    if let Some(Modal::Viewer(v)) = &mut state.modal {
        match id {
            CommandId::CursorDown => {
                v.scroll = v.scroll.saturating_add(1);
                return Vec::new();
            }
            CommandId::CursorUp => {
                v.scroll = v.scroll.saturating_sub(1);
                return Vec::new();
            }
            CommandId::CursorPageDown => {
                v.scroll = v.scroll.saturating_add(20);
                return Vec::new();
            }
            CommandId::CursorPageUp => {
                v.scroll = v.scroll.saturating_sub(20);
                return Vec::new();
            }
            CommandId::CursorHome => {
                v.scroll = 0;
                return Vec::new();
            }
            CommandId::CursorEnd => {
                v.scroll = usize::MAX;
                return Vec::new();
            }
            _ => {}
        }
    }

    // OpDialog: Tab cycles focus across path field + buttons. Enter on
    // the path field advances to first button; Enter on a button submits.
    if let Some(Modal::Op(_)) = state.modal.as_ref() {
        match id {
            CommandId::FocusOther => {
                if let Some(Modal::Op(d)) = state.modal.as_mut() {
                    let n_buttons = d.buttons.len();
                    d.focus = match d.focus {
                        crate::state::OpFocus::Path => crate::state::OpFocus::Button(0),
                        crate::state::OpFocus::Button(i) if i + 1 < n_buttons => {
                            crate::state::OpFocus::Button(i + 1)
                        }
                        crate::state::OpFocus::Button(_) => crate::state::OpFocus::Path,
                    };
                }
                return Vec::new();
            }
            CommandId::Cancel => {
                state.modal = None;
                return Vec::new();
            }
            CommandId::EnterDir => {
                return resolve_op(state);
            }
            _ => return Vec::new(),
        }
    }

    // Confirm modal: Tab cycles focus, Enter resolves, Esc cancels.
    if let Some(Modal::Confirm(_)) = state.modal.as_ref() {
        match id {
            CommandId::FocusOther => {
                if let Some(Modal::Confirm(d)) = state.modal.as_mut() {
                    if !d.buttons.is_empty() {
                        d.focused = (d.focused + 1) % d.buttons.len();
                    }
                }
                return Vec::new();
            }
            CommandId::Cancel => {
                state.modal = None;
                return Vec::new();
            }
            CommandId::EnterDir => {
                return resolve_confirm(state);
            }
            _ => return Vec::new(),
        }
    }

    // If a modal swallows the input, handle that first.
    if let Some(m) = &state.modal {
        match (m, id) {
            (Modal::Viewer(_), CommandId::View | CommandId::Cancel) => {
                state.modal = None;
                return Vec::new();
            }
            (_, CommandId::Cancel) | (Modal::Help, CommandId::Help) => {
                state.modal = None;
                return Vec::new();
            }
            (Modal::QuitConfirm, CommandId::QuitConfirm | CommandId::Quit) => {
                state.should_quit = true;
                state.modal = None;
                return vec![Command::Quit];
            }
            // Anything else while a modal is open: ignored.
            _ => return Vec::new(),
        }
    }
    handle_command_no_modal(state, id)
}

fn handle_command(state: &mut State, id: CommandId, cmds: &mut Vec<Command>) {
    cmds.extend(dispatch(state, id));
}

#[allow(clippy::too_many_lines)] // single dispatch point; splitting hurts readability
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

        CommandId::CursorUp => move_cursor(state, -1),
        CommandId::CursorDown => move_cursor(state, 1),
        CommandId::CursorPageUp => {
            #[allow(clippy::cast_possible_wrap)]
            move_cursor(state, -(PAGE as isize));
        }
        CommandId::CursorPageDown => {
            #[allow(clippy::cast_possible_wrap)]
            move_cursor(state, PAGE as isize);
        }
        CommandId::CursorHome => set_cursor(state, 0),
        CommandId::CursorEnd => {
            let last = state.focused().entries.len().saturating_sub(1);
            set_cursor(state, last);
        }

        CommandId::EnterDir => {
            let side = state.focus;
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let entry = &panel.entries[panel.cursor];
            if !entry.is_dir_like() {
                return Vec::new();
            }
            if entry.name == ".." {
                let parent = match panel.cwd.parent() {
                    Some(p) if !p.as_str().is_empty() => p.to_path_buf(),
                    _ => return Vec::new(),
                };
                let focus = panel.cwd.file_name().unwrap_or("").to_string();
                if focus.is_empty() {
                    cd_to(state, side, parent);
                } else {
                    cd_to_with_focus(state, side, parent, focus);
                }
            } else {
                let target = panel.cwd.join(&entry.name);
                cd_to(state, side, target);
            }
            return vec![Command::RescanDir(side)];
        }
        CommandId::ParentDir => {
            let side = state.focus;
            let panel = state.focused();
            let parent = match panel.cwd.parent() {
                Some(p) if !p.as_str().is_empty() => p.to_path_buf(),
                _ => return Vec::new(),
            };
            let focus = panel.cwd.file_name().unwrap_or("").to_string();
            if focus.is_empty() {
                cd_to(state, side, parent);
            } else {
                cd_to_with_focus(state, side, parent, focus);
            }
            return vec![Command::RescanDir(side)];
        }

        CommandId::ToggleHidden => {
            let side = state.focus;
            let new = !state.focused().show_hidden;
            state.focused_mut().show_hidden = new;
            return vec![Command::RescanDir(side)];
        }
        CommandId::CycleSort => {
            use crate::state::SortMode;
            let side = state.focus;
            let panel = state.focused_mut();
            panel.sort = match panel.sort {
                SortMode::ByName => SortMode::BySize,
                SortMode::BySize => SortMode::ByModified,
                SortMode::ByModified => SortMode::ByName,
            };
            return vec![Command::RescanDir(side)];
        }
        CommandId::RescanFocused => {
            let side = state.focus;
            state.panels[side.index()].loading = true;
            return vec![Command::RescanDir(side)];
        }
        CommandId::RescanBoth => {
            state.panels[0].loading = true;
            state.panels[1].loading = true;
            return vec![
                Command::RescanDir(PanelSide::Left),
                Command::RescanDir(PanelSide::Right),
            ];
        }
        CommandId::ToggleSelect => {
            let panel = state.focused_mut();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let i = panel.cursor;
            if panel.entries[i].name == ".." {
                return Vec::new();
            }
            if !panel.selection.insert(i) {
                panel.selection.remove(&i);
            }
        }
        CommandId::SelectAll => {
            let panel = state.focused_mut();
            for (i, e) in panel.entries.iter().enumerate() {
                if e.name != ".." {
                    panel.selection.insert(i);
                }
            }
        }
        CommandId::SelectNone => {
            state.focused_mut().selection.clear();
        }
        CommandId::InvertSelection => {
            let panel = state.focused_mut();
            let n = panel.entries.len();
            for i in 0..n {
                if panel.entries[i].name == ".." {
                    continue;
                }
                if !panel.selection.insert(i) {
                    panel.selection.remove(&i);
                }
            }
        }

        CommandId::View => {
            use crate::state::{EntryKind, Modal, ViewerDialog};
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let entry = &panel.entries[panel.cursor];
            if entry.kind != EntryKind::File && entry.kind != EntryKind::Symlink {
                return Vec::new();
            }
            let path = panel.cwd.join(&entry.name);
            state.modal = Some(Modal::Viewer(ViewerDialog {
                path: path.clone(),
                body: "loading…".into(),
                scroll: 0,
                truncated: false,
                binary: false,
                loading: true,
            }));
            return vec![Command::OpenViewer(path)];
        }

        CommandId::Mkdir => {
            use crate::state::{InputDialog, InputKind, Modal};
            let parent = state.focused().cwd.clone();
            state.modal = Some(Modal::Input(InputDialog {
                title: "Make directory".into(),
                prompt: "Enter the new directory name:".into(),
                value: String::new(),
                cursor: 0,
                kind: InputKind::Mkdir { parent },
            }));
        }
        CommandId::Rename => {
            use crate::state::{InputDialog, InputKind, Modal};
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let entry = &panel.entries[panel.cursor];
            if entry.name == ".." {
                return Vec::new();
            }
            let from = panel.cwd.join(&entry.name);
            let value = entry.name.clone();
            let cursor = value.len();
            state.modal = Some(Modal::Input(InputDialog {
                title: "Rename".into(),
                prompt: format!("New name for {}:", entry.name),
                value,
                cursor,
                kind: InputKind::Rename { from },
            }));
        }

        CommandId::Delete => {
            use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let paths = collect_targets(panel);
            if paths.is_empty() {
                return Vec::new();
            }
            let body = if paths.len() == 1 {
                let name = paths[0]
                    .file_name()
                    .map_or_else(|| paths[0].to_string(), str::to_string);
                format!("Do you wish to delete the file\n{name}")
            } else {
                format!("Do you wish to delete {} items", paths.len())
            };
            state.modal = Some(Modal::Confirm(ConfirmDialog {
                title: "Delete".into(),
                body,
                buttons: vec![ConfirmButton::Delete, ConfirmButton::Cancel],
                focused: 0, // Far-faithful: default focus on the destructive action.
                kind: ConfirmKind::Delete { paths },
            }));
        }

        CommandId::Copy => {
            if let Some(d) = build_op_dialog(state, OpVerb::Copy) {
                state.modal = Some(crate::state::Modal::Op(d));
            }
        }

        CommandId::Move => {
            if let Some(d) = build_op_dialog(state, OpVerb::Move) {
                state.modal = Some(crate::state::Modal::Op(d));
            }
        }
    }
    Vec::new()
}

#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
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

fn cd_to(state: &mut State, side: PanelSide, dir: camino::Utf8PathBuf) {
    let panel = &mut state.panels[side.index()];
    panel.cwd = dir;
    panel.entries = std::sync::Arc::new([]);
    panel.cursor = 0;
    panel.scroll = 0;
    panel.selection.clear();
    panel.loading = true;
}

/// Pop the worker entry, mark its affected panels as loading, and emit a
/// `RescanDir` per side. Called from both `Done` and `Failed` paths so the
/// UI always reflects the post-op filesystem state.
fn schedule_post_op_rescan(state: &mut State, id: crate::event::WorkerId, cmds: &mut Vec<Command>) {
    let Some(worker) = state.workers.remove(&id) else {
        return;
    };
    for side in worker.affected_sides {
        state.panels[side.index()].loading = true;
        cmds.push(Command::RescanDir(side));
    }
}

#[derive(Clone, Copy)]
enum OpVerb {
    Copy,
    Move,
}

fn build_op_dialog(state: &State, verb: OpVerb) -> Option<crate::state::OpDialog> {
    use crate::state::{ConfirmButton, OpDialog, OpFocus, OpKind};
    let panel = state.focused();
    if panel.entries.is_empty() {
        return None;
    }
    let src = collect_targets(panel);
    if src.is_empty() {
        return None;
    }
    let other = state.focus.other();
    // Pre-fill with the destination *directory* only. The worker appends each
    // source's basename, so including the basename here would double it.
    let target = state.panels[other.index()].cwd.to_string();
    let cursor = target.len();
    let (title, prompt, action_button, kind) = match verb {
        OpVerb::Copy => {
            let prompt = if src.len() == 1 {
                format!("Copy {} to:", src[0].file_name().unwrap_or(src[0].as_str()))
            } else {
                format!("Copy {} items to:", src.len())
            };
            (
                "Copy".to_string(),
                prompt,
                ConfirmButton::Copy,
                OpKind::Copy { src },
            )
        }
        OpVerb::Move => {
            let prompt = if src.len() == 1 {
                format!("Move {} to:", src[0].file_name().unwrap_or(src[0].as_str()))
            } else {
                format!("Move {} items to:", src.len())
            };
            (
                "Move".to_string(),
                prompt,
                ConfirmButton::Move,
                OpKind::Move { src },
            )
        }
    };
    Some(OpDialog {
        title,
        prompt,
        target,
        cursor,
        focus: OpFocus::Path,
        buttons: vec![action_button, ConfirmButton::Cancel],
        kind,
    })
}

/// Consume a key when an `OpDialog` is open and focus is on the path field.
/// Returns `true` when the key was handled (and should not bubble to the
/// keymap).
fn op_modal_consume_key(d: &mut crate::state::OpDialog, c: KeyChord) -> bool {
    use crate::input::KeyCode;
    use crate::state::OpFocus;
    if !matches!(d.focus, OpFocus::Path) {
        return false;
    }
    if c.mods.ctrl || c.mods.alt {
        return false;
    }
    match c.code {
        KeyCode::Char(ch) => {
            d.target.insert(d.cursor, ch);
            d.cursor += ch.len_utf8();
            true
        }
        KeyCode::Backspace => {
            if d.cursor > 0 {
                let new_cursor = floor_char_boundary(&d.target, d.cursor - 1);
                d.target.replace_range(new_cursor..d.cursor, "");
                d.cursor = new_cursor;
            }
            true
        }
        KeyCode::Delete => {
            if d.cursor < d.target.len() {
                let next = ceil_char_boundary(&d.target, d.cursor + 1);
                d.target.replace_range(d.cursor..next, "");
            }
            true
        }
        KeyCode::Left => {
            if d.cursor > 0 {
                d.cursor = floor_char_boundary(&d.target, d.cursor - 1);
            }
            true
        }
        KeyCode::Right => {
            if d.cursor < d.target.len() {
                d.cursor = ceil_char_boundary(&d.target, d.cursor + 1);
            }
            true
        }
        KeyCode::Home => {
            d.cursor = 0;
            true
        }
        KeyCode::End => {
            d.cursor = d.target.len();
            true
        }
        _ => false,
    }
}

fn resolve_op(state: &mut State) -> Vec<Command> {
    use crate::state::{ConfirmButton, Modal, OpFocus, OpKind};
    let Some(Modal::Op(d)) = state.modal.take() else {
        return Vec::new();
    };
    // Path-focus + Enter fires the default (first) button — same as if the
    // user had Tab'd to it and hit Enter.
    let idx = match d.focus {
        OpFocus::Path => 0,
        OpFocus::Button(i) => i,
    };
    let button = d.buttons.get(idx).copied();
    // Non-action buttons (Cancel) close the modal without emitting a Command.
    if !matches!(
        button,
        Some(ConfirmButton::Copy | ConfirmButton::Move | ConfirmButton::Yes)
    ) {
        return Vec::new();
    }
    let dst = camino::Utf8PathBuf::from(d.target.trim());
    if dst.as_str().is_empty() {
        // Empty target: re-open the modal with focus back on the path so
        // the user can type one in.
        let mut d = d;
        d.focus = OpFocus::Path;
        state.modal = Some(Modal::Op(d));
        return Vec::new();
    }
    match (d.kind, button) {
        (OpKind::Copy { src }, _) => vec![Command::StartCopy { src, dst }],
        (OpKind::Move { src }, _) => vec![Command::StartMove { src, dst }],
    }
}

fn collect_targets(panel: &crate::state::PanelState) -> Vec<camino::Utf8PathBuf> {
    let mut out = Vec::new();
    if !panel.selection.is_empty() {
        for &i in &panel.selection {
            if let Some(e) = panel.entries.get(i) {
                if e.name == ".." {
                    continue;
                }
                out.push(panel.cwd.join(&e.name));
            }
        }
    } else if let Some(e) = panel.entries.get(panel.cursor) {
        if e.name != ".." {
            out.push(panel.cwd.join(&e.name));
        }
    }
    out
}

fn input_modal_consume_key(d: &mut crate::state::InputDialog, c: KeyChord) -> bool {
    use crate::input::KeyCode;
    if c.mods.ctrl || c.mods.alt {
        return false;
    }
    match c.code {
        KeyCode::Char(ch) => {
            d.value.insert(d.cursor, ch);
            d.cursor += ch.len_utf8();
            true
        }
        KeyCode::Backspace => {
            if d.cursor > 0 {
                let new_cursor = floor_char_boundary(&d.value, d.cursor - 1);
                d.value.replace_range(new_cursor..d.cursor, "");
                d.cursor = new_cursor;
            }
            true
        }
        KeyCode::Delete => {
            if d.cursor < d.value.len() {
                let next = ceil_char_boundary(&d.value, d.cursor + 1);
                d.value.replace_range(d.cursor..next, "");
            }
            true
        }
        KeyCode::Left => {
            if d.cursor > 0 {
                d.cursor = floor_char_boundary(&d.value, d.cursor - 1);
            }
            true
        }
        KeyCode::Right => {
            if d.cursor < d.value.len() {
                d.cursor = ceil_char_boundary(&d.value, d.cursor + 1);
            }
            true
        }
        KeyCode::Home => {
            d.cursor = 0;
            true
        }
        KeyCode::End => {
            d.cursor = d.value.len();
            true
        }
        _ => false,
    }
}

fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn resolve_input(state: &mut State) -> Vec<Command> {
    use crate::state::{InputKind, Modal};
    let Some(Modal::Input(d)) = state.modal.take() else {
        return Vec::new();
    };
    let value = d.value.trim().to_string();
    if value.is_empty() {
        return Vec::new();
    }
    match d.kind {
        InputKind::Mkdir { parent } => vec![Command::Mkdir {
            parent,
            name: value,
        }],
        InputKind::Rename { from } => {
            let parent = from
                .parent()
                .map(camino::Utf8Path::to_path_buf)
                .unwrap_or_default();
            let to = if parent.as_str().is_empty() {
                camino::Utf8PathBuf::from(value)
            } else {
                parent.join(value)
            };
            vec![Command::Rename { from, to }]
        }
    }
}

fn resolve_confirm(state: &mut State) -> Vec<Command> {
    use crate::command::OverwritePolicy as P;
    use crate::state::{ConfirmButton, ConfirmKind, Modal};
    let Some(Modal::Confirm(d)) = state.modal.take() else {
        return Vec::new();
    };
    let button = d.buttons.get(d.focused).copied();
    match (d.kind, button) {
        (ConfirmKind::Delete { paths }, Some(ConfirmButton::Yes | ConfirmButton::Delete)) => {
            vec![Command::StartDelete { paths }]
        }
        (ConfirmKind::StartCopy { src, dst }, Some(ConfirmButton::Yes | ConfirmButton::Copy)) => {
            vec![Command::StartCopy { src, dst }]
        }
        (ConfirmKind::StartMove { src, dst }, Some(ConfirmButton::Yes | ConfirmButton::Move)) => {
            vec![Command::StartMove { src, dst }]
        }
        (ConfirmKind::Conflict { worker }, Some(b)) => {
            let policy = match b {
                ConfirmButton::Yes => P::Yes,
                ConfirmButton::No => P::No,
                ConfirmButton::YesAll => P::YesAll,
                ConfirmButton::NoAll => P::NoAll,
                ConfirmButton::Cancel | ConfirmButton::Ok => P::Cancel,
                ConfirmButton::Delete | ConfirmButton::Copy | ConfirmButton::Move => P::Cancel,
            };
            vec![Command::ResolveConflict(worker, policy)]
        }
        (ConfirmKind::QuitWithWorkers, Some(ConfirmButton::Yes)) => {
            state.should_quit = true;
            vec![Command::Quit]
        }
        // No / Cancel / no button focused: just close.
        _ => Vec::new(),
    }
}

/// `cd` to `dir`, asking the `DirScanned` handler to place the cursor on
/// the entry named `focus` once the scan completes. Used when navigating up
/// so the user lands on the directory they just left.
fn cd_to_with_focus(state: &mut State, side: PanelSide, dir: camino::Utf8PathBuf, focus: String) {
    cd_to(state, side, dir);
    state.panels[side.index()].pending_focus_name = Some(focus);
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
                affected_sides: vec![PanelSide::Right],
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
        // Push pending_since well past the chord timeout (now 1500 ms).
        let long_ago = Instant::now()
            .checked_sub(Duration::from_secs(3))
            .expect("monotonic clock supports 3s subtraction");
        let s2 = State {
            pending_since: Some(long_ago),
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

    #[test]
    fn confirm_modal_tab_cycles_focus() {
        use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Confirm(ConfirmDialog {
            title: "x".into(),
            body: "y".into(),
            buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
            focused: 0,
            kind: ConfirmKind::Delete {
                paths: vec!["/x".into()],
            },
        }));
        let (s, _) = update(s, key(KeyCode::Tab));
        match s.modal {
            Some(Modal::Confirm(ref d)) => assert_eq!(d.focused, 1),
            _ => panic!(),
        }
        let (s, _) = update(s, key(KeyCode::Tab));
        match s.modal {
            Some(Modal::Confirm(ref d)) => assert_eq!(d.focused, 0),
            _ => panic!(),
        }
    }

    #[test]
    fn confirm_modal_enter_on_yes_emits_start_delete() {
        use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Confirm(ConfirmDialog {
            title: "Delete?".into(),
            body: "1 file".into(),
            buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
            focused: 0,
            kind: ConfirmKind::Delete {
                paths: vec!["/x/y.txt".into()],
            },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
        assert_eq!(
            cmds,
            vec![Command::StartDelete {
                paths: vec!["/x/y.txt".into()]
            }],
        );
    }

    #[test]
    fn input_modal_accepts_typed_characters() {
        use crate::state::{InputDialog, InputKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Input(InputDialog {
            title: "New".into(),
            prompt: "name:".into(),
            value: String::new(),
            cursor: 0,
            kind: InputKind::Mkdir {
                parent: "/x".into(),
            },
        }));
        let (s, _) = update(s, key(KeyCode::Char('a')));
        let (s, _) = update(s, key(KeyCode::Char('b')));
        match s.modal {
            Some(Modal::Input(ref d)) => {
                assert_eq!(d.value, "ab");
                assert_eq!(d.cursor, 2);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn input_modal_backspace_deletes_left() {
        use crate::state::{InputDialog, InputKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Input(InputDialog {
            title: "x".into(),
            prompt: "y".into(),
            value: "abc".into(),
            cursor: 3,
            kind: InputKind::Mkdir {
                parent: "/x".into(),
            },
        }));
        let (s, _) = update(s, key(KeyCode::Backspace));
        match s.modal {
            Some(Modal::Input(ref d)) => {
                assert_eq!(d.value, "ab");
                assert_eq!(d.cursor, 2);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn input_modal_enter_submits_mkdir() {
        use crate::state::{InputDialog, InputKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Input(InputDialog {
            title: "New dir".into(),
            prompt: "name:".into(),
            value: "src".into(),
            cursor: 3,
            kind: InputKind::Mkdir {
                parent: "/proj".into(),
            },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
        assert_eq!(
            cmds,
            vec![Command::Mkdir {
                parent: "/proj".into(),
                name: "src".into()
            }],
        );
    }

    #[test]
    fn confirm_modal_enter_on_no_just_closes() {
        use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Confirm(ConfirmDialog {
            title: "Delete?".into(),
            body: "1 file".into(),
            buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
            focused: 1,
            kind: ConfirmKind::Delete {
                paths: vec!["/x/y.txt".into()],
            },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
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
    fn enter_dir_on_dir_changes_cwd_and_emits_rescan() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::dir("subdir"),
        ]
        .into();
        s.panels[0].cursor = 1;
        s.panels[0].cwd = "/Users/test".into();

        let (s, cmds) = update(s, Event::Command(CommandId::EnterDir));
        assert_eq!(s.panels[0].cwd, "/Users/test/subdir");
        assert!(s.panels[0].loading);
        assert_eq!(s.panels[0].cursor, 0);
        assert!(s.panels[0].selection.is_empty());
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
    }

    #[test]
    fn enter_dir_on_parent_walks_up() {
        let mut s = st();
        s.panels[0].entries = vec![crate::state::DirEntry::parent()].into();
        s.panels[0].cursor = 0;
        s.panels[0].cwd = "/Users/test/sub".into();

        let (s, cmds) = update(s, Event::Command(CommandId::EnterDir));
        assert_eq!(s.panels[0].cwd, "/Users/test");
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
    }

    #[test]
    fn enter_dir_on_file_is_noop() {
        let mut s = st();
        s.panels[0].entries = vec![crate::state::DirEntry::file("x.txt", 1)].into();
        s.panels[0].cursor = 0;
        let cwd_before = s.panels[0].cwd.clone();

        let (s, cmds) = update(s, Event::Command(CommandId::EnterDir));
        assert_eq!(s.panels[0].cwd, cwd_before);
        assert!(cmds.is_empty());
    }

    #[test]
    fn parent_dir_sets_pending_focus_to_left_directory() {
        let mut s = st();
        s.panels[0].cwd = "/Users/test/sub".into();
        let (s, _) = update(s, Event::Command(CommandId::ParentDir));
        assert_eq!(s.panels[0].pending_focus_name.as_deref(), Some("sub"));
    }

    #[test]
    fn dir_scanned_lands_cursor_on_pending_focus_name() {
        use crate::event::{WorkerId, WorkerMsg};
        let mut s = st();
        s.panels[0].pending_focus_name = Some("target".into());
        let entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::dir("crates"),
            crate::state::DirEntry::dir("docs"),
            crate::state::DirEntry::dir("target"),
        ];
        let ev = Event::Worker(
            WorkerId(1),
            WorkerMsg::DirScanned {
                side: PanelSide::Left,
                entries,
            },
        );
        let (s, _) = update(s, ev);
        assert_eq!(s.panels[0].cursor, 3);
        assert!(s.panels[0].pending_focus_name.is_none());
    }

    #[test]
    fn parent_dir_walks_up_and_rescans() {
        let mut s = st();
        s.panels[0].cwd = "/a/b/c".into();
        let (s, cmds) = update(s, Event::Command(CommandId::ParentDir));
        assert_eq!(s.panels[0].cwd, "/a/b");
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
    }

    #[test]
    fn parent_dir_at_root_is_noop() {
        let mut s = st();
        s.panels[0].cwd = "/".into();
        let (s, cmds) = update(s, Event::Command(CommandId::ParentDir));
        assert_eq!(s.panels[0].cwd, "/");
        assert!(cmds.is_empty());
    }

    #[test]
    fn toggle_hidden_flips_flag_and_rescans() {
        let s = st();
        assert!(!s.panels[0].show_hidden);
        let (s, cmds) = update(s, Event::Command(CommandId::ToggleHidden));
        assert!(s.panels[0].show_hidden);
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
    }

    #[test]
    fn cycle_sort_rotates_and_rescans() {
        use crate::state::SortMode;
        let s = st();
        let (s, cmds) = update(s, Event::Command(CommandId::CycleSort));
        assert_eq!(s.panels[0].sort, SortMode::BySize);
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
        let (s, _) = update(s, Event::Command(CommandId::CycleSort));
        assert_eq!(s.panels[0].sort, SortMode::ByModified);
        let (s, _) = update(s, Event::Command(CommandId::CycleSort));
        assert_eq!(s.panels[0].sort, SortMode::ByName);
    }

    #[test]
    fn rescan_focused_emits_one_rescan_for_current_side() {
        let s = st();
        let (_, cmds) = update(s, Event::Command(CommandId::RescanFocused));
        assert_eq!(cmds, vec![Command::RescanDir(PanelSide::Left)]);
    }

    #[test]
    fn rescan_both_emits_two_rescans() {
        let s = st();
        let (_, cmds) = update(s, Event::Command(CommandId::RescanBoth));
        assert_eq!(
            cmds,
            vec![
                Command::RescanDir(PanelSide::Left),
                Command::RescanDir(PanelSide::Right),
            ],
        );
    }

    #[test]
    fn toggle_select_adds_then_removes_focused_row() {
        let mut s = st_with_entries(5);
        s.panels[0].cursor = 2;
        let (s, _) = update(s, Event::Command(CommandId::ToggleSelect));
        assert!(s.panels[0].selection.contains(&2));
        let (s, _) = update(s, Event::Command(CommandId::ToggleSelect));
        assert!(!s.panels[0].selection.contains(&2));
    }

    #[test]
    fn select_all_skips_parent_dot_dot() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("a", 1),
            crate::state::DirEntry::file("b", 2),
        ]
        .into();
        let (s, _) = update(s, Event::Command(CommandId::SelectAll));
        assert!(!s.panels[0].selection.contains(&0));
        assert!(s.panels[0].selection.contains(&1));
        assert!(s.panels[0].selection.contains(&2));
    }

    #[test]
    fn invert_selection_skips_parent_dot_dot() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("a", 1),
            crate::state::DirEntry::file("b", 2),
        ]
        .into();
        s.panels[0].selection.insert(1);
        let (s, _) = update(s, Event::Command(CommandId::InvertSelection));
        assert!(!s.panels[0].selection.contains(&0));
        assert!(!s.panels[0].selection.contains(&1));
        assert!(s.panels[0].selection.contains(&2));
    }

    #[test]
    fn select_none_clears() {
        let mut s = st_with_entries(5);
        s.panels[0].selection.insert(1);
        s.panels[0].selection.insert(3);
        let (s, _) = update(s, Event::Command(CommandId::SelectNone));
        assert!(s.panels[0].selection.is_empty());
    }

    #[test]
    fn view_command_on_file_opens_loading_viewer_and_emits_open_viewer() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("README.md", 100),
        ]
        .into();
        s.panels[0].cursor = 1;
        s.panels[0].cwd = "/x".into();

        let (s, cmds) = update(s, Event::Command(CommandId::View));
        match s.modal {
            Some(crate::state::Modal::Viewer(ref v)) => {
                assert_eq!(v.path, "/x/README.md");
                assert!(v.loading);
            }
            _ => panic!("expected Viewer modal"),
        }
        assert_eq!(cmds, vec![Command::OpenViewer("/x/README.md".into())]);
    }

    #[test]
    fn view_command_on_dir_is_noop() {
        let mut s = st();
        s.panels[0].entries = vec![crate::state::DirEntry::dir("src")].into();
        s.panels[0].cursor = 0;
        let (s, cmds) = update(s, Event::Command(CommandId::View));
        assert!(s.modal.is_none());
        assert!(cmds.is_empty());
    }

    #[test]
    fn preview_loaded_replaces_viewer_body() {
        use crate::state::{Modal, ViewerDialog};
        let mut s = st();
        s.modal = Some(Modal::Viewer(ViewerDialog {
            path: "/x/README.md".into(),
            body: "loading…".into(),
            scroll: 0,
            truncated: false,
            binary: false,
            loading: true,
        }));
        let (s, _) = update(
            s,
            Event::PreviewLoaded {
                path: "/x/README.md".into(),
                body: "actual content".into(),
                truncated: false,
                binary: false,
            },
        );
        match s.modal {
            Some(Modal::Viewer(v)) => {
                assert_eq!(v.body, "actual content");
                assert!(!v.loading);
            }
            _ => panic!(),
        }
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
            WorkerMsg::DirScanned {
                side: PanelSide::Left,
                entries,
            },
        );
        let (s, cmds) = update(s, ev);
        assert_eq!(s.panels[0].entries.len(), 3);
        assert_eq!(s.panels[0].cursor, 2);
        assert!(!s.panels[0].loading);
        assert!(cmds.is_empty());
    }
}
