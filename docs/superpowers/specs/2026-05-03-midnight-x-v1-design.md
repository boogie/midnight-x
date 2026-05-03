# Midnight X — v1 Design Spec

- **Project name:** Midnight X
- **Binary:** `mx`
- **Date:** 2026-05-03
- **Status:** Approved design — pending implementation plan
- **License:** Dual MIT / Apache-2.0
- **Implementation language:** Rust (stable)

---

## 1. Goals & Scope

Midnight X (`mx`) is a modern, modular Rust reimplementation of the Midnight Commander style two-pane file manager.

### v1 in scope

- Two-pane file browser
- Navigation (arrows, page, home/end, parent, enter dir, focus toggle, swap panels)
- Multi-selection
- Basic file operations: copy, move, delete, mkdir, rename
- Built-in plain-text file viewer (F3, read-only pager)
- Sort modes, hidden-file toggle, manual rescan
- TOML config + remappable keymap with MC-faithful defaults
- Two themes: `classic` (MC blue), `dark`
- Modal dialogs: Confirm, Input, Progress (with cancel), Error, Help, QuitConfirm

### v1 explicitly out of scope (designed-for, not built)

Built-in editor (`mcedit`-style F4); pluggable rich previewer / `mxview` separate binary; tachyonfx animation layer; Lua scripting; network volumes (FTP/SFTP/SMB); trash/recycle on delete; file-find; user menu (F2); directory hotlist; bottom shell command-line; mouse; filesystem watching; live config reload; system-wide config; deb/rpm/AUR packaging; Windows CI.

### Non-goals

Feature parity with mc(1). Drop-in compatibility with MC config files. Subshell support. VFS plugin compatibility with mc.

### Clean-room rule

Implementation works only from MC's observable behavior (man page, screenshots, running tool). Contributors **must not** read MC source code, and MC source must not be pasted into design or review channels. This protects the MIT/Apache license posture.

---

## 2. Locked Decisions Summary

| # | Topic | Decision |
|---|---|---|
| 1 | Scope | Two-pane browser + basic file ops; viewer is plain text |
| 2 | Platform | macOS + Linux primary; Windows-friendly code, no Windows CI in v1 |
| 3 | Name / binary | Midnight X / `mx` |
| 4 | TUI library | `ratatui` + `crossterm` |
| 5 | Concurrency | Synchronous core + threaded workers; `mpsc` channel for events |
| 6 | Config / keys | TOML config, remappable keymap, MC-faithful defaults; scripting deferred but seam designed in |
| 7 | Tests | Unit + tempdir-backed integration + `insta` snapshot tests on rendered `Buffer` |
| 8 | License | Dual MIT/Apache-2.0; clean-room (no MC source) |
| 9 | Architecture | MVU (Elm-style): pure `update` and `view`, typed `Command`s, executor handles side effects |

---

## 3. Architecture

### Event loop

```
                   ┌────────────────────────┐
   keypresses ──►  │  crossterm input thread│ ──► Event ─┐
                   └────────────────────────┘            │
                                                         ▼
   worker msgs ──► (mpsc::Sender<Event>) ──────────► Event Queue
                                                         │
                                                         ▼
                              ┌──────────────────────────────────────┐
                              │  main loop:                           │
                              │   ev = rx.recv()                      │
                              │   (state, cmds) = update(state, ev)   │
                              │   for cmd in cmds: executor.run(cmd)  │
                              │   renderer.draw(&state, dt)           │
                              └──────────────────────────────────────┘
                                                         │
                                                         ▼
                              ┌──────────────────────────────────────┐
                              │  Executor                            │
                              │   - spawns worker threads             │
                              │   - sends Event::Worker(...) back     │
                              └──────────────────────────────────────┘
```

- **Main thread** owns `State` and the `Terminal`. Reads events, runs `update`, renders. Never blocks on I/O.
- **Input thread** wraps `crossterm::event::read()` and forwards events into the same `mpsc` channel.
- **Worker threads** are spawned per long operation (copy, move, delete, dir scan). They send `Event::Worker(WorkerId, WorkerMsg)` via the channel.
- **`update`** is `fn(State, Event) -> (State, Vec<Command>)` — pure. Owns no I/O, no threads. Every state change goes through it.
- **`Executor`** consumes `Command`s and turns them into thread spawns / terminal-mode changes. It is the only impure component.
- **`Renderer`** owns the `terminal.draw(|frame| …)` closure (see §6 for tachyonfx-compat seams). `view(&State, &layout, frame)` is pure — writes to the frame's buffer.

### Why MVU

A pure `update` + pure `view` make snapshot tests cheap (drive a state through events, snapshot the buffer) and give scripting a clean dispatch target later (any producer of `CommandId` events plugs in).

---

## 4. Repo & Crate Layout

Cargo workspace. Splitting boundaries now is cheap; splitting later is expensive.

