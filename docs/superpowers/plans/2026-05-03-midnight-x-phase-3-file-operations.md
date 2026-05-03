# Midnight X — Phase 3: File Operations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the v1 spec's promised file operations: `mkdir`, `rename`, `delete`, `copy`, and `move`. Each runs through the existing MVU loop, with progress feedback, cancellation, conflict resolution (Yes/No/All/Skip All/Cancel), per-file error tolerance, and graceful error reporting via modals.

**Architecture:** Synchronous helpers live in `mx-fs` (`copy.rs`, `delete.rs`, `move_op.rs`, plus inline `mkdir`/`rename` on the main thread). Long ops (`copy`, `move`, `delete`) are spawned as worker threads via `Executor`, talk to the main loop through the existing `mpsc` channel, and resume from `Conflict` pauses through a per-worker resume channel. `mx-core::update` dispatches generic `Modal::Confirm` and `Modal::Input` flows now needed by every op; `mx-tui::view` renders the four still-unrendered modals (`Confirm`, `Input`, `Progress`, `Error`). EXDEV cross-device move is decoupled behind a `MoveBackend` trait so tests can force the copy+delete fallback without two real devices.

**Tech Stack:** Same as Phase 2. No new workspace dependencies; only deeper use of `std::fs`, `std::sync::atomic`, `std::sync::mpsc`. Test-only `tempfile` (already present).

**Reference design doc:** `docs/superpowers/specs/2026-05-03-midnight-x-v1-design.md` (sections 5, 7).

---

## File map

```
midnight-x/
├── crates/
│   ├── mx-core/
│   │   └── src/
│   │       ├── command.rs                  # MODIFY: Command::ResolveConflict, OverwritePolicy
│   │       ├── event.rs                    # MODIFY: WorkerMsg fields finalized (already in shape)
│   │       ├── state.rs                    # MODIFY: Modal::Input/Confirm helpers; ConfirmButton enum
│   │       └── update.rs                   # MODIFY: generic Confirm/Input dispatch; worker-msg routing
│   ├── mx-fs/
│   │   ├── Cargo.toml                       # (no change)
│   │   └── src/
│   │       ├── copy.rs                     # NEW: copy_file (with progress), copy_tree
│   │       ├── delete.rs                   # NEW: recursive delete
│   │       ├── move_op.rs                  # NEW: rename + EXDEV fallback (uses copy.rs + delete.rs)
│   │       ├── move_backend.rs             # NEW: MoveBackend trait + LocalBackend
│   │       ├── ops.rs                      # NEW: mkdir, rename (synchronous helpers)
│   │       ├── executor.rs                 # MODIFY: start_copy/move/delete, resolve_conflict, cancel
│   │       └── lib.rs                      # MODIFY: re-exports
│   ├── mx-tui/
│   │   └── src/
│   │       └── view.rs                     # MODIFY: render_confirm, render_input, render_progress, render_error
│   └── mx/
│       └── src/
│           └── app.rs                      # MODIFY: handle Mkdir/Rename/Start*/CancelWorker/ResolveConflict
└── docs/superpowers/plans/...                # this file
```

---

## Conventions

- Same as Phases 1 & 2: TDD per task; `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` after each commit; conventional commits, no AI-tool attribution.
- `PATH=/opt/homebrew/opt/rustup/bin:$PATH` prefix for cargo on this dev machine (or wherever cargo lives in your shell).
- Snapshot regeneration when modal rendering changes: `INSTA_UPDATE=force cargo test -p mx-tui --test render_snapshots`, then `find tests/snapshots -name '*.snap.new' -delete`.
- All new public APIs document `# Errors` and (where applicable) `# Panics`; clippy enforces this on libraries.

---

## Task 1: `mx-core` — `OverwritePolicy`, `Command::ResolveConflict`, generic confirm/input button enums

**Files:**
- Modify: `crates/mx-core/src/command.rs`
- Modify: `crates/mx-core/src/state.rs`

The Phase 2 `Command` enum already has `CancelWorker(WorkerId)` but no resolve-conflict variant. Add it. Also add `OverwritePolicy` (the five buttons of the conflict modal) and `ConfirmButton` (the two-button shape used by Delete/Copy/Move "are you sure?" dialogs).

- [ ] **Step 1: Modify `crates/mx-core/src/command.rs`**

Find the `Command` enum and add `ResolveConflict`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RescanDir(crate::state::PanelSide),
    StartCopy   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartMove   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartDelete { paths: Vec<Utf8PathBuf> },
    Mkdir       { parent: Utf8PathBuf, name: String },
    Rename      { from: Utf8PathBuf, to: Utf8PathBuf },
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
```

Imports at the top of the file already include `WorkerId`; no change needed there.

- [ ] **Step 2: Modify `crates/mx-core/src/state.rs`** — add a `ConfirmButton` enum and let `ConfirmDialog` track which button has focus.

Replace the existing `ConfirmDialog`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDialog {
    pub title:       String,
    pub body:        String,
    pub buttons:     Vec<ConfirmButton>,
    pub focused:     usize,        // index into `buttons`
    pub kind:        ConfirmKind,  // what command this dialog resolves to
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmButton {
    Yes,
    No,
    YesAll,
    NoAll,
    Cancel,
    Ok,
}

/// What the calling code wants to do with the confirm result. Stored on the
/// dialog so update() can fire the right `Command` when the user submits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    /// Plain are-you-sure for Delete. Body lists what would be deleted.
    Delete { paths: Vec<camino::Utf8PathBuf> },
    /// Are-you-sure for Copy / Move (initial dispatch).
    StartCopy { src: Vec<camino::Utf8PathBuf>, dst: camino::Utf8PathBuf },
    StartMove { src: Vec<camino::Utf8PathBuf>, dst: camino::Utf8PathBuf },
    /// Mid-worker overwrite conflict. The id identifies the worker waiting
    /// on a `Command::ResolveConflict`.
    Conflict { worker: crate::event::WorkerId },
    /// Quit-while-workers-running confirm.
    QuitWithWorkers,
}
```

Replace `InputDialog` to track *what to submit*:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDialog {
    pub title:  String,
    pub prompt: String,
    pub value:  String,
    pub cursor: usize,        // byte offset within `value`
    pub kind:   InputKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    /// New directory name; submit emits `Command::Mkdir`.
    Mkdir { parent: camino::Utf8PathBuf },
    /// Rename target; submit emits `Command::Rename`.
    Rename { from: camino::Utf8PathBuf },
}
```

`ConfirmDialog`'s old field `default_yes` is gone — `focused` replaces it.

- [ ] **Step 3: Update Phase 1's `QuitConfirm` modal handling**

The Phase 1 `Modal::QuitConfirm` (a unit variant) stays as a separate variant for backwards compat; nothing to change beyond making sure the new `ConfirmKind::QuitWithWorkers` path doesn't conflict. Search:

```bash
grep -rn 'Modal::QuitConfirm' crates/
```

Existing match arms continue to work.

- [ ] **Step 4: Verify mx-core builds**

Run: `PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo build -p mx-core`
Expected: clean — though existing tests that constructed `ConfirmDialog { default_yes: ... }` will fail to compile; that's the cue to fix them in step 5.

- [ ] **Step 5: Fix tests that referenced old fields**

Search for old struct literals:
```bash
grep -rn 'ConfirmDialog\|default_yes\|InputDialog {' crates/
```

If any production code or test still uses the old shape, update to the new shape. (Phase 1 had no production constructors of `ConfirmDialog`/`InputDialog`; only the type definitions exist. If you see compile errors, fix them inline.)

- [ ] **Step 6: Tests + clippy**

Run: `cargo test -p mx-core && cargo clippy --workspace --all-targets -- -D warnings`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add crates/mx-core/src/command.rs crates/mx-core/src/state.rs
git commit -m "feat(core): OverwritePolicy, ConfirmKind, InputKind, Command::ResolveConflict"
```

---

## Task 2: `mx-core::update` — generic Modal::Confirm dispatch

**Files:**
- Modify: `crates/mx-core/src/update.rs`

`Tab`/`Shift-Tab` cycles button focus, `Enter` submits the focused button, `Esc` cancels. On submit, the dialog's `kind` decides which `Command` to emit.

- [ ] **Step 1: Write the failing tests**

Append to `update::tests`:

```rust
    #[test]
    fn confirm_modal_tab_cycles_focus() {
        use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Confirm(ConfirmDialog {
            title: "x".into(),
            body: "y".into(),
            buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
            focused: 0,
            kind: ConfirmKind::Delete { paths: vec!["/x".into()] },
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
            kind: ConfirmKind::Delete { paths: vec!["/x/y.txt".into()] },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
        assert_eq!(
            cmds,
            vec![Command::StartDelete { paths: vec!["/x/y.txt".into()] }],
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
            focused: 1, // No
            kind: ConfirmKind::Delete { paths: vec!["/x/y.txt".into()] },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
        assert!(cmds.is_empty());
    }
```

- [ ] **Step 2: Implement**

In `crates/mx-core/src/update.rs::dispatch`, just below the viewer-modal block but above the existing modal-aware match, add a confirm-modal handler:

```rust
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
                // Re-using the Enter chord here — `Enter` resolves the focused button.
                return resolve_confirm(state);
            }
            _ => return Vec::new(),
        }
    }
```

Then add the helper at the bottom of the file (near `cd_to`):

