# Changelog

All notable changes to this project will be documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0-phase3] - 2026-05-03

### Added
- `mx-fs::copy` (streaming `copy_file` with progress + cancel + mode preservation,
  recursive `copy_tree`, conflict callback, symlinks-as-symlinks).
- `mx-fs::delete` (recursive `delete_tree`, leaf-first, per-file error tolerance,
  cooperative cancel).
- `mx-fs::move_op` + `MoveBackend` trait + `LocalBackend` (rename fast path,
  EXDEV → copy + delete fallback testable via mock backend).
- `mx-fs::ops` (synchronous `mkdir`, `rename`).
- `mx-fs::Executor` learns `start_copy`, `start_move`, `start_delete`,
  `cancel`, `resolve_conflict`. Per-worker resume channel for conflict
  round-trips; per-worker cancel atomic; sticky `YesAll` / `NoAll` policy
  state.
- `mx-core`: `Command::ResolveConflict`, `OverwritePolicy`, `ConfirmKind`,
  `InputKind`, `Event::OpFailed`. Generic `Modal::Confirm` (Tab cycles, Enter
  resolves) and `Modal::Input` (typing, cursor, Enter submits) dispatch in
  `update()`.
- `mx-tui::view`: rendered `Modal::Confirm` (focused-button `>label<`
  markers), `Modal::Input` (inline `▏` cursor glyph), `Modal::Progress`
  (block-style bar + percentage), `Modal::Error` (with details list).
- F-key wiring: F5 Copy, F6 Move, F7 Mkdir, F8 Delete, Shift-F6 / Ctrl-T
  Rename. Each pre-confirms with a modal before mutating state.
- 4 new snapshot fixtures (Confirm / Input / Progress / Error).
- 155 tests across the workspace.

### Changed
- `ConfirmDialog` shape: removed `default_yes`, added `buttons`, `focused`,
  `kind`. `InputDialog`: added `kind` so submit emits the right `Command`.

### Known limitations (Phase 3)
- `max_concurrent_workers` cap deferred — only the executor's natural
  serialisation prevents two long ops at once for now.
- mtime preservation deferred (would require an `unsafe` `utimensat` call,
  forbidden by the workspace `unsafe_code` policy). Mode bits are preserved.
- No filesystem watching; manual `Ctrl-R` refreshes after external changes.
- Progress bar shows bytes only — no ETA computation.

## [0.2.0-phase2] - 2026-05-03

### Added
- New `mx-fs` crate: `Executor` (worker spawning + worker-id allocation),
  synchronous `dir_scan` helper, `format_size` formatter.
- New `mx-preview` crate: `Previewer` trait + `PlainTextPreviewer` with
  binary-file detection via NUL heuristic.
- `chrono` workspace dep for `date_format`-driven mtime rendering.
- `DirEntry` enriched: `size`, `mtime`, `symlink_target`, `symlink_broken`.
- `WorkerKind::DirScan(side)` and `WorkerMsg::DirScanned { side, entries }`.
- All navigation commands wired in `update()`: `CursorUp/Down/PageUp/Down/Home/End`,
  `EnterDir`, `ParentDir`, `FocusOther`, `SwapPanels`.
- Selection commands: `ToggleSelect`, `SelectAll`, `SelectNone`, `InvertSelection`.
- Sort/hidden/rescan commands: `ToggleHidden`, `CycleSort`, `RescanFocused`,
  `RescanBoth`.
- F3 viewer (`Modal::Viewer`) with scrollable text body and `[binary file]`
  placeholder; reuses the cursor commands for scrolling inside the modal.
- Help modal renders the live keymap grouped by category, with chord
  stringification (`KeyChord` → `"ctrl-shift-pgup"`, sequences as space-joined
  strings).
- `mx::app` wires `Executor`; emits initial scans on both panels at startup;
  pumps `Command::OpenViewer` via `PlainTextPreviewer` synchronously and
  reports back via `Event::PreviewLoaded` / `Event::PreviewFailed`.
- Snapshot fixtures: `populated_classic_wide`, `populated_dark_wide`,
  `populated_classic_narrow`, `viewer_classic_wide`. Existing
  `help_modal_classic_wide` regenerated for the new keymap layout.
- 125 tests across the workspace.

### Changed
- `view.rs::render_panel` now renders entries (rows with name/size/mtime),
  cursor reverse-video, selection bullets, dir/symlink/error coloring.
- `view.rs::render_status` summarises focused-panel entry counts and total
  bytes.

### Known limitations (Phase 2)
- File operations (copy/move/delete/mkdir/rename) still scaffolded but
  inactive — that's Phase 3.
- The viewer reads files synchronously on the main thread (capped at 1 MiB by
  default). Phase 3 may move this to a worker.
- Filesystem watching (`notify`) is still out — manual `Ctrl-R` is the way to
  refresh.

## [0.1.0-phase1] - 2026-05-03

### Added
- Workspace skeleton: `mx-core`, `mx-config`, `mx-tui`, `mx` crates.
- v1 design spec at `docs/superpowers/specs/2026-05-03-midnight-x-v1-design.md`.
- `mx-core` data hub: `Event`, `Command`, `CommandId`, `KeyChord`, `KeyCode`,
  `KeyModifiers`, `InputEvent`, `Theme`, `Color`, `Keymap`, `Config` family,
  `FsError`, `AppError`, `ConfigWarning`, `State`, `PanelState`, `Modal`,
  pure `update()` with chord engine and modal-aware dispatch.
- MC-faithful default keymap, including `Esc`-prefix chords and macOS-friendly
  Ctrl-letter aliases.
- `mx-config`: TOML loader with non-fatal warnings, embedded default config,
  key-string parser (`"ctrl-shift-pgup"`, `"esc 1"`).
- `mx-tui`: ratatui rendering through a single `Renderer` with explicit
  layout / widgets / effects / cursor phases (tachyonfx-compat seam),
  `TestBackend`-driven `draw_to_buffer` snapshot helper, theme→Style mapping,
  layout collapse on narrow terminals, input thread.
- `mx` binary: `--config`, `--print-default-config`, `--version`, `--help`;
  raw-mode panic hook; `tracing` logging via `RUST_LOG`; main event loop
  with tick-driven chord flush and clean shutdown.
- CI: GitHub Actions matrix on `ubuntu-latest` and `macos-latest` running
  fmt / clippy / test / doc / cargo-deny.
- Tests: 75 unit tests, 4 snapshot fixtures (classic + dark, wide + narrow,
  help modal), 4 binary smoke tests.

### Known limitations (Phase 1)
- Panels are empty placeholders — no directory scanning yet.
- F3 viewer, file ops, multi-select, sort modes, hidden toggle, and the help
  table are scaffolded but inactive (Phase 2).
- Worker model and `mx-fs` crate do not exist yet.