```
midnight-x/
├── Cargo.toml                 # [workspace]
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── CHANGELOG.md
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── rust-toolchain.toml        # pin stable
├── rustfmt.toml               # 4-space, max_width 100
├── clippy.toml
├── deny.toml                  # cargo-deny: license + advisories
├── .editorconfig
├── .gitignore
├── .gitattributes
├── .github/workflows/
│   ├── ci.yml                 # fmt, clippy -D warnings, test, deny, llvm-cov
│   └── release.yml            # tag → cargo-dist binaries
├── crates/
│   ├── mx/                    # the `mx` binary (thin: arg parsing, wiring)
│   │   └── src/main.rs
│   ├── mx-core/               # State, Event, Command, update(), pure logic
│   │   └── src/lib.rs
│   ├── mx-tui/                # ratatui view(), Renderer, widgets, theme
│   │   └── src/lib.rs
│   ├── mx-fs/                 # file ops, worker model, dir scanning
│   │   └── src/lib.rs
│   ├── mx-config/             # TOML loader, keymap parsing, defaults
│   │   └── src/lib.rs
│   └── mx-preview/            # Previewer trait + PlainTextPreviewer (v1 only impl)
│       └── src/lib.rs
├── docs/
│   ├── architecture.md
│   ├── keys.md
│   ├── config.md
│   └── superpowers/specs/
└── man/
    └── mx.1
```

### Boundary rules

- `mx-core` is the **data hub**: it owns the public type vocabulary used everywhere else (`State`, `Event`, `Command`, `CommandId`, `KeyChord`, `KeyCode`, `KeyModifiers`, `InputEvent`, `Theme`, `Keymap`, `Config`, `FsError`, `WorkerMsg`, etc.). It has **zero terminal dependencies** — no `ratatui`, no `crossterm`. Its `KeyCode` / `KeyModifiers` enums mirror crossterm's but are our own, so `update()` can be tested without a terminal.
- `mx-tui` depends on `mx-core`. It owns the `Renderer`, the input thread, and the translation `crossterm::event::KeyEvent → mx_core::InputEvent`.
- `mx-fs` depends on `mx-core` (uses `Event`, `WorkerMsg`, `Command`, `FsError`). It is the only crate that calls `std::fs`, spawns worker threads, or owns the worker side of the channel. `update()` only sees `Command`s and `Event::Worker(...)`.
- `mx-config` depends on `mx-core` (parses TOML *into* the types `mx-core` defines). It does **not** depend on `mx-tui` or `mx-fs`. It is leaf with respect to the app — `mx-core` ↛ `mx-config`.
- `mx-preview` depends on `mx-core` and is the seam for the future `mxview` binary and rich previewers. v1: `Previewer` trait + `PlainTextPreviewer`.

Dependency graph (arrows point from dependent to dependency):

```
            mx (binary)
           /   |   |   \
          v    v   v    v
       mx-tui mx-fs mx-config mx-preview
          \    |   |    /
           v   v   v   v
              mx-core            (no app dependencies)
```

### Lints / hygiene

- `clippy::all` denied; selected `clippy::pedantic` lints opted in (full pedantic is too noisy).
- `#![forbid(unsafe_code)]` in every crate.
- `cargo deny` in CI for license drift and known advisories.
- MSRV pinned in `rust-toolchain.toml`.
- `unwrap` / `expect` forbidden in `mx-core` and `mx-fs` library code (clippy lint). Allowed at the binary layer with a `// expect: <reason>` comment.

---

## 5. Core Data Model (`mx-core`)

```rust
// ---- State ---------------------------------------------------------------

pub struct State {
    pub panels:        [PanelState; 2],   // left, right
    pub focus:         PanelSide,
    pub modal:         Option<Modal>,     // single-modal in v1
    pub status:        StatusLine,
    pub workers:       HashMap<WorkerId, WorkerState>,
    pub config:        Arc<Config>,       // immutable per session
    pub pending_chord: Vec<KeyChord>,     // multi-key chord buffer
    pub pending_since: Option<Instant>,
    pub should_quit:   bool,
}

pub struct PanelState {
    pub cwd:         Utf8PathBuf,         // camino — guaranteed UTF-8
    pub entries:     Arc<[DirEntry]>,     // shared cheaply across update calls
    pub cursor:      usize,
    pub scroll:      usize,
    pub selection:   BTreeSet<usize>,
    pub sort:        SortMode,
    pub show_hidden: bool,
    pub loading:     bool,
}

pub enum Modal {
    Confirm(ConfirmDialog),
    Input(InputDialog),
    Progress(ProgressDialog),
    Error(ErrorDialog),
    Help,
    QuitConfirm,
}

// ---- Event ---------------------------------------------------------------

pub enum Event {
    Input(InputEvent),                     // mx-core's own type, not crossterm's
    Command(CommandId),                    // synthesized from keymap resolution
    Worker(WorkerId, WorkerMsg),
    Tick,                                   // ~10 Hz
    Resize(u16, u16),
}

pub enum InputEvent {
    Key { code: KeyCode, mods: KeyModifiers },
    Mouse(MouseEvent),                     // reserved; not used in v1
    Paste(String),
}

// KeyCode / KeyModifiers are mx-core enums that mirror crossterm's, so
// mx-core stays terminal-free. mx-tui's input thread translates
// `crossterm::event::Event` → `mx_core::InputEvent` at the boundary.

pub enum WorkerMsg {
    Progress { bytes_done: u64, bytes_total: u64, current_path: Utf8PathBuf },
    DirScanned { entries: Vec<DirEntry> },
    Conflict { src: Utf8PathBuf, dst: Utf8PathBuf, kind: ConflictKind },
    Done,
    Failed { errors: Vec<(Utf8PathBuf, FsError)> },
}

// ---- Command (intent, not action) ----------------------------------------

pub enum Command {
    RescanDir(PanelSide),
    StartCopy   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartMove   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartDelete { paths: Vec<Utf8PathBuf> },
    Mkdir       { parent: Utf8PathBuf, name: String },
    Rename      { from: Utf8PathBuf, to: Utf8PathBuf },
    CancelWorker(WorkerId),
    ResolveConflict(WorkerId, ConflictDecision),
    OpenViewer(Utf8PathBuf),
    Quit,
}

// ---- Pure functions ------------------------------------------------------

pub fn update(state: State, event: Event) -> (State, Vec<Command>);
pub fn view(state: &State, layout: &FrameLayout, frame: &mut ratatui::Frame); // mx-tui
```