```rust
fn resolve_confirm(state: &mut State) -> Vec<Command> {
    use crate::state::{ConfirmButton, ConfirmKind, Modal};
    let Some(Modal::Confirm(d)) = state.modal.take() else { return Vec::new(); };
    let button = d.buttons.get(d.focused).copied();
    match (d.kind, button) {
        (ConfirmKind::Delete { paths }, Some(ConfirmButton::Yes)) => {
            vec![Command::StartDelete { paths }]
        }
        (ConfirmKind::StartCopy { src, dst }, Some(ConfirmButton::Yes)) => {
            vec![Command::StartCopy { src, dst }]
        }
        (ConfirmKind::StartMove { src, dst }, Some(ConfirmButton::Yes)) => {
            vec![Command::StartMove { src, dst }]
        }
        (ConfirmKind::Conflict { worker }, Some(b)) => {
            use crate::command::OverwritePolicy as P;
            let policy = match b {
                ConfirmButton::Yes    => P::Yes,
                ConfirmButton::No     => P::No,
                ConfirmButton::YesAll => P::YesAll,
                ConfirmButton::NoAll  => P::NoAll,
                ConfirmButton::Cancel => P::Cancel,
                ConfirmButton::Ok     => P::No, // shouldn't happen on conflict
            };
            vec![Command::ResolveConflict(worker, policy)]
        }
        (ConfirmKind::QuitWithWorkers, Some(ConfirmButton::Yes)) => {
            state.should_quit = true;
            vec![Command::Quit]
        }
        // Anything else (No / Cancel / no button focused / unmatched combo): just close.
        _ => Vec::new(),
    }
}
```

The mapping above re-uses `CommandId::EnterDir` for "Enter pressed inside Confirm". `EnterDir` is the binding for the `Enter` key in the default keymap, and the existing modal-aware dispatch already short-circuits before it reaches `handle_command_no_modal`. Verified by Phase 1 tests.

- [ ] **Step 3: Move the `(_, CommandId::Cancel)` arm above the new handler**

The existing modal-aware match in `dispatch` includes `(_, CommandId::Cancel) | (Modal::Help, CommandId::Help)` as a "close any modal" arm. Make sure the new Confirm handler runs *before* that arm so we hit the new code path. Inspect the file order; the new handler should appear above the existing match.

- [ ] **Step 4: Run tests + clippy**

Run: `cargo test -p mx-core update::confirm`
Expected: 3 new tests pass.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): generic Modal::Confirm dispatch (tab cycles, enter submits via ConfirmKind)"
```

---

## Task 3: `mx-core::update` — Modal::Input dispatch (typing, cursor, submit)

**Files:**
- Modify: `crates/mx-core/src/update.rs`

`Modal::Input` accepts printable characters, Backspace, Delete, Left/Right, Home/End, Enter (submits → emits `Command::Mkdir` / `Command::Rename` based on `kind`), Esc (cancels).

- [ ] **Step 1: Write the failing tests**

Append to `update::tests`:

```rust
    #[test]
    fn input_modal_accepts_typed_characters() {
        use crate::state::{InputDialog, InputKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Input(InputDialog {
            title: "New".into(),
            prompt: "name:".into(),
            value: String::new(),
            cursor: 0,
            kind: InputKind::Mkdir { parent: "/x".into() },
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
            kind: InputKind::Mkdir { parent: "/x".into() },
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
            kind: InputKind::Mkdir { parent: "/proj".into() },
        }));
        let (s, cmds) = update(s, key(KeyCode::Enter));
        assert!(s.modal.is_none());
        assert_eq!(
            cmds,
            vec![Command::Mkdir { parent: "/proj".into(), name: "src".into() }],
        );
    }

    #[test]
    fn input_modal_esc_cancels_without_emitting_command() {
        use crate::state::{InputDialog, InputKind, Modal};
        let mut s = st();
        s.modal = Some(Modal::Input(InputDialog {
            title: "x".into(),
            prompt: "y".into(),
            value: "name".into(),
            cursor: 4,
            kind: InputKind::Mkdir { parent: "/x".into() },
        }));
        // Esc alone fires Cancel via the chord engine after the timeout.
        let one_sec_ago = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("monotonic clock supports 1s subtraction");
        let s2 = State { pending_chord: vec![KeyChord::bare(KeyCode::Esc)], pending_since: Some(one_sec_ago), ..s };
        let (s3, cmds) = update(s2, Event::Tick { dt: Duration::from_millis(100) });
        assert!(s3.modal.is_none());
        assert!(cmds.is_empty());
    }
```

- [ ] **Step 2: Implement Input handling at the top of `dispatch`**

In `dispatch`, *before* the Viewer block and Confirm block, add:

```rust
    if let Some(Modal::Input(_)) = state.modal.as_ref() {
        match id {
            CommandId::Cancel => {
                state.modal = None;
                return Vec::new();
            }
            CommandId::EnterDir => {
                return resolve_input(state);
            }
            _ => return Vec::new(), // typing handled in handle_chord, not here
        }
    }
