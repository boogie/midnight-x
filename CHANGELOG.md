# Changelog

All notable changes to this project will be documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