### Type choices

- **`Utf8PathBuf` (`camino`)** — accepted tradeoff: loses non-UTF-8 paths on Linux. Wins: `Display` + `serde` cleanly, no `OsStr` poison through the codebase. Non-UTF-8 entries surface as `DirEntry::Unreadable { lossy_name, .. }` — visible, not selectable.
- **`Arc<[DirEntry]>`** — `update` calls that don't change the entry list are a single Arc clone, not a 10k-entry copy. `update` runs per keystroke.
- **`Modal: Option<...>`** — single modal in v1. If we ever stack, the type changes deliberately.
- **`Command` is intent, not action** — tests assert "this event produced this command list" with zero threads.
- **`WorkerId(u64)`** — typed newtype, generated by an atomic counter in the executor.
- **`FsError`** is a `mx-fs`-owned enum (not `std::io::Error`) — see §10.

---

## 6. UI Surfaces (`mx-tui`)

### Main layout (no modal)

```
┌─ Left: /Users/boogie ──────────────┐┌─ Right: /tmp ──────────────────────┐
│ ..                                 ││ ..                                 │
│ Workspace/                         ││ build/                             │
│ src/                               ││ logs/                              │
│ Cargo.toml                  1.2 K  ││ output.txt                  43 K   │
│ README.md                   4.8 K  ││ ...                                │
├────────────────────────────────────┤├────────────────────────────────────┤
│ /Users/boogie/Cargo.toml    1.2 K  ││ 14 files, 2 dirs            128 K  │
└────────────────────────────────────┘└────────────────────────────────────┘
 Hint: F1 Help  F3 View  F5 Copy  F6 Move  F7 Mkdir  F8 Del  F10 Quit
```

- Two panels via `Layout::horizontal([Constraint::Percentage(50); 2])`.
- On terminals < 80 cols, collapse to one panel; the other is hidden until resize / `Tab`.
- Each panel: title (current path, middle-truncated), entry list, footer info line.
- Focused panel highlights its title in accent color; unfocused dims.
- Bottom hint line is one row, populated from a `KeyHint` table keyed by current modal/focus.

### Panel rows

- Columns: `Name` (flex), `Size` (right-aligned, human-readable), `Modified` (auto-hidden first when narrow).
- Directories: name only (no size), trailing `/`, listed first within the chosen sort.
- Symlinks: `→ target` on the info line; broken symlinks rendered in error color.
- Cursor row: reverse video. Selected rows: accent foreground + bullet glyph in a left margin.

### Modals (centered, fixed-width, dim overlay)

- **Confirm** — body + `[ Yes ]  [ No ]`; default = `No` for destructive ops.
- **Input** — single-line text field; pre-fills name for rename.
- **Progress** — current path, byte progress bar, files done/total, ETA, `[ Cancel ]`.
- **Error** — message + `[ OK ]`; non-fatal.
- **Help** — scrollable list of bindings, grouped (Navigation / File ops / View / Misc).
- **QuitConfirm** — only if a worker is in flight; otherwise quit is immediate.

### Theme

- `Theme` is data — a struct of named color slots (`bg`, `fg`, `accent`, `selection_bg`, `error_fg`, `dir_fg`, `symlink_fg`, …). Defined in `mx-core` so `mx-config` can populate it without depending on `mx-tui`.
- Two built-ins (also defined as `Theme` constants in `mx-core`): `classic` (MC blue), `dark` (terminal-default-bg).
- `mx-tui` *consumes* the theme — it owns the mapping from `Theme` slots to `ratatui::style::Style`. All color use goes through that mapping; no hardcoded `Color::Blue` outside it.

