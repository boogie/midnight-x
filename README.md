# Midnight X

A modern, modular Rust two-pane terminal file manager. Lineage: Midnight Commander. Binary: `mx`.

> Status: pre-release. v1 design is complete (see `docs/superpowers/specs/`); Phase 1 (foundation) is the first usable scaffolding.

## Goals

- Faithful **MC keybindings** with sane defaults that work on macOS Terminal.app (incl. `Esc`-prefix chords for F-keys).
- **Modular**: small focused crates, pure `update()` core, single rendering chokepoint.
- **Modern Rust**: `ratatui` + `crossterm`, no `unsafe`, snapshot-tested rendering.
- **Clean-room** reimplementation: no GNU MC source is read or referenced.

## License

Dual-licensed under either of:

- MIT license (`LICENSE-MIT`)
- Apache License, Version 2.0 (`LICENSE-APACHE`)

at your option.

## Contributing

See `CONTRIBUTING.md`. Briefly: clean-room rule (no MC source), conventional commits, no AI-tool attribution in committed files or commit messages.