```

Plus a separate path in `handle_chord`: when `Modal::Input` is open, route printable keys directly to the dialog *before* the keymap-resolution logic. Replace the body of `handle_chord` with:

```rust
fn handle_chord(state: &mut State, chord: KeyChord, cmds: &mut Vec<Command>) {
    // Typing into an Input modal short-circuits the keymap entirely.
    if let Some(Modal::Input(d)) = state.modal.as_mut() {
        if input_modal_consume_key(d, chord) {
            return;
        }
        // Anything we didn't consume falls through to the keymap (so Esc /
        // Tab / Enter / arrow keys still work).
    }

    state.pending_chord.push(chord);
    let lookup = state.config.keymap.lookup(&state.pending_chord);
    // … rest of the function unchanged.
}
```

Add the helpers:

```rust
fn input_modal_consume_key(d: &mut crate::state::InputDialog, c: KeyChord) -> bool {
    use crate::input::{KeyCode, KeyModifiers};
    // Plain or Shift-modified chars become text. Ctrl-X, Alt-X, function keys,
    // Esc, Tab, Enter all bubble up to the chord engine.
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
        KeyCode::Home => { d.cursor = 0; true }
        KeyCode::End  => { d.cursor = d.value.len(); true }
        _ => {
            let _ = KeyModifiers::NONE;
            false
        }
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
    let Some(Modal::Input(d)) = state.modal.take() else { return Vec::new(); };
    let value = d.value.trim().to_string();
    if value.is_empty() {
        return Vec::new();
    }
    match d.kind {
        InputKind::Mkdir { parent } => vec![Command::Mkdir { parent, name: value }],
        InputKind::Rename { from } => {
            // Same parent as `from`; new last segment = `value`.
            let parent = from.parent().map(camino::Utf8Path::to_path_buf).unwrap_or_default();
            let to = if parent.as_str().is_empty() {
                camino::Utf8PathBuf::from(value)
            } else {
                parent.join(value)
            };
            vec![Command::Rename { from, to }]
        }
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mx-core update::input`
Expected: 4 new tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): Modal::Input typing/cursor/submit dispatch"
```

---

## Task 4: `mx-tui::view` — render Modal::Confirm

**Files:**
- Modify: `crates/mx-tui/src/view.rs`

The confirm modal already has a `body` field; we replace its renderer to draw the body lines plus a button row at the bottom. The focused button is reverse-video.

- [ ] **Step 1: Modify `render_modal`**

Find the `Modal::Confirm(d)` arm in the `body` match and replace with:

```rust
        Modal::Confirm(d) => render_confirm_body(d),
```

Replace the corresponding `title` arm:

```rust
        Modal::Confirm(d) => {
            // ratatui's Block::title takes &str; use a leaked Box only if
            // the title is dynamic. We already had `" Confirm "` as a
            // static; preserve that and prepend any per-dialog title via the body.
            " Confirm "
        }
```

(Leave `title` as `" Confirm "` — `d.title` shows in the body's first line.)

Add the helper at the bottom of the file:

```rust
fn render_confirm_body(d: &mx_core::state::ConfirmDialog) -> String {
    let mut out = String::new();
    if !d.title.is_empty() {
        out.push_str(&d.title);
        out.push('\n');
        out.push('\n');
    }
    out.push_str(&d.body);
    out.push_str("\n\n");
    for (i, b) in d.buttons.iter().enumerate() {
        let label = match b {
            mx_core::state::ConfirmButton::Yes    => "Yes",
            mx_core::state::ConfirmButton::No     => "No",
            mx_core::state::ConfirmButton::YesAll => "Yes-All",
            mx_core::state::ConfirmButton::NoAll  => "No-All",
            mx_core::state::ConfirmButton::Cancel => "Cancel",
            mx_core::state::ConfirmButton::Ok     => "OK",
        };
        if i > 0 {
            out.push_str("  ");
        }
        if i == d.focused {
            // Render focused button surrounded by `>` `<` markers — the
            // overall modal style stays uniform; we don't change cell styles
            // mid-paragraph, so the markers are the focus signal.
            out.push_str(&format!(">{label}<"));
        } else {
            out.push_str(&format!(" {label} "));
        }
    }
    out
}
```

- [ ] **Step 2: Build + check**

Run: `cargo build -p mx-tui && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. (Snapshot regeneration done in Task 11.)

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/view.rs
git commit -m "feat(tui): render Modal::Confirm with focused-button markers"
```

---

## Task 5: `mx-tui::view` — render Modal::Input

**Files:**
- Modify: `crates/mx-tui/src/view.rs`

The input dialog shows the prompt above a text field; the cursor blinks at `d.cursor`. Since we don't move the terminal cursor (Phase 1 chose to render cursors in-line), we render the text with a `▏` glyph at the cursor position and let the user see where they are.

- [ ] **Step 1: Replace the `Modal::Input(d)` `body` arm**

```rust
        Modal::Input(d) => render_input_body(d),
```

Add the helper:

```rust
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
```

- [ ] **Step 2: Build + clippy**

Run: `cargo build -p mx-tui && cargo clippy --workspace --all-targets -- -D warnings`

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/view.rs
git commit -m "feat(tui): render Modal::Input with inline ▏ cursor glyph"
```

---

## Task 6: `mx-fs::ops` — sync `mkdir` and `rename` helpers

**Files:**
- Create: `crates/mx-fs/src/ops.rs`
- Modify: `crates/mx-fs/src/lib.rs`

Both run on the main thread (instant on local FS), called by `app.rs` when it gets the corresponding `Command`.

- [ ] **Step 1: Write `crates/mx-fs/src/ops.rs`**

```rust
//! Synchronous filesystem helpers run on the main thread (mkdir/rename).
//! They return `FsError` on failure; the caller surfaces that via a modal.

use camino::{Utf8Path, Utf8PathBuf};

use mx_core::errors::FsError;

use crate::dir_scan;

/// Create a single new directory under `parent`.
///
/// # Errors
///
/// Returns `FsError::AlreadyExists` if the target already exists,
/// `FsError::NotFound` if `parent` doesn't exist, etc.
pub fn mkdir(parent: &Utf8Path, name: &str) -> Result<Utf8PathBuf, FsError> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(FsError::Io(format!("invalid directory name: {name:?}")));
    }
    let path = parent.join(name);
    std::fs::create_dir(&path).map_err(dir_scan::map_io_pub)?;
    Ok(path)
}

/// Rename `from` to `to` (same directory expected; the caller validates).
///
/// # Errors
///
/// Returns `FsError::AlreadyExists` if `to` already exists, etc.
pub fn rename(from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError> {
    if to.exists() {
        return Err(FsError::AlreadyExists);
    }
    std::fs::rename(from, to).map_err(dir_scan::map_io_pub)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn td() -> tempfile::TempDir { tempfile::tempdir().unwrap() }
    fn p(d: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(d.path().to_path_buf()).unwrap()
    }

    #[test]
    fn mkdir_creates_a_new_directory() {
        let t = td();
        let path = mkdir(&p(&t), "newdir").unwrap();
        assert!(path.is_dir());
    }

    #[test]
    fn mkdir_rejects_slash_in_name() {
        let t = td();
        assert!(matches!(mkdir(&p(&t), "a/b"), Err(FsError::Io(_))));
    }

    #[test]
    fn mkdir_already_exists_returns_error() {
        let t = td();
        std::fs::create_dir(t.path().join("there")).unwrap();
        assert_eq!(mkdir(&p(&t), "there").unwrap_err(), FsError::AlreadyExists);
    }

    #[test]
    fn rename_works_and_rejects_overwrite() {
        let t = td();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        let from = p(&t).join("a");
        let to = p(&t).join("b");
        rename(&from, &to).unwrap();
        assert!(to.exists() && !from.exists());

        // Now create another file at `b` and try to rename `c` over it.
        std::fs::write(t.path().join("c"), b"y").unwrap();
        let c = p(&t).join("c");
        assert_eq!(rename(&c, &to).unwrap_err(), FsError::AlreadyExists);
    }
}
```

- [ ] **Step 2: Expose `dir_scan::map_io_pub`**

In `crates/mx-fs/src/dir_scan.rs`, find the existing `fn map_io(e: &std::io::Error) -> FsError` and add a public re-export beside it:

```rust
/// Public re-export so sibling modules (`ops`, `copy`, `delete`) can map
/// `io::Error` consistently.
pub fn map_io_pub(e: std::io::Error) -> FsError {
    map_io(&e)
}
```

(Or rename `map_io` and adjust callers — your call. Inline `pub fn map_io_pub` is the lowest-friction option.)

- [ ] **Step 3: Update `crates/mx-fs/src/lib.rs`**

Add `pub mod ops;` and a re-export `pub use ops::{mkdir, rename};`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mx-fs ops::tests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-fs
git commit -m "feat(fs): synchronous mkdir + rename helpers with tempdir tests"
```

---

## Task 7: Wire `Mkdir` / `Rename` commands

**Files:**
- Modify: `crates/mx-core/src/update.rs`
- Modify: `crates/mx/src/app.rs`

`F7` opens `Modal::Input{ kind: Mkdir }`; on submit `update()` emits `Command::Mkdir`. `Ctrl-T`/`Shift-F6` opens an Input pre-filled with the focused entry's name; submit emits `Command::Rename`. `app.rs` calls the sync helpers; on success it kicks a rescan.

- [ ] **Step 1: Modify `update.rs`** — extend `handle_command_no_modal` for `Mkdir` and `Rename`. Replace the existing arms:

```rust
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
```

(Lift them out of the catchall — same pattern Phase 2 used.)

- [ ] **Step 2: Modify `app.rs`** — handle the two new commands.

In the `for cmd in cmds` match, replace:

```rust
                | Command::Mkdir { .. }
                | Command::Rename { .. }
```

with explicit handlers:

```rust
                Command::Mkdir { parent, name } => {
                    if let Err(e) = mx_fs::ops::mkdir(&parent, &name) {
                        let _ = tx.send(Event::PreviewFailed { path: parent.clone(), error: e });
                    }
                    schedule_rescan(&mut executor, &state, state.focus);
                }
                Command::Rename { from, to } => {
                    if let Err(e) = mx_fs::ops::rename(&from, &to) {
                        let _ = tx.send(Event::PreviewFailed { path: from.clone(), error: e });
                    }
                    schedule_rescan(&mut executor, &state, state.focus);
                }
```

(`Event::PreviewFailed` is being repurposed as a generic "open an Error modal with this path+error" channel. We rename it to `Event::OpFailed` for clarity in Task 8.)

- [ ] **Step 3: Tests**

Run: `cargo test --workspace`
Expected: green.

Run: `cargo run -p mx --release` and verify F7 opens the mkdir prompt; type `foo`, press Enter, see the new directory appear.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs crates/mx/src/app.rs
git commit -m "feat: wire F7 Mkdir and Shift-F6/Ctrl-T Rename via Input modal"
```

---

## Task 8: Generalize `Event::PreviewFailed` into `Event::OpFailed`

**Files:**
- Modify: `crates/mx-core/src/event.rs`
- Modify: `crates/mx-core/src/update.rs`
- Modify: `crates/mx/src/app.rs`

The existing `PreviewLoaded` / `PreviewFailed` are viewer-only. We generalize the failure path to "an op for this path failed; show an error modal" so mkdir / rename / future ops share one channel.

- [ ] **Step 1: Add a new `Event::OpFailed` variant**

In `crates/mx-core/src/event.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Input(InputEvent),
    Command(CommandId),
    Worker(WorkerId, WorkerMsg),
    Tick { dt: Duration },
    Resize { cols: u16, rows: u16 },
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
    OpFailed {
        title: String,
        path: camino::Utf8PathBuf,
        error: FsError,
    },
}
```

- [ ] **Step 2: Handle in `update.rs`**

Add an arm to the top-level `match event`:

```rust
        Event::OpFailed { title, path, error } => {
            use crate::state::{ErrorDialog, Modal};
            state.modal = Some(Modal::Error(ErrorDialog {
                title,
                body: format!("{path}: {error}"),
                details: Vec::new(),
            }));
        }
```

- [ ] **Step 3: Use it from `app.rs`** — replace the `PreviewFailed` reuse for mkdir/rename:

```rust
                Command::Mkdir { parent, name } => {
                    if let Err(e) = mx_fs::ops::mkdir(&parent, &name) {
                        let _ = tx.send(Event::OpFailed {
                            title: "Make directory failed".into(),
                            path: parent.join(&name),
                            error: e,
                        });
                    }
                    schedule_rescan(&mut executor, &state, state.focus);
                }
                Command::Rename { from, to } => {
                    if let Err(e) = mx_fs::ops::rename(&from, &to) {
                        let _ = tx.send(Event::OpFailed {
                            title: "Rename failed".into(),
                            path: from.clone(),
                            error: e,
                        });
                    }
                    schedule_rescan(&mut executor, &state, state.focus);
                }
```

- [ ] **Step 4: Tests + commit**

```bash
cargo test --workspace
git add crates/mx-core/src/event.rs crates/mx-core/src/update.rs crates/mx/src/app.rs
git commit -m "feat(core): Event::OpFailed → generic Modal::Error for mkdir/rename failures"
```

---

## Task 9: `mx-fs::delete` — recursive delete helper

**Files:**
- Create: `crates/mx-fs/src/delete.rs`
- Modify: `crates/mx-fs/src/lib.rs`

Removes files / directory trees recursively, leaf-first. Reports per-file errors; never aborts the whole job on a single failure.

- [ ] **Step 1: Write `crates/mx-fs/src/delete.rs`**

```rust
//! Recursive delete with per-file error tolerance and cooperative
//! cancellation. The synchronous helpers are reusable (tests and the
//! Executor share them); threading lives in `executor.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use camino::Utf8Path;

use mx_core::errors::FsError;

use crate::dir_scan;

/// Result of deleting one tree (file or directory).
#[derive(Debug, Default)]
pub struct DeleteReport {
    pub deleted: u64,
    pub errors: Vec<(camino::Utf8PathBuf, FsError)>,
    pub cancelled: bool,
}

/// Recursively delete `root`. Returns a report rather than `Result` so the
/// caller can present partial successes.
///
/// `cancel` is checked between entries; cancelling stops further work but
/// does not undo prior deletions.
#[must_use]
pub fn delete_tree(root: &Utf8Path, cancel: &Arc<AtomicBool>) -> DeleteReport {
    let mut report = DeleteReport::default();
    delete_inner(root, cancel, &mut report);
    if cancel.load(Ordering::Relaxed) {
        report.cancelled = true;
    }
    report
}

fn delete_inner(path: &Utf8Path, cancel: &Arc<AtomicBool>, r: &mut DeleteReport) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(e)));
            return;
        }
    };
    if meta.is_dir() && !meta.is_symlink() {
        let read = match std::fs::read_dir(path) {
            Ok(r) => r,
            Err(e) => {
                r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(e)));
                return;
            }
        };
        for child in read {
            if cancel.load(Ordering::Relaxed) { return; }
            let Ok(child) = child else { continue; };
            let Some(child_name) = child.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let child_path = path.join(child_name);
            delete_inner(&child_path, cancel, r);
        }
        if cancel.load(Ordering::Relaxed) { return; }
        match std::fs::remove_dir(path) {
            Ok(()) => r.deleted += 1,
            Err(e) => r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(e))),
        }
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => r.deleted += 1,
            Err(e) => r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn p(d: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(d.path().to_path_buf()).unwrap()
    }
    fn cancel_off() -> Arc<AtomicBool> { Arc::new(AtomicBool::new(false)) }

    #[test]
    fn deletes_a_single_file() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        let report = delete_tree(&p(&t).join("a"), &cancel_off());
        assert_eq!(report.deleted, 1);
        assert!(report.errors.is_empty());
        assert!(!report.cancelled);
        assert!(!t.path().join("a").exists());
    }

    #[test]
    fn deletes_a_directory_tree() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("a/b/c")).unwrap();
        std::fs::write(t.path().join("a/b/c/leaf"), b"x").unwrap();
        std::fs::write(t.path().join("a/sibling"), b"y").unwrap();
        let report = delete_tree(&p(&t).join("a"), &cancel_off());
        assert!(report.errors.is_empty());
        assert!(report.deleted >= 5); // dirs a, a/b, a/b/c + files leaf, sibling
        assert!(!t.path().join("a").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_dir_is_unlinked_not_recursed() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("real")).unwrap();
        std::fs::write(t.path().join("real/sentinel"), b"x").unwrap();
        std::os::unix::fs::symlink(t.path().join("real"), t.path().join("link")).unwrap();
        let report = delete_tree(&p(&t).join("link"), &cancel_off());
        assert!(report.errors.is_empty());
        assert!(t.path().join("real/sentinel").exists(), "sentinel inside real dir must remain");
        assert!(!t.path().join("link").exists());
    }
}
```

- [ ] **Step 2: Update `lib.rs`**

```rust
pub mod delete;
pub use delete::{delete_tree, DeleteReport};
```

- [ ] **Step 3: Tests**

Run: `cargo test -p mx-fs delete::tests`
Expected: 3 (Unix: 3 / Windows: 2) tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-fs
git commit -m "feat(fs): recursive delete_tree with per-file error tolerance + cancel"
```