### Tachyonfx forward-compatibility

The animation/effects layer is not built in v1. The architecture must keep these seams so it drops in cleanly later:

1. **Single rendering chokepoint.** All drawing through `Renderer` in `mx-tui`. No code outside `Renderer` calls `terminal.draw`.
2. **Explicit render phases inside the closure**, fixed order:
   ```
   1. layout  — compute Rects: panels, modal, status
   2. widgets — view(&State, &layout, frame)
   3. effects — renderer.effects.process(dt, frame.buffer_mut(), &layout);  // v1: empty
   4. cursor  — set_cursor when a text input modal is focused
   ```
   Step 3 is a real call site in v1 iterating an empty `Vec<Box<dyn Effect>>`.
3. **Frame-time delta plumbed.** `Renderer::draw(&State, dt: Duration)`. v1 ignores `dt`; tachyonfx needs it.
4. **Layout rects are first-class.** Layout step returns `FrameLayout { left_panel, right_panel, modal: Option<Rect>, status, .. }` — effects can scope to named regions.
5. **`view()` stays pure.** Snapshot tests render through `Renderer::draw_to_buffer(&state, area, dt = 0)` so the empty-effects pass is exercised.
6. **Effect triggers come from state diffs**, not from inside `update`. A future `effects_for(prev, next) -> Vec<Effect>` adds them.
7. **No `tachyonfx` dep in v1.** Just the seams.

### Resize

`Event::Resize` triggers re-layout next frame. Cursor visibility preserved (scroll if needed). No rescan.

### Not in v1's UI

Bottom command line / shell prompt; file-find dialog; user menu (F2); directory hotlist; tree view.

---

## 7. File Ops & Worker Model (`mx-fs`)

### Worker lifecycle

- `Executor` owns a `Sender<Event>` and a `WorkerId` counter.
- On `Command::Start{Copy,Move,Delete}`, allocate id, spawn `std::thread`, store `(WorkerId, JoinHandle, Arc<AtomicBool> /* cancel */)`.
- Worker runs to completion or until `cancel.load() == true`. Sends `WorkerMsg::Progress` on a debounced cadence and exactly one terminal message: `Done` or `Failed`.
- Main loop maps progress into `state.workers` and `Modal::Progress`; on terminal message, removes the entry and emits `Command::RescanDir(side)`.

### Copy semantics

- **Recursive** for directories.
- **Per-file streaming**, fixed buffer (default 1 MiB, configurable). Not `std::fs::copy` for big files — we want progress.
- **Permissions and mtime preserved** on Unix (`fchmod` + `futimens`). Owner/group not preserved (would need root). On Windows: best-effort.
- **Symlinks copied as symlinks**, not followed.
- **Sparse files not preserved** in v1 — they expand. Documented limitation.
- **Cross-device move** falls back to copy + delete, atomically per file. If the copy half fails, source is preserved.
- **Same-device move** uses `std::fs::rename` (atomic).
- **Conflict policy:** if destination exists, worker pauses and emits `WorkerMsg::Conflict { src, dst, kind }`. Main loop opens overwrite dialog: `Yes / No / All / Skip All / Cancel`. Worker resumes via a `resume(decision)` channel. Default: `No`. This is the only worker→main→worker round-trip.
- **Per-file error tolerance:** a single `EACCES` doesn't kill the whole copy. Worker collects errors into `Vec<(Utf8PathBuf, FsError)>` and reports at the end.

### Delete semantics

- Recursive, depth-first leaf-first.
- Confirmation modal lists count + total size when selection > 1 file.
- No trash/recycle in v1 — direct unlink. (Trash via `trash` crate deferred.)

### Mkdir / rename

- Synchronous on the main thread. Instant. No worker.
- Validation in `update()`: empty name, names with `/`, names that already exist → `Modal::Error`.

### Directory scan

- Initial panel scan (app start, `cd`, `RescanDir`) is a worker, even when local-disk-instant — same machinery handles slow network mounts later.
- Sort applied in worker before sending entries back.
- `stat`s done eagerly during scan (we need size, mtime, type).
- v1 does **not** stream partial scans; `WorkerMsg::DirScanned` fires once with the full list.

### Progress debouncing

- Worker emits progress at most every **50 ms** OR every **64 MiB**, whichever first. Cancel-check happens on the same cadence.

### Cancellation

- `Arc<AtomicBool>` per worker. Cancel button in progress modal flips it. Worker checks at every progress tick and at every file boundary. Partially-copied destination file is removed on cancel; previously-completed files are kept.

### Threading hygiene

- One thread per active worker. Concurrency cap (default 2) — additional workers queue. Configurable via `[ops] max_concurrent_workers`.
- No global thread pool. No `rayon`.

---

## 8. Keymap, Commands, Dispatch

### Named commands (`CommandId`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandId {
    // navigation
    CursorUp, CursorDown, CursorPageUp, CursorPageDown, CursorHome, CursorEnd,
    EnterDir, ParentDir, FocusOther, SwapPanels,
    // selection
    ToggleSelect, SelectAll, SelectNone, InvertSelection,
    // file ops
    Copy, Move, Delete, Mkdir, Rename,
    // view
    View,
    ToggleHidden, CycleSort,
    // app
    Help, QuitConfirm, Quit, Cancel,
    // refresh
    RescanFocused, RescanBoth,
}
```

### Resolution flow

```
KeyEvent ──► Keymap::resolve(KeyChord) ──► CommandId
                                              │
                                              ▼
                            update(state, Event::Command(CommandId))
```

- **`Keymap`** is prefix-aware: a binding is `Vec<KeyChord>`. Lookup returns `Match(CommandId)`, `Prefix`, or `NoMatch`.
- The input thread synthesizes `Event::Command(CommandId)` after a successful keymap lookup. Keys that don't resolve are dropped (or fed to a focused text-input modal first).
- **`update()` matches on `CommandId`, never on raw `KeyCode`s.** Zero `KeyCode::F(5)` matches in `mx-core`. This is what makes scripting later just another producer of `CommandId`.

### Multi-key chord engine

- `State.pending_chord: Vec<KeyChord>`, `pending_since: Option<Instant>`.
- On `Event::Input(key)`: push, look up.
  - `Match(cmd)` → clear pending, dispatch.
  - `Prefix` → keep pending, set `pending_since = now`.
  - `NoMatch` → if pending starts with a binding that *does* match a shorter prefix, fire that and reprocess the new key fresh; else drop pending and treat new key as fresh.
- On `Event::Tick`: if `pending_since` older than `chord_timeout`, flush — fire whatever the pending sequence resolves to standalone (bare `Esc` → `Cancel`), or drop.
- **`chord_timeout`** in `[input]` config, default **500 ms**.
- Snapshot tests drive chord behavior deterministically with controlled `Tick`s.

### MC-faithful defaults

| Sequence | Command |
|---|---|
| `↑ ↓` | CursorUp / CursorDown |
| `PgUp / PgDn` | CursorPageUp / CursorPageDown |
| `Home / End` | CursorHome / CursorEnd |
| `Enter` | EnterDir (no-op on files in v1) |
| `Backspace` | ParentDir |
| `Tab` | FocusOther |
| `Ctrl-U` | SwapPanels |
| `Insert` / `*` | ToggleSelect / InvertSelection |
| `Ctrl-A` | SelectAll |
| `F1` | Help |
| `F3` | View |
| `F5` | Copy |
| `F6` | Move |
| `F7` | Mkdir |
| `F8` / `Delete` | Delete |
| `Shift-F6` / `Ctrl-T` | Rename |
| `F10` / `Ctrl-Q` | QuitConfirm |
| `Ctrl-R` | RescanFocused |
| `Ctrl-H` | ToggleHidden |
| `Ctrl-S` | CycleSort |
| `esc` | Cancel |
| `esc 1` | Help (= F1) |
| `esc 3` | View (= F3) |
| `esc 5` | Copy (= F5) |
| `esc 6` | Move (= F6) |
| `esc 7` | Mkdir (= F7) |
| `esc 8` | Delete (= F8) |
| `esc 0` | QuitConfirm (= F10) |

`esc 2`, `esc 4`, `esc 9` are reserved (their F-key counterparts are deferred).

On macOS, "Option as Meta" in Terminal.app/iTerm2 makes `Option-1` = `Esc 1` work for free. Documented in README.

### TOML keymap syntax

```toml
[keymap]
"esc 1"     = "help"
"esc 0"     = "quit_confirm"
"ctrl-r r"  = "rescan_both"      # arbitrary multi-key chord
"f5"        = "<unbind>"
```

- Space-separated for sequences.
- `"<unbind>"` removes a default binding.
- Parser uses `serde` + a small key-string grammar (`"ctrl-shift-pgup"`).
- Errors at config-load time are non-fatal — logged + surfaced as `Modal::Error` on first frame; defaults are used.

### Modal-aware dispatch

- `Esc` always closes current modal first (via `Cancel`).
- `Enter` in `Confirm` activates the default button.
- `Tab` cycles focus between buttons/fields inside dialogs.
- These are handled in `update()` by checking `state.modal` *before* dispatching the resolved `CommandId`. Result: `CommandId::Quit` cannot fire while a dialog is open.

### Future scripting hook (named, not built)

- Scripting will add `Event::Command(CommandId)` from a runtime, plus likely `Event::ScriptCommand(String, Args)` dispatched by a string→handler map.
- v1 designs the seam by making `update()` already match on a typed enum that is producer-agnostic.

---

## 9. Configuration (`mx-config`)

### File location

- Linux/macOS: `$XDG_CONFIG_HOME/mx/config.toml`, falling back to `~/.config/mx/config.toml`. Resolved via `directories`.
- Windows (best-effort): `%APPDATA%\mx\config.toml`.
- `--config <path>` CLI flag overrides.
- Missing file is fine — defaults apply silently.
- `mx --print-default-config` writes the canonical default to stdout (no auto-create on first run).

### Schema

```toml
[ui]
theme              = "classic"     # "classic" | "dark"
show_hidden        = false
panel_ratio        = 50            # 1..99
truncate_path      = "middle"      # "middle" | "start" | "end"
date_format        = "%Y-%m-%d %H:%M"