---

## Task 10: `Executor::start_delete` + cancel infra

**Files:**
- Modify: `crates/mx-fs/src/executor.rs`

Per worker we now also store an `Arc<AtomicBool>` cancel flag and a `Sender` for resume signals (used in Task 14 for conflict resolution; preallocate the slot now to avoid re-plumbing).

- [ ] **Step 1: Replace the `Executor` struct**

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub struct Executor {
    tx:       Sender<Event>,
    next_id:  AtomicU64,
    workers:  HashMap<WorkerId, WorkerHandle>,
}

struct WorkerHandle {
    join:    JoinHandle<()>,
    cancel:  Arc<AtomicBool>,
    /// Sender used to deliver a `OverwritePolicy` resolution to a worker
    /// paused on a `Conflict`. `None` for ops that never raise conflicts.
    resume:  Option<Sender<mx_core::command::OverwritePolicy>>,
}
```

Replace `start_dir_scan` and add `start_delete`:

```rust
impl Executor {
    #[must_use]
    pub fn new(tx: Sender<Event>) -> Self {
        Self { tx, next_id: AtomicU64::new(1), workers: HashMap::new() }
    }

    fn alloc_id(&self) -> WorkerId {
        WorkerId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    pub fn start_dir_scan(
        &mut self,
        side: PanelSide,
        dir: Utf8PathBuf,
        show_hidden: bool,
        sort: SortMode,
    ) -> WorkerId {
        let id = self.alloc_id();
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let join = std::thread::Builder::new()
            .name(format!("mx-fs-scan-{}", id.0))
            .spawn(move || {
                let msg = match crate::dir_scan::scan(&dir, show_hidden, sort) {
                    Ok(entries) => WorkerMsg::DirScanned { side, entries },
                    Err(e) => WorkerMsg::Failed { errors: vec![(dir.clone(), e)] },
                };
                let _ = tx.send(Event::Worker(id, msg));
            })
            .expect("spawn scan");
        self.workers.insert(id, WorkerHandle { join, cancel, resume: None });
        id
    }

    pub fn start_delete(&mut self, paths: Vec<Utf8PathBuf>) -> WorkerId {
        let id = self.alloc_id();
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let join = std::thread::Builder::new()
            .name(format!("mx-fs-del-{}", id.0))
            .spawn(move || {
                let mut errors = Vec::new();
                for p in &paths {
                    if cancel_for_thread.load(Ordering::Relaxed) { break; }
                    let _ = tx.send(Event::Worker(
                        id,
                        WorkerMsg::Progress {
                            bytes_done: 0,
                            bytes_total: 0,
                            current_path: p.clone(),
                        },
                    ));
                    let report = crate::delete::delete_tree(p, &cancel_for_thread);
                    errors.extend(report.errors);
                }
                let msg = if errors.is_empty() {
                    WorkerMsg::Done
                } else {
                    WorkerMsg::Failed { errors }
                };
                let _ = tx.send(Event::Worker(id, msg));
            })
            .expect("spawn delete");
        self.workers.insert(id, WorkerHandle { join, cancel, resume: None });
        id
    }

    pub fn cancel(&self, id: WorkerId) {
        if let Some(w) = self.workers.get(&id) {
            w.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn reap(&mut self) {
        self.workers.retain(|_, w| !w.join.is_finished());
    }

    pub fn join_all(&mut self) {
        let take = std::mem::take(&mut self.workers);
        for (_, w) in take {
            let _ = w.join.join();
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize { self.workers.len() }
}
```

(The old `start_dir_scan` body is kept as-is functionally; the difference is it now stores into the new `WorkerHandle` shape.)

The existing tests for `start_dir_scan` continue to pass; the new `cancel` and the worker handle change are additive.

- [ ] **Step 2: Add a delete test**

Append to `executor::tests`:

```rust
    #[test]
    fn start_delete_removes_paths_and_emits_done() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        std::fs::write(t.path().join("b"), b"y").unwrap();
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let p_a = camino::Utf8PathBuf::from_path_buf(t.path().join("a")).unwrap();
        let p_b = camino::Utf8PathBuf::from_path_buf(t.path().join("b")).unwrap();
        let _id = ex.start_delete(vec![p_a, p_b]);
        let _ = drain(&rx, |e| matches!(e, Event::Worker(_, WorkerMsg::Done)));
        ex.join_all();
        assert!(!t.path().join("a").exists());
        assert!(!t.path().join("b").exists());
    }
```

Run: `cargo test -p mx-fs executor::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-fs/src/executor.rs
git commit -m "feat(fs): Executor::start_delete + per-worker cancel + resume slot"
```

---

## Task 11: `F8 Delete` flow + Modal::Progress + WorkerMsg::Progress handler

**Files:**
- Modify: `crates/mx-core/src/update.rs`
- Modify: `crates/mx/src/app.rs`
- Modify: `crates/mx-tui/src/view.rs` (render Progress modal)

`F8` opens a `Modal::Confirm{kind: Delete{paths}}`. On Yes → `Command::StartDelete`. App spawns the worker. Worker emits Progress → `Modal::Progress` opens. Worker emits Done → modal closes and the panel rescans. Worker emits Failed → Error modal.

- [ ] **Step 1: `update.rs` — wire `Delete` command**

Replace the existing `Delete` arm in the `handle_command_no_modal` Phase-3 catchall:

```rust
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
                format!("Delete {}?", paths[0])
            } else {
                format!("Delete {} items?", paths.len())
            };
            state.modal = Some(Modal::Confirm(ConfirmDialog {
                title: "Confirm delete".into(),
                body,
                buttons: vec![ConfirmButton::No, ConfirmButton::Yes],
                focused: 0, // Default to "No" for destructive ops.
                kind: ConfirmKind::Delete { paths },
            }));
        }
```

Add the helper near `cd_to`:

```rust
fn collect_targets(panel: &crate::state::PanelState) -> Vec<camino::Utf8PathBuf> {
    let mut out = Vec::new();
    if !panel.selection.is_empty() {
        for &i in &panel.selection {
            if let Some(e) = panel.entries.get(i) {
                if e.name == ".." { continue; }
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
```

- [ ] **Step 2: `update.rs` — handle `WorkerMsg::Progress` and `WorkerMsg::Done` / `Failed`**

Replace the Phase-2 stub `handle_worker_msg`:

```rust
fn handle_worker_msg(state: &mut State, id: crate::event::WorkerId, msg: crate::event::WorkerMsg) {
    use crate::event::WorkerMsg;
    use crate::state::{ErrorDialog, Modal, ProgressDialog};
    match msg {
        WorkerMsg::DirScanned { side, entries } => {
            let panel = &mut state.panels[side.index()];
            let new_entries: std::sync::Arc<[_]> = entries.into();
            panel.entries = new_entries;
            panel.loading = false;
            if let Some(want) = panel.pending_focus_name.take() {
                if let Some(idx) = panel.entries.iter().position(|e| e.name == want) {
                    panel.cursor = idx;
                    panel.scroll = idx.saturating_sub(FOCUS_VIEWPORT_HINT / 2);
                }
            }
            let last = panel.entries.len().saturating_sub(1);
            if panel.cursor > last { panel.cursor = last; }
            if panel.scroll > last { panel.scroll = last; }
        }
        WorkerMsg::Progress { bytes_done, bytes_total, current_path } => {
            // Open or update the progress modal for this worker.
            let need_open = !matches!(state.modal, Some(Modal::Progress(ref p)) if p.worker_id == id);
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
            state.workers.remove(&id);
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
            state.workers.remove(&id);
        }
        WorkerMsg::Conflict { .. } => {
            // Filled in in Task 14.
        }
    }
}
```

Also at the top of `update::tests`, register the worker in state.workers when the test simulates a delete (so removal works). Or skip that detail — `state.workers.remove` is a no-op for unknown ids.

- [ ] **Step 3: `app.rs` — handle `StartDelete` and `CancelWorker`**

Replace the Phase-2 stubs:

```rust
                Command::StartDelete { paths } => {
                    let id = executor.start_delete(paths);
                    state.workers.insert(
                        id,
                        mx_core::state::WorkerState {
                            id,
                            kind: mx_core::state::WorkerKind::Delete,
                        },
                    );
                }
                Command::CancelWorker(id) => {
                    executor.cancel(id);
                }
```

Leave the still-Phase-3 variants (`StartCopy`, `StartMove`, `ResolveConflict`) as no-ops for now.

- [ ] **Step 4: Render `Modal::Progress`**

In `view.rs::render_modal`, the existing `Modal::Progress(d)` body arm renders bytes; expand it to show a progress bar:

```rust
        Modal::Progress(d) => render_progress_body(d, area),
```

Add the helper:

```rust
fn render_progress_body(d: &mx_core::state::ProgressDialog, area: Rect) -> String {
    let mut out = String::new();
    out.push_str(&d.current_path.to_string());
    out.push('\n');
    let inner_w = area.width.saturating_sub(4) as usize;
    let bar_w = inner_w.saturating_sub(8); // room for "[ ]" + " 100%"
    if d.bytes_total > 0 && bar_w > 0 {
        let filled = ((d.bytes_done.min(d.bytes_total) as f64 / d.bytes_total as f64) * bar_w as f64) as usize;
        out.push('[');
        for _ in 0..filled { out.push('█'); }
        for _ in filled..bar_w { out.push('─'); }
        let pct = (d.bytes_done * 100) / d.bytes_total.max(1);
        out.push_str(&format!("] {pct:>3}%"));
    } else {
        out.push_str("…");
    }
    out.push_str("\n\n");
    out.push_str(" Esc / Ctrl-C = Cancel ");
    out
}
```

(`as f64` cast warnings might flare; `#[allow(clippy::cast_*_loss, ...)]` on the function is fine.)

- [ ] **Step 5: Tests + manual smoke**

Run: `cargo test --workspace`
Expected: green.

Run: `cargo run -p mx --release`. Press `F8` (or `Delete`) on a junk file in a tempdir. Verify Confirm modal opens, Yes deletes, panel rescans, file gone.

- [ ] **Step 6: Commit**

```bash
git add crates/mx-core crates/mx-tui crates/mx
git commit -m "feat: F8 Delete with confirm + progress + done/failure modals"
```

---

## Task 12: `mx-fs::copy` — `copy_file` + `copy_tree`

**Files:**
- Create: `crates/mx-fs/src/copy.rs`
- Modify: `crates/mx-fs/src/lib.rs`

`copy_file` streams in 1 MiB chunks, calls a progress callback every chunk, checks the cancel flag, preserves `mtime` and `mode` on Unix. `copy_tree` walks a tree recursively, calling `copy_file` for files and `create_dir` for directories. Symlinks are copied as symlinks (Unix-only).

- [ ] **Step 1: Write `crates/mx-fs/src/copy.rs`**

```rust
//! Streaming copy with progress + cancel + per-file error tolerance.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use camino::Utf8Path;

use mx_core::errors::FsError;

use crate::dir_scan;

const COPY_BUFFER: usize = 1024 * 1024;
const PROGRESS_DEBOUNCE_MS: u128 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteAction {
    /// Overwrite (truncate the destination first).
    Overwrite,
    /// Skip this file.
    Skip,
    /// Cancel the whole op.
    Cancel,
}

/// Callback the worker uses to ask the main loop how to resolve an
/// overwrite. Receives the source / destination paths. Blocks until the
/// main loop replies.
pub type ConflictHandler<'a> = &'a (dyn Fn(&Utf8Path, &Utf8Path) -> OverwriteAction + Send + Sync);

/// Callback the worker uses to publish progress.
pub type ProgressHandler<'a> = &'a (dyn Fn(&Utf8Path, u64, u64) + Send + Sync);

#[derive(Debug, Default)]
pub struct CopyReport {
    pub copied_bytes: u64,
    pub errors: Vec<(camino::Utf8PathBuf, FsError)>,
    pub cancelled: bool,
}

/// Copy a single file. Returns `Ok(bytes_copied)` or an error.
///
/// # Errors
///
/// Any IO failure during open/read/write is mapped to `FsError`.
pub fn copy_file(
    src: &Utf8Path,
    dst: &Utf8Path,
    cancel: &Arc<AtomicBool>,
    progress: ProgressHandler<'_>,
) -> Result<u64, FsError> {
    let src_meta = std::fs::metadata(src).map_err(dir_scan::map_io_pub)?;
    let total = src_meta.len();

    let mut input = File::open(src).map_err(dir_scan::map_io_pub)?;
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dst)
        .map_err(dir_scan::map_io_pub)?;

    let mut buf = vec![0u8; COPY_BUFFER];
    let mut copied: u64 = 0;
    let mut last_tick = Instant::now();
    progress(dst, 0, total);

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Best-effort: drop the partial destination.
            drop(output);
            let _ = std::fs::remove_file(dst);
            return Err(FsError::Cancelled);
        }
        let n = input.read(&mut buf).map_err(dir_scan::map_io_pub)?;
        if n == 0 { break; }
        output.write_all(&buf[..n]).map_err(dir_scan::map_io_pub)?;
        copied += n as u64;
        if last_tick.elapsed().as_millis() >= PROGRESS_DEBOUNCE_MS {
            progress(dst, copied, total);
            last_tick = Instant::now();
        }
    }
    output.flush().map_err(dir_scan::map_io_pub)?;
    drop(output);

    // Best-effort: preserve mtime + mode on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(modified) = src_meta.modified() {
            let _ = filetime_set(dst, modified);
        }
        let mode = src_meta.permissions().mode();
        let perms = std::fs::Permissions::from_mode(mode);
        let _ = std::fs::set_permissions(dst, perms);
    }

    progress(dst, copied, total);
    Ok(copied)
}

/// Copy `src` (file, dir, or symlink) to `dst`. Recurses into directories.
/// On conflict, calls `handler` for each colliding target.
#[must_use]
pub fn copy_tree(
    src: &Utf8Path,
    dst: &Utf8Path,
    cancel: &Arc<AtomicBool>,
    progress: ProgressHandler<'_>,
    handler: ConflictHandler<'_>,
) -> CopyReport {
    let mut report = CopyReport::default();
    copy_inner(src, dst, cancel, progress, handler, &mut report);
    if cancel.load(Ordering::Relaxed) {
        report.cancelled = true;
    }
    report
}

fn copy_inner(
    src: &Utf8Path,
    dst: &Utf8Path,
    cancel: &Arc<AtomicBool>,
    progress: ProgressHandler<'_>,
    handler: ConflictHandler<'_>,
    r: &mut CopyReport,
) {
    if cancel.load(Ordering::Relaxed) { return; }
    let meta = match std::fs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => {
            r.errors.push((src.to_path_buf(), dir_scan::map_io_pub(e)));
            return;
        }
    };

    if meta.is_symlink() {
        // Copy the link itself, not the target.
        #[cfg(unix)]
        if let Ok(target) = std::fs::read_link(src) {
            if dst.exists() {
                match handler(src, dst) {
                    OverwriteAction::Overwrite => { let _ = std::fs::remove_file(dst); }
                    OverwriteAction::Skip => return,
                    OverwriteAction::Cancel => { cancel.store(true, Ordering::Relaxed); return; }
                }
            }
            if let Err(e) = std::os::unix::fs::symlink(&target, dst) {
                r.errors.push((dst.to_path_buf(), dir_scan::map_io_pub(e)));
            }
        }
        return;
    }

    if meta.is_dir() {
        if !dst.exists() {
            if let Err(e) = std::fs::create_dir_all(dst) {
                r.errors.push((dst.to_path_buf(), dir_scan::map_io_pub(e)));
                return;
            }
        }
        let read = match std::fs::read_dir(src) {
            Ok(r) => r,
            Err(e) => { r.errors.push((src.to_path_buf(), dir_scan::map_io_pub(e))); return; }
        };
        for child in read {
            if cancel.load(Ordering::Relaxed) { return; }
            let Ok(child) = child else { continue; };
            let Some(name) = child.file_name().to_str().map(str::to_owned) else { continue; };
            let child_src = src.join(&name);
            let child_dst = dst.join(&name);
            copy_inner(&child_src, &child_dst, cancel, progress, handler, r);
        }
        return;
    }

    // Regular file.
    if dst.exists() {
        match handler(src, dst) {
            OverwriteAction::Overwrite => { /* fall through to copy_file */ }
            OverwriteAction::Skip => return,
            OverwriteAction::Cancel => { cancel.store(true, Ordering::Relaxed); return; }
        }
    }
    match copy_file(src, dst, cancel, progress) {
        Ok(n) => r.copied_bytes += n,
        Err(e) => r.errors.push((dst.to_path_buf(), e)),
    }
}

#[cfg(unix)]
fn filetime_set(path: &Utf8Path, mtime: std::time::SystemTime) -> std::io::Result<()> {
    use std::time::UNIX_EPOCH;
    let dur = mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = dur.as_secs() as i64;
    let nanos = dur.subsec_nanos() as i64;
    let times = [
        libc::timespec { tv_sec: secs, tv_nsec: nanos },
        libc::timespec { tv_sec: secs, tv_nsec: nanos },
    ];
    let cstr = std::ffi::CString::new(path.as_str()).map_err(|_| std::io::Error::other("bad path"))?;
    let r = unsafe { libc::utimensat(libc::AT_FDCWD, cstr.as_ptr(), times.as_ptr(), 0) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn p(d: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(d.path().to_path_buf()).unwrap()
    }
    fn cancel_off() -> Arc<AtomicBool> { Arc::new(AtomicBool::new(false)) }
    fn no_progress() -> impl Fn(&Utf8Path, u64, u64) + Send + Sync { |_, _, _| {} }
    fn always_overwrite() -> impl Fn(&Utf8Path, &Utf8Path) -> OverwriteAction + Send + Sync {
        |_, _| OverwriteAction::Overwrite
    }

    #[test]
    fn copy_file_copies_contents_and_size() {
        let t = tempfile::tempdir().unwrap();
        let src = p(&t).join("a.txt");
        let dst = p(&t).join("b.txt");
        std::fs::write(&src, b"hello world").unwrap();
        let n = copy_file(&src, &dst, &cancel_off(), &no_progress()).unwrap();
        assert_eq!(n, 11);
        assert_eq!(std::fs::read(&dst).unwrap(), b"hello world");
    }

    #[test]
    fn copy_tree_recursive() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("src/inner")).unwrap();
        std::fs::write(t.path().join("src/a.txt"), b"x").unwrap();
        std::fs::write(t.path().join("src/inner/b.txt"), b"yy").unwrap();
        let report = copy_tree(
            &p(&t).join("src"),
            &p(&t).join("dst"),
            &cancel_off(),
            &no_progress(),
            &always_overwrite(),
        );
        assert!(report.errors.is_empty());
        assert_eq!(std::fs::read(t.path().join("dst/a.txt")).unwrap(), b"x");
        assert_eq!(std::fs::read(t.path().join("dst/inner/b.txt")).unwrap(), b"yy");
    }

    #[test]
    fn cancel_aborts_copy_file() {
        let t = tempfile::tempdir().unwrap();
        let src = p(&t).join("big");
        let dst = p(&t).join("big.copy");
        std::fs::write(&src, vec![0u8; 4 * 1024 * 1024]).unwrap();
        let cancel = Arc::new(AtomicBool::new(true)); // pre-cancelled
        let r = copy_file(&src, &dst, &cancel, &no_progress());
        assert_eq!(r.unwrap_err(), FsError::Cancelled);
        assert!(!dst.exists());
    }

    #[test]
    fn conflict_skip_leaves_destination_untouched() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("src")).unwrap();
        std::fs::write(t.path().join("src/a.txt"), b"new").unwrap();
        std::fs::create_dir_all(t.path().join("dst")).unwrap();
        std::fs::write(t.path().join("dst/a.txt"), b"old").unwrap();
        let skip = |_: &Utf8Path, _: &Utf8Path| OverwriteAction::Skip;
        let report = copy_tree(
            &p(&t).join("src"),
            &p(&t).join("dst"),
            &cancel_off(),
            &no_progress(),
            &skip,
        );
        assert!(report.errors.is_empty());
        assert_eq!(std::fs::read(t.path().join("dst/a.txt")).unwrap(), b"old");
    }
}
```

- [ ] **Step 2: Add `libc` to mx-fs deps** for the Unix mtime preservation

In `crates/mx-fs/Cargo.toml`, under `[target.'cfg(unix)'.dependencies]`:

```toml
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

If the table doesn't exist, add it.

- [ ] **Step 3: Update `lib.rs`**

```rust
pub mod copy;
pub use copy::{copy_file, copy_tree, CopyReport, OverwriteAction};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mx-fs copy::tests`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-fs Cargo.lock
git commit -m "feat(fs): copy_file + copy_tree with progress, cancel, conflict handler, mtime/mode preserve"
```

---

## Task 13: `Executor::start_copy` + WorkerMsg::Conflict round-trip

**Files:**
- Modify: `crates/mx-fs/src/executor.rs`

The worker now blocks on a per-worker `mpsc::channel<OverwritePolicy>` whenever it hits a conflict. The main loop owns the `Sender`; on `Command::ResolveConflict`, the executor pushes the decision down the channel.

- [ ] **Step 1: Add `start_copy` to `Executor`**

```rust
    pub fn start_copy(&mut self, src: Vec<Utf8PathBuf>, dst: Utf8PathBuf) -> WorkerId {
        let id = self.alloc_id();
        let tx_main = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let (resume_tx, resume_rx) = mpsc::channel::<mx_core::command::OverwritePolicy>();
        let join = std::thread::Builder::new()
            .name(format!("mx-fs-copy-{}", id.0))
            .spawn(move || {
                run_copy_worker(id, src, dst, tx_main, cancel_for_thread, resume_rx);
            })
            .expect("spawn copy");
        self.workers.insert(id, WorkerHandle { join, cancel, resume: Some(resume_tx) });
        id
    }

    pub fn resolve_conflict(&self, id: WorkerId, policy: mx_core::command::OverwritePolicy) {
        if let Some(w) = self.workers.get(&id) {
            if let Some(ref tx) = w.resume {
                let _ = tx.send(policy);
            }
        }
    }
}

fn run_copy_worker(
    id: WorkerId,
    src_list: Vec<Utf8PathBuf>,
    dst_dir: Utf8PathBuf,
    tx_main: Sender<Event>,
    cancel: Arc<AtomicBool>,
    resume_rx: std::sync::mpsc::Receiver<mx_core::command::OverwritePolicy>,
) {
    use std::sync::Mutex;
    let policy_state: Arc<Mutex<Option<mx_core::command::OverwritePolicy>>> =
        Arc::new(Mutex::new(None)); // sticky All / Skip-All policy

    let progress = {
        let tx = tx_main.clone();
        move |path: &Utf8Path, done: u64, total: u64| {
            let _ = tx.send(Event::Worker(
                id,
                WorkerMsg::Progress {
                    bytes_done: done,
                    bytes_total: total,
                    current_path: path.to_path_buf(),
                },
            ));
        }
    };

    let handler = {
        let tx = tx_main.clone();
        let policy_state = Arc::clone(&policy_state);
        let resume_rx = std::sync::Mutex::new(resume_rx);
        move |src: &Utf8Path, dst: &Utf8Path| -> crate::copy::OverwriteAction {
            // Sticky: if a previous All / Skip-All applies, honour it.
            if let Some(p) = *policy_state.lock().unwrap() {
                return match p {
                    mx_core::command::OverwritePolicy::YesAll => crate::copy::OverwriteAction::Overwrite,
                    mx_core::command::OverwritePolicy::NoAll  => crate::copy::OverwriteAction::Skip,
                    mx_core::command::OverwritePolicy::Cancel => crate::copy::OverwriteAction::Cancel,
                    _ => unreachable!("only sticky variants stored"),
                };
            }
            let _ = tx.send(Event::Worker(
                id,
                WorkerMsg::Conflict {
                    src: src.to_path_buf(),
                    dst: dst.to_path_buf(),
                    kind: detect_conflict_kind(src, dst),
                },
            ));
            // Block until the main loop replies via resolve_conflict.
            let rx = resume_rx.lock().unwrap();
            let decision = match rx.recv() {
                Ok(d) => d,
                Err(_) => mx_core::command::OverwritePolicy::Cancel,
            };
            // Latch sticky policies.
            if matches!(
                decision,
                mx_core::command::OverwritePolicy::YesAll
                    | mx_core::command::OverwritePolicy::NoAll
                    | mx_core::command::OverwritePolicy::Cancel
            ) {
                *policy_state.lock().unwrap() = Some(decision);
            }
            match decision {
                mx_core::command::OverwritePolicy::Yes
                | mx_core::command::OverwritePolicy::YesAll => crate::copy::OverwriteAction::Overwrite,
                mx_core::command::OverwritePolicy::No
                | mx_core::command::OverwritePolicy::NoAll => crate::copy::OverwriteAction::Skip,
                mx_core::command::OverwritePolicy::Cancel => crate::copy::OverwriteAction::Cancel,
            }
        }
    };

    let mut all_errors = Vec::new();
    for src in &src_list {
        if cancel.load(Ordering::Relaxed) { break; }
        let dst = dst_dir.join(src.file_name().unwrap_or(""));
        let report = crate::copy::copy_tree(src, &dst, &cancel, &progress, &handler);
        all_errors.extend(report.errors);
    }
    let msg = if all_errors.is_empty() {
        WorkerMsg::Done
    } else {
        WorkerMsg::Failed { errors: all_errors }
    };
    let _ = tx_main.send(Event::Worker(id, msg));
}

fn detect_conflict_kind(src: &Utf8Path, dst: &Utf8Path) -> mx_core::event::ConflictKind {
    use mx_core::event::ConflictKind;
    let s = std::fs::metadata(src).ok();
    let d = std::fs::metadata(dst).ok();
    let s_dir = s.as_ref().is_some_and(|m| m.is_dir());
    let d_dir = d.as_ref().is_some_and(|m| m.is_dir());
    match (s_dir, d_dir) {
        (false, false) => ConflictKind::FileOverFile,
        (false, true)  => ConflictKind::FileOverDir,
        (true,  false) => ConflictKind::DirOverFile,
        (true,  true)  => ConflictKind::DirOverDir,
    }
}
```

(`use std::sync::mpsc;` at the top of `executor.rs` if not already present.)

- [ ] **Step 2: Add an executor copy test**

Append to `executor::tests`:

```rust
    #[test]
    fn start_copy_succeeds_without_conflict() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.txt"), b"hello").unwrap();
        std::fs::create_dir(t.path().join("dst")).unwrap();
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let src = camino::Utf8PathBuf::from_path_buf(t.path().join("a.txt")).unwrap();
        let dst = camino::Utf8PathBuf::from_path_buf(t.path().join("dst")).unwrap();
        let _id = ex.start_copy(vec![src], dst);
        let _ = drain(&rx, |e| matches!(e, Event::Worker(_, WorkerMsg::Done)));
        ex.join_all();
        assert_eq!(std::fs::read(t.path().join("dst/a.txt")).unwrap(), b"hello");
    }
```

Run: `cargo test -p mx-fs executor::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-fs/src/executor.rs
git commit -m "feat(fs): Executor::start_copy with conflict round-trip + sticky All/Skip-All"
```

---

## Task 14: `update.rs` — handle WorkerMsg::Conflict; wire F5 Copy

**Files:**
- Modify: `crates/mx-core/src/update.rs`
- Modify: `crates/mx/src/app.rs`

When a Conflict arrives, open `Modal::Confirm{kind: Conflict{worker}}` with the 5 buttons (Yes / No / Yes-All / No-All / Cancel).

- [ ] **Step 1: `update.rs` — handle the `Conflict` arm**

Replace the placeholder in `handle_worker_msg`:

```rust
        WorkerMsg::Conflict { src, dst, kind } => {
            use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
            let body = format!(
                "{src}\nalready exists at\n{dst}\n({kind:?})"
            );
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
                focused: 1, // Default to "No".
                kind: ConfirmKind::Conflict { worker: id },
            }));
        }
```

- [ ] **Step 2: `update.rs` — wire `Copy` command**

Lift `Copy` out of the catchall:

```rust
        CommandId::Copy => {
            use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal, PanelSide};
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let src = collect_targets(panel);
            if src.is_empty() {
                return Vec::new();
            }
            let other = match state.focus { PanelSide::Left => 1, PanelSide::Right => 0 };
            let dst = state.panels[other].cwd.clone();
            let body = if src.len() == 1 {
                format!("Copy {} to {}?", src[0], dst)
            } else {
                format!("Copy {} items to {}?", src.len(), dst)
            };
            state.modal = Some(Modal::Confirm(ConfirmDialog {
                title: "Confirm copy".into(),
                body,
                buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
                focused: 0,
                kind: ConfirmKind::StartCopy { src, dst },
            }));
        }
```

- [ ] **Step 3: `app.rs` — handle `StartCopy` and `ResolveConflict`**

```rust
                Command::StartCopy { src, dst } => {
                    let id = executor.start_copy(src, dst);
                    state.workers.insert(
                        id,
                        mx_core::state::WorkerState {
                            id,
                            kind: mx_core::state::WorkerKind::Copy,
                        },
                    );
                }
                Command::ResolveConflict(id, policy) => {
                    executor.resolve_conflict(id, policy);
                }
```

- [ ] **Step 4: Tests + manual smoke**

Run: `cargo test --workspace`
Expected: green.

`cargo run -p mx --release`. Use `Tab` to move to the right panel and create a destination directory there with `F7`. Tab back, select a file, press `F5`, confirm Yes. Verify file copies.

To exercise conflicts: copy the same file twice in succession. The Overwrite? dialog should appear.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-core crates/mx
git commit -m "feat: F5 Copy with confirm + per-conflict overwrite dialog"
```

---

## Task 15: `mx-fs::move_op` + `MoveBackend` trait + Executor::start_move

**Files:**
- Create: `crates/mx-fs/src/move_backend.rs`
- Create: `crates/mx-fs/src/move_op.rs`
- Modify: `crates/mx-fs/src/lib.rs`
- Modify: `crates/mx-fs/src/executor.rs`

`MoveBackend::rename` is the only seam — production uses `LocalBackend` which calls `std::fs::rename`. Tests inject a backend that returns `EXDEV` to force the copy+delete fallback.

- [ ] **Step 1: Write `crates/mx-fs/src/move_backend.rs`**

```rust
//! `MoveBackend` decouples `std::fs::rename` from `move_op` so tests can
//! force the cross-device fallback without two real devices.

use camino::Utf8Path;
use mx_core::errors::FsError;

pub trait MoveBackend: Send + Sync {
    /// Try a same-device rename. Returns `FsError::CrossDevice` to signal the
    /// caller should fall back to copy + delete.
    fn rename(&self, from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError>;
}

pub struct LocalBackend;

impl MoveBackend for LocalBackend {
    fn rename(&self, from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError> {
        match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(e) => {
                // EXDEV on Unix is mapped to FsError::CrossDevice.
                #[cfg(unix)]
                if e.raw_os_error() == Some(libc::EXDEV) {
                    return Err(FsError::CrossDevice);
                }
                Err(crate::dir_scan::map_io_pub(e))
            }
        }
    }
}
```

- [ ] **Step 2: Write `crates/mx-fs/src/move_op.rs`**

```rust
//! Move = rename when same-device, otherwise copy + delete.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use camino::Utf8Path;
use mx_core::errors::FsError;

use crate::copy::{copy_tree, ConflictHandler, OverwriteAction, ProgressHandler};
use crate::delete::delete_tree;
use crate::move_backend::MoveBackend;

#[derive(Debug, Default)]
pub struct MoveReport {
    pub moved: u64,
    pub errors: Vec<(camino::Utf8PathBuf, FsError)>,
    pub cancelled: bool,
}

#[must_use]
pub fn move_tree(
    backend: &dyn MoveBackend,
    src: &Utf8Path,
    dst: &Utf8Path,
    cancel: &Arc<AtomicBool>,
    progress: ProgressHandler<'_>,
    handler: ConflictHandler<'_>,
) -> MoveReport {
    let mut report = MoveReport::default();

    if dst.exists() {
        match handler(src, dst) {
            OverwriteAction::Overwrite => {}
            OverwriteAction::Skip => return report,
            OverwriteAction::Cancel => {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                report.cancelled = true;
                return report;
            }
        }
    }

    match backend.rename(src, dst) {
        Ok(()) => { report.moved += 1; }
        Err(FsError::CrossDevice) => {
            // copy + delete fallback
            let copy = copy_tree(src, dst, cancel, progress, handler);
            report.errors.extend(copy.errors);
            if copy.cancelled {
                report.cancelled = true;
                return report;
            }
            let del = delete_tree(src, cancel);
            report.errors.extend(del.errors);
            if del.cancelled { report.cancelled = true; }
            if report.errors.is_empty() {
                report.moved += 1;
            }
        }
        Err(e) => report.errors.push((src.to_path_buf(), e)),
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn p(d: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(d.path().to_path_buf()).unwrap()
    }
    fn cancel_off() -> Arc<AtomicBool> { Arc::new(AtomicBool::new(false)) }
    fn no_progress() -> impl Fn(&Utf8Path, u64, u64) + Send + Sync { |_, _, _| {} }
    fn always_overwrite() -> impl Fn(&Utf8Path, &Utf8Path) -> OverwriteAction + Send + Sync {
        |_, _| OverwriteAction::Overwrite
    }

    struct AlwaysCrossDevice;
    impl MoveBackend for AlwaysCrossDevice {
        fn rename(&self, _from: &Utf8Path, _to: &Utf8Path) -> Result<(), FsError> {
            Err(FsError::CrossDevice)
        }
    }

    #[test]
    fn fast_path_renames_when_backend_succeeds() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        let r = move_tree(
            &crate::move_backend::LocalBackend,
            &p(&t).join("a"),
            &p(&t).join("b"),
            &cancel_off(),
            &no_progress(),
            &always_overwrite(),
        );
        assert!(r.errors.is_empty());
        assert_eq!(r.moved, 1);
        assert!(t.path().join("b").exists() && !t.path().join("a").exists());
    }

    #[test]
    fn cross_device_falls_back_to_copy_plus_delete() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a"), b"hello").unwrap();
        let r = move_tree(
            &AlwaysCrossDevice,
            &p(&t).join("a"),
            &p(&t).join("b"),
            &cancel_off(),
            &no_progress(),
            &always_overwrite(),
        );
        assert!(r.errors.is_empty());
        assert_eq!(r.moved, 1);
        assert_eq!(std::fs::read(t.path().join("b")).unwrap(), b"hello");
        assert!(!t.path().join("a").exists());
    }
}
```

- [ ] **Step 3: Update `lib.rs`**

```rust
pub mod move_backend;
pub mod move_op;
pub use move_backend::{LocalBackend, MoveBackend};
pub use move_op::{move_tree, MoveReport};
```

- [ ] **Step 4: Add `Executor::start_move`** — same shape as `start_copy`, but using `LocalBackend`:

```rust
    pub fn start_move(&mut self, src: Vec<Utf8PathBuf>, dst: Utf8PathBuf) -> WorkerId {
        let id = self.alloc_id();
        let tx_main = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let (resume_tx, resume_rx) = mpsc::channel::<mx_core::command::OverwritePolicy>();
        let join = std::thread::Builder::new()
            .name(format!("mx-fs-move-{}", id.0))
            .spawn(move || {
                run_move_worker(id, src, dst, tx_main, cancel_for_thread, resume_rx);
            })
            .expect("spawn move");
        self.workers.insert(id, WorkerHandle { join, cancel, resume: Some(resume_tx) });
        id
    }
```

Add the worker function next to `run_copy_worker`:

```rust
fn run_move_worker(
    id: WorkerId,
    src_list: Vec<Utf8PathBuf>,
    dst_dir: Utf8PathBuf,
    tx_main: Sender<Event>,
    cancel: Arc<AtomicBool>,
    resume_rx: std::sync::mpsc::Receiver<mx_core::command::OverwritePolicy>,
) {
    use std::sync::Mutex;
    let policy_state: Arc<Mutex<Option<mx_core::command::OverwritePolicy>>> =
        Arc::new(Mutex::new(None));

    let progress = {
        let tx = tx_main.clone();
        move |path: &Utf8Path, done: u64, total: u64| {
            let _ = tx.send(Event::Worker(
                id,
                WorkerMsg::Progress {
                    bytes_done: done,
                    bytes_total: total,
                    current_path: path.to_path_buf(),
                },
            ));
        }
    };

    let handler = make_conflict_handler(id, tx_main.clone(), Arc::clone(&policy_state), resume_rx);

    let backend = crate::move_backend::LocalBackend;
    let mut all_errors = Vec::new();
    for src in &src_list {
        if cancel.load(Ordering::Relaxed) { break; }
        let dst = dst_dir.join(src.file_name().unwrap_or(""));
        let report = crate::move_op::move_tree(&backend, src, &dst, &cancel, &progress, &handler);
        all_errors.extend(report.errors);
    }
    let msg = if all_errors.is_empty() { WorkerMsg::Done } else { WorkerMsg::Failed { errors: all_errors } };
    let _ = tx_main.send(Event::Worker(id, msg));
}

fn make_conflict_handler(
    id: WorkerId,
    tx: Sender<Event>,
    policy_state: Arc<std::sync::Mutex<Option<mx_core::command::OverwritePolicy>>>,
    resume_rx: std::sync::mpsc::Receiver<mx_core::command::OverwritePolicy>,
) -> impl Fn(&Utf8Path, &Utf8Path) -> crate::copy::OverwriteAction + Send + Sync {
    let resume_rx = std::sync::Mutex::new(resume_rx);
    move |src: &Utf8Path, dst: &Utf8Path| -> crate::copy::OverwriteAction {
        if let Some(p) = *policy_state.lock().unwrap() {
            return match p {
                mx_core::command::OverwritePolicy::YesAll => crate::copy::OverwriteAction::Overwrite,
                mx_core::command::OverwritePolicy::NoAll  => crate::copy::OverwriteAction::Skip,
                mx_core::command::OverwritePolicy::Cancel => crate::copy::OverwriteAction::Cancel,
                _ => unreachable!(),
            };
        }
        let _ = tx.send(Event::Worker(
            id,
            WorkerMsg::Conflict {
                src: src.to_path_buf(),
                dst: dst.to_path_buf(),
                kind: detect_conflict_kind(src, dst),
            },
        ));
        let rx = resume_rx.lock().unwrap();
        let decision = rx.recv().unwrap_or(mx_core::command::OverwritePolicy::Cancel);
        if matches!(
            decision,
            mx_core::command::OverwritePolicy::YesAll
                | mx_core::command::OverwritePolicy::NoAll
                | mx_core::command::OverwritePolicy::Cancel
        ) {
            *policy_state.lock().unwrap() = Some(decision);
        }
        match decision {
            mx_core::command::OverwritePolicy::Yes
            | mx_core::command::OverwritePolicy::YesAll => crate::copy::OverwriteAction::Overwrite,
            mx_core::command::OverwritePolicy::No
            | mx_core::command::OverwritePolicy::NoAll => crate::copy::OverwriteAction::Skip,
            mx_core::command::OverwritePolicy::Cancel => crate::copy::OverwriteAction::Cancel,
        }
    }
}
```

(Refactor `run_copy_worker` to use the new shared `make_conflict_handler` to avoid duplication.)

- [ ] **Step 5: Wire `Move` in `update.rs` and `app.rs`**

In `update.rs`, lift `Move` out of the catchall — same shape as `Copy` but `ConfirmKind::StartMove`:

```rust
        CommandId::Move => {
            use crate::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal, PanelSide};
            let panel = state.focused();
            if panel.entries.is_empty() {
                return Vec::new();
            }
            let src = collect_targets(panel);
            if src.is_empty() {
                return Vec::new();
            }
            let other = match state.focus { PanelSide::Left => 1, PanelSide::Right => 0 };
            let dst = state.panels[other].cwd.clone();
            let body = if src.len() == 1 {
                format!("Move {} to {}?", src[0], dst)
            } else {
                format!("Move {} items to {}?", src.len(), dst)
            };
            state.modal = Some(Modal::Confirm(ConfirmDialog {
                title: "Confirm move".into(),
                body,
                buttons: vec![ConfirmButton::Yes, ConfirmButton::No],
                focused: 0,
                kind: ConfirmKind::StartMove { src, dst },
            }));
        }
```

In `app.rs`:

```rust
                Command::StartMove { src, dst } => {
                    let id = executor.start_move(src, dst);
                    state.workers.insert(
                        id,
                        mx_core::state::WorkerState {
                            id,
                            kind: mx_core::state::WorkerKind::Move,
                        },
                    );
                }
```

- [ ] **Step 6: Tests + smoke**

Run: `cargo test --workspace`
Expected: green.

Smoke: `cargo run -p mx --release`. Move a file from one panel to the other with F6.

- [ ] **Step 7: Commit**

```bash
git add crates/mx-fs crates/mx-core crates/mx
git commit -m "feat: F6 Move with rename fast path + EXDEV copy+delete fallback"
```

---

## Task 16: `mx-tui::view` — render Modal::Error with details

**Files:**
- Modify: `crates/mx-tui/src/view.rs`

The error modal currently renders only `body`. Make it list `details` too.

- [ ] **Step 1: Replace the `Modal::Error(d)` body arm**

```rust
        Modal::Error(d) => render_error_body(d),
```

Add the helper:

```rust
fn render_error_body(d: &mx_core::state::ErrorDialog) -> String {
    let mut out = String::new();
    out.push_str(&d.body);
    if !d.details.is_empty() {
        out.push_str("\n\n");
        out.push_str("Details:\n");
        for line in &d.details {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str("\n[ OK ]");
    out
}
```

`Esc` already closes the modal via the existing dispatch.

- [ ] **Step 2: Build + commit**

```bash
cargo clippy --workspace --all-targets -- -D warnings
git add crates/mx-tui/src/view.rs
git commit -m "feat(tui): render Modal::Error with paginated details"
```

---

## Task 17: Snapshot fixtures for the new modals

**Files:**
- Modify: `crates/mx-tui/tests/render_snapshots.rs`

- [ ] **Step 1: Append fixtures**

```rust
#[test]
fn confirm_modal_two_buttons() {
    use mx_core::state::{ConfirmButton, ConfirmDialog, ConfirmKind, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Confirm(ConfirmDialog {
        title: "Confirm delete".into(),
        body: "Delete /tmp/x.txt?".into(),
        buttons: vec![ConfirmButton::No, ConfirmButton::Yes],
        focused: 0,
        kind: ConfirmKind::Delete { paths: vec!["/tmp/x.txt".into()] },
    }));
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("confirm_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn input_modal_with_value() {
    use mx_core::state::{InputDialog, InputKind, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Input(InputDialog {
        title: "Make directory".into(),
        prompt: "name:".into(),
        value: "newdir".into(),
        cursor: 6,
        kind: InputKind::Mkdir { parent: "/tmp".into() },
    }));
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("input_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn progress_modal_at_50_percent() {
    use mx_core::event::WorkerId;
    use mx_core::state::{Modal, ProgressDialog};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Progress(ProgressDialog {
        title: "Working…".into(),
        current_path: "/tmp/big.bin".into(),
        bytes_done: 50_000_000,
        bytes_total: 100_000_000,
        worker_id: WorkerId(1),
    }));
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("progress_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn error_modal_with_details() {
    use mx_core::state::{ErrorDialog, Modal};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Error(ErrorDialog {
        title: "Operation failed".into(),
        body: "3 errors".into(),
        details: vec![
            "/x/a: permission denied".into(),
            "/x/b: not found".into(),
            "/x/c: io: disk full".into(),
        ],
    }));
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("error_classic_wide", buffer_snapshot(&buf));
}
```

- [ ] **Step 2: Generate + accept**

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH HOME=/Users/boogie INSTA_UPDATE=force \
    cargo test -p mx-tui --test render_snapshots
find crates/mx-tui/tests/snapshots -name '*.snap.new' -delete
cargo test -p mx-tui --test render_snapshots
```

Expected: 4 new snapshots accepted; everything green.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/tests
git commit -m "test(tui): snapshot Confirm / Input / Progress / Error modals"
```

---

## Task 18: CHANGELOG + tag v0.3.0-phase3

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Manual smoke checklist**

Run: `cargo run -p mx --release` and walk through:
- `F7` opens mkdir prompt; type a name; Enter creates the directory; panel rescans.
- `Shift-F6` (or `Ctrl-T`) on a file opens rename prompt pre-filled; submit renames.
- `F8` on a file opens Confirm; Yes deletes; panel rescans.
- `F8` on a directory with content also works (recursive).
- `F5` on a file copies it to the other panel's cwd.
- `F5` over an existing target opens the Overwrite? dialog with five buttons; Yes-All sticks; cancel aborts cleanly.
- `F6` moves; same-device is instant; the source is removed.
- During a long copy, `Esc` opens the cancel question; partial destination is removed.
- Any per-file failure surfaces a Modal::Error with details list.

- [ ] **Step 2: Update `CHANGELOG.md`**

Insert above the `[0.2.0-phase2]` section:

```markdown
## [0.3.0-phase3] - 2026-05-03

### Added
- `mx-fs::copy` (streaming `copy_file`, recursive `copy_tree`), `mx-fs::delete`
  (recursive `delete_tree`), `mx-fs::move_op` (with EXDEV fallback through
  the new `MoveBackend` trait), `mx-fs::ops` (`mkdir`, `rename`).
- `mx-fs::Executor` learns `start_copy`, `start_move`, `start_delete`,
  `cancel`, `resolve_conflict`. Per-worker resume channel for conflict
  round-trips; per-worker cancel atomic; sticky `YesAll` / `NoAll` policy
  state.
- `mx-core`: `Command::ResolveConflict`, `OverwritePolicy`, `ConfirmKind`,
  `InputKind`, `Event::OpFailed`. Generic `Modal::Confirm` and
  `Modal::Input` dispatch in `update()`.
- `mx-tui::view`: rendered `Modal::Confirm` (button cycling), `Modal::Input`
  (inline `▏` cursor), `Modal::Progress` (block-style bar + percentage),
  `Modal::Error` (scrollable details).
- F-key wiring: F5 Copy, F6 Move, F7 Mkdir, F8 Delete, Shift-F6 / Ctrl-T
  Rename. Each pre-confirms with a modal before mutating state.
- 4 new snapshot fixtures.

### Known limitations (Phase 3)
- `max_concurrent_workers` cap deferred — only the executor's natural
  serialisation prevents two long ops at once for now.
- No filesystem watching; manual `Ctrl-R` refreshes after external changes.
- Progress bar shows bytes only — no ETA computation.

## [0.2.0-phase2] - 2026-05-03
```

(Preserve everything below.)

- [ ] **Step 3: Commit + tag + push**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for v0.3.0-phase3"
git tag -a v0.3.0-phase3 -m "Midnight X — Phase 3: file operations (copy/move/delete/mkdir/rename)"
git push origin main
git push origin v0.3.0-phase3
```

---

## Phase 3 — Acceptance criteria

- [ ] `cargo fmt --all -- --check` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo test --workspace` fully green.
- [ ] `cargo doc --workspace --no-deps` warning-free under `RUSTDOCFLAGS=-D warnings`.
- [ ] Manual smoke checklist (Task 18 Step 1) all behaves correctly.
- [ ] All four new modal snapshot fixtures accepted and pass.
- [ ] `v0.3.0-phase3` tag exists locally and on `origin`.
- [ ] No AI-tool attribution in any file or commit.

When all criteria pass, Phase 3 is done. Beyond v1, the spec lists the deferred road: tachyonfx, Lua scripting, mxview rich-preview binary, network volumes, trash/recycle, file-find, F2 menu, hotlist, mouse, filesystem watching, system-wide config, live reload — each its own brainstorm/spec/plan cycle.