[ui.columns]
size               = true
modified           = true          # auto-hides on narrow terminals regardless

[input]
chord_timeout_ms   = 500
double_click_ms    = 250           # reserved for future mouse support

[ops]
copy_buffer_kib    = 1024
preserve_mtime     = true
preserve_mode      = true
follow_symlinks    = false
max_concurrent_workers = 2

[keymap]
# "f5" = "<unbind>"
# "ctrl-c" = "copy"

[theme.classic]
# bg            = "#0000aa"
# fg            = "#cccccc"
# accent        = "#ffff55"
# ...

[logging]
level              = "warn"        # off|error|warn|info|debug|trace
file               = "auto"        # "auto" | absolute path | "off"
```

### Loading & validation

- `mx-config::load(path: Option<&Path>) -> (Config, Vec<ConfigWarning>)`.
- Loading **never panics**. Per-field errors → `ConfigWarning`s, surfaced as a single startup `Modal::Error`. A typo in one binding never locks the user out.
- Two genuinely fatal cases (stderr + non-zero exit): explicit `--config <path>` is unreadable; the TOML doesn't parse at all.
- Out-of-range numerics clamp + warn.
- Unknown keys produce warnings, never failures (forward-compat).

### Default source of truth

- `pub const DEFAULT_CONFIG_TOML: &str = include_str!("default.toml");`. Parsed at compile-time via a const-eval-friendly path (or at startup once).
- Same string is emitted by `mx --print-default-config`. Docs and binary cannot diverge.
- Snapshot test: deserialize the embedded default, re-serialize, assert equal.
- Round-trip tests for keymap parsing.

### `Config` struct

```rust
// Defined in mx-core (the data hub).
pub struct Config {
    pub ui:      UiConfig,
    pub input:   InputConfig,
    pub ops:     OpsConfig,
    pub keymap:  Keymap,        // resolved: defaults merged with user
    pub theme:   Theme,         // resolved: palette baked in
    pub logging: LoggingConfig,
}
```

- `Config` and its component types (`UiConfig`, `InputConfig`, `OpsConfig`, `Keymap`, `Theme`, `LoggingConfig`) live in `mx-core`. `mx-config` depends on `mx-core` and provides `load(path) -> (Config, Vec<ConfigWarning>)`.
- `Keymap` and `Theme` are *resolved* products of (defaults + user overrides) — the rest of the app sees a single source of truth.
- `serde::Deserialize` impls live alongside the types in `mx-core` (gated behind a `serde` feature so the data hub doesn't force a serde dep on consumers that don't want it — though in practice every crate enables it).

### Live reload — out of v1

Restart the app to pick up config changes.

---

## 10. Errors, Logging, Panics

### Error taxonomy

```rust
// mx-fs — boundary with the OS
pub enum FsError {
    NotFound, PermissionDenied, AlreadyExists,
    NotADirectory, IsADirectory, CrossDevice,
    Cancelled, Io(String),
}

// mx-core — transport in WorkerMsg / Modal::Error
pub enum AppError {
    Fs { path: Utf8PathBuf, source: FsError },
    Config(String),
    Internal(&'static str),
}

// mx-config — load-time
pub enum ConfigError { Parse(String), Io(String) }
pub struct ConfigWarning { pub key: String, pub message: String }
```

- `impl From<io::Error> for FsError` at the boundary; no `std::io::Error` leaks upward.
- No `anyhow` in library crates. `anyhow` is allowed in `mx::main()` glue.
- `thiserror` for `Display` impls.

### Error UX

- File-op errors are collected per-file. Worker emits `WorkerMsg::Failed { errors }`. Main loop opens an `Error` modal: "3 files could not be copied. [ Details ] [ OK ]".
- Single-file errors (mkdir, rename) → small `Error` modal.
- Startup errors → stderr + non-zero exit (only the two fatal cases).
- Config warnings → one consolidated startup modal.
- We never silently swallow an error — unknown ones go to the log + a generic "see log" modal.

### Logging

- `tracing` + `tracing-subscriber` + `tracing-appender`.
- Default destination: `$XDG_STATE_HOME/mx/mx.log`, rotated by size (1 MiB + 1 archive). Windows: `%LOCALAPPDATA%\mx\logs\mx.log`.
- `level = "off"` skips installing the subscriber (zero overhead).
- Spans around: app startup, config load, each worker (id, kind, paths-count), each modal open/close, each `update()` event at `trace`. No file contents, ever.
- `RUST_LOG` env overrides config (`tracing_subscriber::EnvFilter`).

### Panics

- Custom panic hook installed in `mx::main()` *before* entering raw mode:
  1. Restore terminal (`disable_raw_mode`, leave alternate screen, show cursor).
  2. Write panic payload + backtrace to log.
  3. Print "mx crashed: see <log path>" to stderr.
  4. Re-raise.
- Worker thread panics caught by executor (`JoinHandle::join` returns `Err`). Synthesizes `WorkerMsg::Failed` so UI degrades gracefully.
- No `catch_unwind` inside `update()` — bugs there are real crashes; tests cover it.
- `unwrap`/`expect` policy: forbidden in `mx-core` and `mx-fs` library code (clippy lint). Allowed at the binary layer with a `// expect: <reason>` comment.

### Termination

- Normal quit: `state.should_quit = true`. Loop drains pending workers (with "quitting…" status), waits up to 200 ms, disables raw mode, exits.
- Quit with workers in flight: `Modal::QuitConfirm`; on confirm, cancel all and quit.
- Signals: `SIGINT` → `Cancel`. `SIGTERM` → graceful quit. `SIGHUP` → skip modal, restore terminal, kill workers, exit.

---

## 11. Testing Strategy

### Layout

| Crate | Test types | Location | Tools |
|---|---|---|---|
| `mx-core` | unit + snapshot of `update()` traces | `src/` `#[cfg(test)]` + `tests/` | `insta` |
| `mx-tui` | snapshot of rendered `Buffer` | `tests/snapshots/` | `insta`, `ratatui::backend::TestBackend` |
| `mx-fs` | integration against real FS (tempdirs) | `tests/` | `tempfile`, `assert_fs` |
| `mx-config` | round-trip + golden-file | `tests/` | `insta` for goldens |
| `mx-preview` | unit on trait + `PlainTextPreviewer` | `src/` | — |
| `mx` (bin) | smoke: `--version`, `--print-default-config` | `tests/` | `assert_cmd` |

### `update()` tests

```rust
#[test]
fn pressing_f5_with_one_selected_opens_copy_confirm() {
    let mut s = State::with_panels(panel("/a", &["x.txt"]), panel("/b", &[]));
    s.panels[0].selection.insert(0);

    let (s, cmds) = update(s, Event::Command(CommandId::Copy));

    assert!(matches!(s.modal, Some(Modal::Confirm(_))));
    assert!(cmds.is_empty());
    insta::assert_debug_snapshot!(s.modal);
}
```

Snapshots target end-of-sequence state. Reviewed via `cargo insta review`.

### `mx-tui` snapshot tests

Render with `TestBackend` at fixed sizes (e.g., 100×30 and 60×24 for narrow). Snapshot a custom `Display` of the `Buffer` (text grid + per-cell-style table for non-default cells). Color regressions show as diffs.

Minimum fixture set (≥ 10): empty panels, files, multi-select, focus toggle, classic theme, dark theme, narrow terminal collapse, Unicode names, each modal kind, error modal with details list.

### `mx-fs` integration tests

- Each test gets a fresh `TempDir`; tests run a worker via the same `Executor` API the binary uses.
- Coverage: copy file, copy dir tree, copy with overwrite Yes/No/All/Skip/Cancel, move same-device fast path, move cross-device fallback, delete recursive, cancel mid-copy, mkdir/rename validation, symlink-as-symlink, broken symlink visible in scan, permission-denied per-file, mtime preserved.
- **Cross-device coverage** without a second device: a `MoveBackend` trait whose default impl wraps `std::fs::rename`; tests inject a backend that returns `EXDEV` to force the copy+delete fallback. Production never sees the trait.
- Slow / large-file tests: `#[ignore]`, run on CI via `cargo test -- --include-ignored` in a separate slow job.

### `mx-config` tests

- Default TOML round-trip (parse → re-serialize → equal — drift detector).
- Golden config files (`tests/data/full.toml`, `typos.toml`, `missing.toml`); snapshot resulting `(Config, Vec<ConfigWarning>)`.
- Keymap parser: ~30 table-driven tuples.

### Property tests

- `proptest` for keymap string round-trip.
- `proptest` for `update()` invariants: any sequence of `Event`s never panics, never produces `cursor >= entries.len()`, never stacks `Modal` > 1.

### Coverage

- `cargo llvm-cov` in CI. **≥ 85 %** line coverage on `mx-core` and `mx-fs`. Reported, not gated.

### Out of v1

- Real PTY end-to-end harnesses (too fragile, low ROI without an editor or scripting).
- TTY rendering across emulators (we trust `crossterm`).
- Multi-process / IPC.

### CI matrix

`ubuntu-latest`, `macos-latest`. Stable Rust. Steps:
1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`
4. `cargo test --workspace -- --include-ignored` (separate slow job)
5. `cargo deny check`
6. `cargo llvm-cov --workspace --summary-only`

`windows-latest` is not in the matrix in v1 (decision Q2.C).

---

## 12. Repo Conventions, Dependencies, Release

### Locked v1 dependencies

| Crate | Where | Why |
|---|---|---|
| `ratatui` | `mx-tui` | TUI |
| `crossterm` | `mx-tui` (input) + `mx` (panic hook, raw mode) | terminal layer |
| `serde`, `serde_derive` | `mx-config`, `mx-core` | config + stable serialization |
| `toml` | `mx-config` | config format |
| `camino` | re-exported via `mx-core` | UTF-8 paths |
| `directories` | `mx-config` | XDG / platform dirs |
| `thiserror` | every lib crate | error `Display` |
| `tracing`, `tracing-subscriber`, `tracing-appender` | `mx`; libs pull `tracing` only | logging |
| `tempfile`, `insta`, `assert_fs`, `assert_cmd`, `proptest` | dev-deps | testing |
| `pico-args` (or hand-rolled) | `mx` | tiny CLI surface |

### Not added in v1 (deliberate "no")

- `tokio` — went sync (Q5).
- `anyhow` in libs — explicit error types. Optional in `mx::main`.
- `clap` — start with `pico-args`; if CLI grows past 4 flags, swap.
- `rayon` — no parallelism beyond per-worker threads.
- `notify` (FS watching) — manual rescan is fine.
- `tachyonfx`, `mlua`, image crates, SFTP crates — held for future specs.

### Repo conventions

- Conventional commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`). Hook (`gitlint`), not CI-enforced.
- One change per PR. PR template: what / why / how tested.
- `CHANGELOG.md` updated per PR under "Unreleased". Release moves the section under a version heading.
- Issue templates: `bug.md`, `feature.md`. Bug template requires `mx --version`, OS, terminal, repro.
- Single long-lived branch: `main`. Releases are tagged commits on `main`.
- `CONTRIBUTING.md` covers: clean-room rule (no MC source), commit style, test expectations, **no Claude/AI attribution in committed files or commit messages** — the project reads as the maintainer's work.
- `CODE_OF_CONDUCT.md`: Contributor Covenant 2.1.
- `SECURITY.md`: contact address; no GitHub Security Advisories ceremony in v1.

### Release / distribution

- v0.1 = "first usable release," not feature-complete v1. v1.0 ships when this design is fully implemented and the snapshot fixture matrix is met.
- `cargo-dist` for releases: tag `vX.Y.Z` on `main` → GitHub Action builds `mx` for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`. Tarballs + checksums on the GitHub Release.
- crates.io publish for all workspace crates per release. `mx-*` libs and `mx` binary all `publish = true` (so `cargo install mx` works).
- Homebrew tap (`homebrew-mx`) generated by `cargo-dist`.
- No deb/rpm/AUR in v1 — community can package; we'll link.

### Versioning

- SemVer at the workspace level.
- v0.x = pre-stability; minor bumps may break.
- v1.0 = stability commitment on the binary CLI and config schema.
- `cargo-semver-checks` in CI as a tripwire (not a gate) for breaking changes to lib crate public types.

### Performance budget (informational, not gated)

- Cold start to first frame: < 50 ms on a 1k-entry directory.
- Frame render: < 5 ms p99 at 200×60.
- Directory scan: < 100 ms for 10k local entries.
- Memory: < 30 MiB RSS for the same workload.
- Measured via `criterion` benches for `update()` and `view()`. Tracked over time.

### Documentation deliverables

- `README.md`: what it is, screenshot, install, basic keys, link to docs.
- `docs/keys.md`: full default keymap.
- `docs/config.md`: every TOML field.
- `docs/architecture.md`: condensed §1–§3 of this spec, for contributors.
- `man/mx.1` man page (generated from a small markdown source).

---

## 13. Out of v1 — Designed-For

The v1 design is shaped to allow each of these without rework. Not a roadmap commitment.

1. **Built-in editor (`mcedit`-style, F4)** — new `mx-editor` crate, same MVU pattern, opens via `Command::OpenEditor(path)`.
2. **`mxview` separate binary + pluggable previewers** — image protocols (Kitty/iTerm2/Sixel), markdown render, syntax highlight, hex, JSON tree. v1 ships `PlainTextPreviewer` only.
3. **Tachyonfx visual effects layer** — seams in §6.
4. **Lua scripting (`mlua`)** — emits `Event::Command(CommandId)` + likely a `Event::ScriptCommand(String, Args)`.
5. **Network volumes (SFTP first)** — a `VfsBackend` trait that local FS implicitly conforms to. Tokio scoped to that backend if needed.
6. **Trash/recycle for delete** — `trash` crate.
7. **File-find dialog**, **user menu (F2)**, **directory hotlist**, **bottom shell command-line**, **mouse support** — distinct follow-on specs.
8. **Filesystem watching** — `notify` crate for live panel updates.
9. **System-wide config**, **live config reload**.
10. **deb / rpm / AUR** packaging.
11. **Windows CI** when there is demand.
