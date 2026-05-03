# Midnight X — Phase 1: Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Rust workspace and ship a runnable `mx` binary that opens an alternate-screen TUI, draws two empty panels in the classic theme, processes the multi-key chord engine (incl. `Esc`-prefix), and exits cleanly via `F10` or `Ctrl-Q`. No filesystem operations, no rendering of entries, no file viewer — those land in Phase 2.

**Architecture:** MVU. `mx-core` is the data hub (no terminal deps); `mx-config` parses TOML into `mx-core` types; `mx-tui` owns the `Renderer` and translates `crossterm` events into `mx-core::InputEvent`; `mx` wires the main loop and panic hook. Render pipeline is split into four explicit phases (layout → widgets → effects-empty → cursor) so tachyonfx can drop in later. `update()` is pure: `(State, Event) -> (State, Vec<Command>)`.

**Tech Stack:** Rust 1.85 (stable), `ratatui`, `crossterm`, `serde`, `toml`, `camino`, `directories`, `thiserror`, `tracing` + `tracing-subscriber` + `tracing-appender`, `pico-args`, `anyhow` (binary glue only). Dev-deps: `insta`, `tempfile`, `assert_cmd`.

**Reference design doc:** `docs/superpowers/specs/2026-05-03-midnight-x-v1-design.md` (Sections 1–10, 12).

---

## File map

Files this plan creates. Each file has one responsibility.

```
midnight-x/
├── Cargo.toml                          # [workspace] manifest
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── CHANGELOG.md
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── SECURITY.md
├── rust-toolchain.toml
├── rustfmt.toml
├── deny.toml
├── .editorconfig
├── .gitattributes
├── .github/workflows/ci.yml
├── crates/
│   ├── mx-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate root, re-exports, lints
│   │       ├── input.rs                # InputEvent, KeyCode, KeyModifiers, KeyChord
│   │       ├── command.rs              # CommandId, Command
│   │       ├── event.rs                # Event, WorkerMsg, WorkerId, ConflictKind
│   │       ├── theme.rs                # Theme, Color, classic/dark presets
│   │       ├── keymap.rs               # Keymap, Lookup, default bindings
│   │       ├── config.rs               # Config + sub-config structs, defaults
│   │       ├── errors.rs               # FsError, AppError, ConfigWarning
│   │       ├── state.rs                # State, PanelState, Modal, StatusLine
│   │       └── update.rs               # update() — Phase 1 subset
│   ├── mx-config/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # load(), apply_overrides()
│   │       ├── default.toml            # canonical default config text
│   │       └── keymap_str.rs           # parse "ctrl-shift-pgup" → KeyChord
│   ├── mx-tui/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate root, re-exports
│   │       ├── input_xlate.rs          # crossterm::KeyEvent → mx_core::InputEvent
│   │       ├── input_thread.rs         # spawn input reader thread
│   │       ├── theme_styles.rs         # Theme → ratatui::style::Style
│   │       ├── layout.rs               # FrameLayout + compute()
│   │       ├── view.rs                 # view(&State, &FrameLayout, &mut Frame)
│   │       └── renderer.rs             # Renderer + draw + draw_to_buffer
│   └── mx/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # entry point
│           ├── cli.rs                  # pico-args parsing
│           ├── panic_hook.rs           # restore terminal + log panic
│           └── app.rs                  # main loop wiring (State, Executor, Renderer)
└── docs/
    └── superpowers/specs/...           # already exists
```

Phase 2 will add `crates/mx-fs/` and `crates/mx-preview/` and flesh out `view.rs` to render entries; those are out of scope here.

---

## Conventions

- **TDD**: every functional task is `write test → run (expect fail) → implement → run (expect pass) → commit`.
- **Commits**: Conventional Commits (`feat:`, `fix:`, `test:`, `chore:`, `docs:`, `refactor:`). **No** `Co-Authored-By: Claude` trailer. **No** "Generated with Claude Code" line.
- **Lints**: every crate `lib.rs`/`main.rs` starts with `#![forbid(unsafe_code)] #![deny(clippy::all)] #![warn(clippy::pedantic)]`.
- **Error display**: every error type uses `thiserror::Error`.
- **Imports**: prefer fully-qualified paths in tests (`mx_core::input::KeyChord`) for clarity.
- **MSRV**: pinned at `1.85` in `rust-toolchain.toml`.

---

## Task 1: Workspace manifest, licenses, README

**Files:**
- Create: `Cargo.toml`
- Create: `LICENSE-MIT`
- Create: `LICENSE-APACHE`
- Create: `README.md`
- Create: `CHANGELOG.md`
- Create: `CONTRIBUTING.md`
- Create: `CODE_OF_CONDUCT.md`
- Create: `SECURITY.md`
- Create: `rust-toolchain.toml`
- Create: `rustfmt.toml`
- Create: `deny.toml`
- Create: `.editorconfig`
- Create: `.gitattributes`

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel    = "stable"
components = ["rustfmt", "clippy"]
profile    = "minimal"
```

(MSRV `1.85` is enforced by `rust-version` in the workspace manifest; the toolchain itself tracks current stable so we don't download an extra older channel.)

- [ ] **Step 2: Write `rustfmt.toml`**

```toml
edition           = "2021"
max_width         = 100
hard_tabs         = false
tab_spaces        = 4
newline_style     = "Unix"
use_field_init_shorthand = true
```

- [ ] **Step 3: Write `deny.toml`**

```toml
[advisories]
yanked = "warn"
ignore = []

[licenses]
allow = [
    "MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-DFS-2016",
    "Zlib", "MPL-2.0", "CC0-1.0",
]
confidence-threshold = 0.93

[bans]
multiple-versions = "warn"

[sources]
unknown-registry = "deny"
unknown-git      = "deny"
```

- [ ] **Step 4: Write `.editorconfig`**

```
root = true

[*]
charset = utf-8
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
indent_style = space
indent_size = 4

[*.md]
trim_trailing_whitespace = false
```

- [ ] **Step 5: Write `.gitattributes`**

```
* text=auto eol=lf
*.snap text eol=lf
*.toml text eol=lf
*.md text eol=lf
```

- [ ] **Step 6: Write `LICENSE-MIT`**

```
MIT License

Copyright (c) 2026 Bártházi András and Midnight X contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 7: Write `LICENSE-APACHE`**

Download text from https://www.apache.org/licenses/LICENSE-2.0.txt or copy the standard Apache License 2.0 text verbatim. The full text is fixed and required.

```bash
curl -fsSL https://www.apache.org/licenses/LICENSE-2.0.txt -o LICENSE-APACHE
```

- [ ] **Step 8: Write `Cargo.toml` (workspace root)**

```toml
[workspace]
resolver = "2"
members  = [
    "crates/mx-core",
    "crates/mx-config",
    "crates/mx-tui",
    "crates/mx",
]

[workspace.package]
edition       = "2021"
rust-version  = "1.85"
authors       = ["Bártházi András and Midnight X contributors"]
license       = "MIT OR Apache-2.0"
repository    = "https://github.com/boogie/midnight-x"
homepage      = "https://github.com/boogie/midnight-x"
readme        = "README.md"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
all      = { level = "deny",  priority = -1 }
pedantic = { level = "warn",  priority = -1 }
module_name_repetitions = "allow"
must_use_candidate      = "allow"

[workspace.dependencies]
camino             = { version = "1.1", features = ["serde1"] }
crossterm          = "0.28"
directories        = "5"
pico-args          = "0.5"
ratatui            = { version = "0.28", default-features = false, features = ["crossterm"] }
serde              = { version = "1", features = ["derive"] }
thiserror          = "1"
toml               = "0.8"
tracing            = "0.1"
tracing-appender   = "0.2"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow             = "1"

# dev-dependencies (pinned in workspace for consistency)
insta              = { version = "1.40", features = ["yaml"] }
tempfile           = "3"
assert_cmd         = "2"

# Internal crates
mx-core            = { path = "crates/mx-core" }
mx-config          = { path = "crates/mx-config" }
mx-tui             = { path = "crates/mx-tui" }

[profile.release]
lto           = "thin"
codegen-units = 1
strip         = "symbols"
```

- [ ] **Step 9: Write `README.md`**

```markdown
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
```

- [ ] **Step 10: Write `CHANGELOG.md`**

```markdown
# Changelog

All notable changes to this project will be documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Workspace skeleton: `mx-core`, `mx-config`, `mx-tui`, `mx` crates.
- v1 design spec at `docs/superpowers/specs/2026-05-03-midnight-x-v1-design.md`.
```

- [ ] **Step 11: Write `CONTRIBUTING.md`**

```markdown
# Contributing to Midnight X

## Clean-room rule

This project is a clean-room reimplementation of the Midnight Commander style
two-pane file manager. **Do not read, paste, or reference GNU Midnight
Commander source code** in code, issues, PRs, or design discussion. The man
page, screenshots, and observed behavior of the running tool are fine.

## Commit style

Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`).

**Do not** add AI-tool attribution to commits or committed files. No
`Co-Authored-By: Claude ...` trailers. No "Generated with Claude Code" lines.

## Code style

- `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- `#![forbid(unsafe_code)]` is enforced workspace-wide.
- Library crates do not use `anyhow`; they define explicit error enums.

## Tests

- All new behavior is covered by tests.
- Snapshot tests use `insta`; review with `cargo insta review`.
- Run `cargo test --workspace` before opening a PR.
```

- [ ] **Step 12: Write `CODE_OF_CONDUCT.md`**

Use Contributor Covenant 2.1. Source: `https://www.contributor-covenant.org/version/2/1/code_of_conduct/`. Save the markdown verbatim, replacing the contact placeholder with `andras@barthazi.hu`.

```bash
curl -fsSL https://raw.githubusercontent.com/EthicalSource/contributor_covenant/release/content/version/2/1/code_of_conduct.md -o CODE_OF_CONDUCT.md
sed -i.bak 's/\[INSERT CONTACT METHOD\]/andras@barthazi.hu/g' CODE_OF_CONDUCT.md && rm CODE_OF_CONDUCT.md.bak
```

- [ ] **Step 13: Write `SECURITY.md`**

```markdown
# Security Policy

## Reporting a vulnerability

Please email `andras@barthazi.hu` with details. Do not open a public issue.

We will acknowledge receipt within 7 days and aim to ship a fix within 30 days
for confirmed issues. There is no bug bounty.

## Supported versions

Pre-1.0 releases (`0.x`): only the latest minor receives security fixes.
Post-1.0: the latest stable release receives security fixes.
```

- [ ] **Step 14: Update `.gitignore` to track `Cargo.lock`**

The repo's `.gitignore` (committed with the spec) currently ignores `Cargo.lock`. Because `mx` is a binary, we should commit the lockfile for reproducible builds.

Modify `.gitignore`. Replace its contents with:

```
/target
**/*.rs.bk
.DS_Store
*.log
```

(Removed the `Cargo.lock` line; everything else is unchanged.)

- [ ] **Step 15: Sanity-check the workspace manifest**

Workspace member crates don't exist yet, so `cargo metadata` will not fully succeed. That's OK — Task 2 creates them. Skip cargo invocations until Task 2.

- [ ] **Step 16: Commit**

```bash
git add Cargo.toml LICENSE-MIT LICENSE-APACHE README.md CHANGELOG.md CONTRIBUTING.md \
        CODE_OF_CONDUCT.md SECURITY.md rust-toolchain.toml rustfmt.toml deny.toml \
        .editorconfig .gitattributes .gitignore
git commit -m "chore: scaffold workspace root, licenses, and contributor docs"
```

---

## Task 2: Empty crate skeletons

Get `cargo build` green with empty crates so subsequent tasks can iterate quickly.

**Files:**
- Create: `crates/mx-core/Cargo.toml`
- Create: `crates/mx-core/src/lib.rs`
- Create: `crates/mx-config/Cargo.toml`
- Create: `crates/mx-config/src/lib.rs`
- Create: `crates/mx-tui/Cargo.toml`
- Create: `crates/mx-tui/src/lib.rs`
- Create: `crates/mx/Cargo.toml`
- Create: `crates/mx/src/main.rs`

- [ ] **Step 1: Write `crates/mx-core/Cargo.toml`**

```toml
[package]
name        = "mx-core"
version     = "0.0.1"
description = "Midnight X — pure data types and update() core (no terminal deps)"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[lints]
workspace = true

[dependencies]
camino    = { workspace = true }
serde     = { workspace = true }
thiserror = { workspace = true }
tracing   = { workspace = true }
```

- [ ] **Step 2: Write `crates/mx-core/src/lib.rs`**

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]
```

- [ ] **Step 3: Write `crates/mx-config/Cargo.toml`**

```toml
[package]
name        = "mx-config"
version     = "0.0.1"
description = "Midnight X — TOML config loader"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[lints]
workspace = true

[dependencies]
mx-core     = { workspace = true }
camino      = { workspace = true }
directories = { workspace = true }
serde       = { workspace = true }
thiserror   = { workspace = true }
toml        = { workspace = true }
tracing     = { workspace = true }
```

- [ ] **Step 4: Write `crates/mx-config/src/lib.rs`**

```rust
//! Midnight X config: parses TOML into types defined in `mx-core` and reports
//! warnings for partially-bad input without ever panicking.

#![forbid(unsafe_code)]
```

- [ ] **Step 5: Write `crates/mx-tui/Cargo.toml`**

```toml
[package]
name        = "mx-tui"
version     = "0.0.1"
description = "Midnight X — ratatui Renderer, layout, theme styles, input thread"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[lints]
workspace = true

[dependencies]
mx-core    = { workspace = true }
crossterm  = { workspace = true }
ratatui    = { workspace = true }
tracing    = { workspace = true }

[dev-dependencies]
insta = { workspace = true }
```

- [ ] **Step 6: Write `crates/mx-tui/src/lib.rs`**

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]
```

- [ ] **Step 7: Write `crates/mx/Cargo.toml`**

```toml
[package]
name        = "mx"
version     = "0.0.1"
description = "Midnight X — modern Rust two-pane terminal file manager (mc-style)"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[[bin]]
name = "mx"
path = "src/main.rs"

[lints]
workspace = true

[dependencies]
mx-core            = { workspace = true }
mx-config          = { workspace = true }
mx-tui             = { workspace = true }
crossterm          = { workspace = true }
pico-args          = { workspace = true }
anyhow             = { workspace = true }
tracing            = { workspace = true }
tracing-appender   = { workspace = true }
tracing-subscriber = { workspace = true }
directories        = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
tempfile   = { workspace = true }
```

- [ ] **Step 8: Write `crates/mx/src/main.rs`**

```rust
#![forbid(unsafe_code)]

fn main() {
    println!("mx 0.0.1 (skeleton)");
}
```

- [ ] **Step 9: Verify build**

Run: `cargo build --workspace`
Expected: compiles cleanly, no warnings. `target/debug/mx` exists.

Run: `cargo run -p mx`
Expected: prints `mx 0.0.1 (skeleton)`.

- [ ] **Step 10: Verify lints**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings, exit 0.

Run: `cargo fmt --all -- --check`
Expected: no diff, exit 0.

- [ ] **Step 11: Commit**

```bash
git add crates
git commit -m "chore: scaffold mx-core, mx-config, mx-tui, mx crates"
```

---

## Task 3: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Test (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: fmt
        run: cargo fmt --all -- --check
      - name: clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: test
        run: cargo test --workspace --all-targets
      - name: doc
        run: cargo doc --workspace --no-deps --document-private-items
        env:
          RUSTDOCFLAGS: -D warnings

  deny:
    name: cargo-deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "chore: add GitHub Actions CI (fmt, clippy, test, doc, cargo-deny)"
```

---

## Task 4: `mx-core::input` — InputEvent, KeyCode, KeyModifiers, KeyChord

**Files:**
- Create: `crates/mx-core/src/input.rs`
- Modify: `crates/mx-core/src/lib.rs`
- Test: in-file `#[cfg(test)] mod tests`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/input.rs`:

```rust
//! Input vocabulary: terminal-free analogues of crossterm's KeyCode /
//! KeyModifiers, plus `KeyChord` (one keystroke) and `InputEvent` (anything
//! the input thread can deliver). Defining these here keeps `mx-core`
//! terminal-agnostic so `update()` can be tested without crossterm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub ctrl:  bool,
    pub shift: bool,
    pub alt:   bool,
}

impl KeyModifiers {
    pub const NONE: Self = Self { ctrl: false, shift: false, alt: false };

    #[must_use]
    pub const fn ctrl()  -> Self { Self { ctrl: true,  ..Self::NONE } }
    #[must_use]
    pub const fn shift() -> Self { Self { shift: true, ..Self::NONE } }
    #[must_use]
    pub const fn alt()   -> Self { Self { alt: true,   ..Self::NONE } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    #[must_use]
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    /// Convenience: a chord with no modifiers.
    #[must_use]
    pub const fn bare(code: KeyCode) -> Self {
        Self { code, mods: KeyModifiers::NONE }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyChord),
    Paste(String),
    /// Reserved for future mouse support.
    MouseStub,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_chord_constructors() {
        let bare = KeyChord::bare(KeyCode::Esc);
        assert_eq!(bare.code, KeyCode::Esc);
        assert!(!bare.mods.ctrl && !bare.mods.shift && !bare.mods.alt);

        let ctrl_q = KeyChord::new(KeyCode::Char('q'), KeyModifiers::ctrl());
        assert!(ctrl_q.mods.ctrl);
        assert_eq!(ctrl_q.code, KeyCode::Char('q'));
    }

    #[test]
    fn key_modifiers_default_is_none() {
        assert_eq!(KeyModifiers::default(), KeyModifiers::NONE);
    }

    #[test]
    fn key_chord_eq_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyChord::bare(KeyCode::F(5)));
        assert!(set.contains(&KeyChord::bare(KeyCode::F(5))));
        assert!(!set.contains(&KeyChord::new(KeyCode::F(5), KeyModifiers::ctrl())));
    }
}
```

Modify `crates/mx-core/src/lib.rs`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
```

- [ ] **Step 2: Run tests; expect compile-fail-then-pass**

Run: `cargo test -p mx-core input::tests`
Expected: compiles and passes (this is the first module so the tests pass on first run — there's no separate "implementation" step here because the types ARE the implementation).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/input.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add input vocabulary (KeyCode, KeyModifiers, KeyChord, InputEvent)"
```

---

## Task 5: `mx-core::command` — CommandId, Command

**Files:**
- Create: `crates/mx-core/src/command.rs`
- Modify: `crates/mx-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/command.rs`:

```rust
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
    StartCopy   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartMove   { src: Vec<Utf8PathBuf>, dst: Utf8PathBuf },
    StartDelete { paths: Vec<Utf8PathBuf> },
    Mkdir       { parent: Utf8PathBuf, name: String },
    Rename      { from: Utf8PathBuf, to: Utf8PathBuf },
    CancelWorker(WorkerId),
    OpenViewer(Utf8PathBuf),
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_id_deserializes_from_snake_case() {
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct W { id: CommandId }
        let w: W = toml::from_str(r#"id = "quit_confirm""#).unwrap();
        assert_eq!(w.id, CommandId::QuitConfirm);
    }

    #[test]
    fn command_id_round_trips_through_toml() {
        // Place inside a single-key TOML doc so we exercise the serde rename.
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct W { id: CommandId }
        let w = W { id: CommandId::CycleSort };
        let s = toml::to_string(&w).unwrap();
        assert!(s.contains("cycle_sort"), "got {s}");
        let back: W = toml::from_str(&s).unwrap();
        assert_eq!(w, back);
    }
}
```

Modify `crates/mx-core/src/lib.rs`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod event;
pub mod state;
```

(`event` and `state` are forward-declared; the next tasks fill them in. We add them now so `command.rs` compiles when those modules are added.)

- [ ] **Step 2: Run tests; expect compile-fail**

Run: `cargo test -p mx-core command::tests`
Expected: FAIL with "unresolved module `event`" / "unresolved module `state`". This is the test driving us to define those modules next.

- [ ] **Step 3: Add minimal `event` and `state` placeholders so this task compiles in isolation**

Create `crates/mx-core/src/event.rs`:

```rust
//! Event vocabulary. Filled in fully in Task 6.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorkerId(pub u64);
```

Create `crates/mx-core/src/state.rs`:

```rust
//! Application state. Filled in fully in Task 9.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide { Left, Right }
```

- [ ] **Step 4: Run tests; expect pass**

Run: `cargo test -p mx-core command::tests`
Expected: PASS. Both round-trip tests succeed.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-core/src/command.rs crates/mx-core/src/event.rs \
        crates/mx-core/src/state.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add CommandId/Command vocabulary with snake_case TOML serde"
```

---

## Task 6: `mx-core::event` — Event, WorkerMsg, ConflictKind

**Files:**
- Modify: `crates/mx-core/src/event.rs`

- [ ] **Step 1: Write the failing tests**

Replace the contents of `crates/mx-core/src/event.rs` with:

```rust
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
        // Filled out in Phase 2 once DirEntry exists.
        // For Phase 1 we just need the variant to exist so update() can pattern-match.
        placeholder: (),
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
    Tick { dt: Duration },
    Resize { cols: u16, rows: u16 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{KeyChord, KeyCode};

    #[test]
    fn event_can_wrap_input_command_and_tick() {
        let _e1 = Event::Input(InputEvent::Key(KeyChord::bare(KeyCode::Esc)));
        let _e2 = Event::Command(CommandId::Quit);
        let _e3 = Event::Tick { dt: Duration::from_millis(100) };
    }

    #[test]
    fn worker_id_is_eq() {
        assert_eq!(WorkerId(7), WorkerId(7));
        assert_ne!(WorkerId(7), WorkerId(8));
    }
}
```

Modify `crates/mx-core/src/lib.rs` to add the missing module declarations:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod state;
```

- [ ] **Step 2: Run tests; expect compile-fail**

Run: `cargo test -p mx-core event::tests`
Expected: FAIL with "unresolved module `errors`". We define it in Task 7 next, but for this task we add a placeholder so the build is green.

- [ ] **Step 3: Add minimal `errors` placeholder**

Create `crates/mx-core/src/errors.rs`:

```rust
//! Error types. Filled in fully in Task 7.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError;
```

- [ ] **Step 4: Run tests; expect pass**

Run: `cargo test -p mx-core event::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mx-core/src/event.rs crates/mx-core/src/errors.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add Event/WorkerMsg/ConflictKind vocabulary"
```

---

## Task 7: `mx-core::errors` — FsError, AppError, ConfigWarning

**Files:**
- Modify: `crates/mx-core/src/errors.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/mx-core/src/errors.rs` with:

```rust
//! Error types used across the workspace. `mx-fs` maps `std::io::Error` into
//! `FsError` at the OS boundary so neither `mx-core` nor downstream consumers
//! need to know about `io::ErrorKind`.

use camino::Utf8PathBuf;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FsError {
    #[error("not found")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("destination already exists")]
    AlreadyExists,
    #[error("path is not a directory")]
    NotADirectory,
    #[error("path is a directory")]
    IsADirectory,
    #[error("operation crosses devices and a copy+delete fallback is required")]
    CrossDevice,
    #[error("operation cancelled")]
    Cancelled,
    #[error("io: {0}")]
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AppError {
    #[error("filesystem error at {path}: {source}")]
    Fs { path: Utf8PathBuf, #[source] source: FsError },
    #[error("config: {0}")]
    Config(String),
    #[error("internal: {0}")]
    Internal(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigWarning {
    pub key: String,
    pub message: String,
}

impl ConfigWarning {
    #[must_use]
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self { key: key.into(), message: message.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fs_error_displays_human_messages() {
        assert_eq!(FsError::NotFound.to_string(), "not found");
        assert_eq!(FsError::Cancelled.to_string(), "operation cancelled");
    }

    #[test]
    fn app_error_includes_path_and_source() {
        let p: Utf8PathBuf = "/tmp/x".into();
        let e = AppError::Fs { path: p.clone(), source: FsError::PermissionDenied };
        assert!(e.to_string().contains("/tmp/x"));
        assert!(e.to_string().contains("permission denied"));
    }

    #[test]
    fn config_warning_constructor() {
        let w = ConfigWarning::new("ui.theme", "unknown theme \"foo\"");
        assert_eq!(w.key, "ui.theme");
        assert!(w.message.contains("foo"));
    }
}
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core errors::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/errors.rs
git commit -m "feat(core): add FsError, AppError, ConfigWarning"
```

---

## Task 8: `mx-core::theme` — Theme, Color, presets

**Files:**
- Create: `crates/mx-core/src/theme.rs`
- Modify: `crates/mx-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/theme.rs`:

```rust
//! Theme is data. The mapping from `Theme` slots to `ratatui::style::Style`
//! lives in `mx-tui::theme_styles`; this crate has no terminal dependency.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Color {
    /// Hex `#RRGGBB`.
    Hex(u8, u8, u8),
    /// 16-color ANSI index 0..15.
    Ansi(u8),
    /// Take the terminal's default foreground/background.
    Default,
}

impl Color {
    /// Parse `"#RRGGBB"`, `"ansi:N"` (0..15), or `"default"`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("default") {
            return Some(Color::Default);
        }
        if let Some(hex) = s.strip_prefix('#') {
            if hex.len() == 6 {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                return Some(Color::Hex(r, g, b));
            }
        }
        if let Some(idx) = s.strip_prefix("ansi:") {
            let n: u8 = idx.parse().ok()?;
            if n < 16 { return Some(Color::Ansi(n)); }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub bg:           Color,
    pub fg:           Color,
    pub accent:       Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub error_fg:     Color,
    pub dir_fg:       Color,
    pub symlink_fg:   Color,
    pub status_bg:    Color,
    pub status_fg:    Color,
    pub modal_bg:     Color,
    pub modal_fg:     Color,
}

impl Theme {
    /// Classic Midnight Commander blue. The default at first run.
    pub const CLASSIC: Self = Self {
        bg:           Color::Hex(0x00, 0x00, 0xaa),
        fg:           Color::Hex(0xcc, 0xcc, 0xcc),
        accent:       Color::Hex(0xff, 0xff, 0x55),
        selection_bg: Color::Hex(0x00, 0x55, 0x77),
        selection_fg: Color::Hex(0xff, 0xff, 0xff),
        error_fg:     Color::Hex(0xff, 0x55, 0x55),
        dir_fg:       Color::Hex(0xff, 0xff, 0xff),
        symlink_fg:   Color::Hex(0x55, 0xff, 0xff),
        status_bg:    Color::Hex(0x00, 0x00, 0x77),
        status_fg:    Color::Hex(0xff, 0xff, 0xff),
        modal_bg:     Color::Hex(0x00, 0x00, 0x77),
        modal_fg:     Color::Hex(0xff, 0xff, 0xff),
    };

    /// Terminal-default-bg theme with subtle accents.
    pub const DARK: Self = Self {
        bg:           Color::Default,
        fg:           Color::Default,
        accent:       Color::Hex(0x7d, 0xc4, 0xff),
        selection_bg: Color::Hex(0x33, 0x33, 0x33),
        selection_fg: Color::Default,
        error_fg:     Color::Hex(0xff, 0x6b, 0x6b),
        dir_fg:       Color::Hex(0x9e, 0xc1, 0xff),
        symlink_fg:   Color::Hex(0x82, 0xe6, 0xe6),
        status_bg:    Color::Hex(0x22, 0x22, 0x22),
        status_fg:    Color::Default,
        modal_bg:     Color::Hex(0x1c, 0x1c, 0x1c),
        modal_fg:     Color::Default,
    };

    /// Look up a named built-in. `None` → caller falls back to default + warning.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "classic" => Some(Self::CLASSIC),
            "dark"    => Some(Self::DARK),
            _ => None,
        }
    }
}

impl Default for Theme {
    fn default() -> Self { Self::CLASSIC }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color() {
        assert_eq!(Color::parse("#ff0000"), Some(Color::Hex(0xff, 0x00, 0x00)));
        assert_eq!(Color::parse("#FFFFFF"), Some(Color::Hex(0xff, 0xff, 0xff)));
        assert_eq!(Color::parse("#abc"),    None);   // 3-digit not supported
        assert_eq!(Color::parse("ff0000"),  None);   // missing #
    }

    #[test]
    fn parse_ansi_color() {
        assert_eq!(Color::parse("ansi:0"),  Some(Color::Ansi(0)));
        assert_eq!(Color::parse("ansi:15"), Some(Color::Ansi(15)));
        assert_eq!(Color::parse("ansi:16"), None);   // out of range
    }

    #[test]
    fn parse_default_color() {
        assert_eq!(Color::parse("default"), Some(Color::Default));
        assert_eq!(Color::parse("DEFAULT"), Some(Color::Default));
    }

    #[test]
    fn named_themes_resolve() {
        assert!(Theme::named("classic").is_some());
        assert!(Theme::named("dark").is_some());
        assert!(Theme::named("solarized").is_none());
    }

    #[test]
    fn default_is_classic() {
        assert_eq!(Theme::default(), Theme::CLASSIC);
    }

    #[test]
    fn classic_is_blue() {
        // The MC heritage check: classic background must be the deep blue.
        assert_eq!(Theme::CLASSIC.bg, Color::Hex(0x00, 0x00, 0xaa));
    }
}
```

Modify `crates/mx-core/src/lib.rs`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod theme;
pub mod state;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core theme::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/theme.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add Theme, Color parser, classic and dark presets"
```

---

## Task 9: `mx-core::keymap` — Keymap, Lookup, default bindings

**Files:**
- Create: `crates/mx-core/src/keymap.rs`
- Modify: `crates/mx-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/keymap.rs`:

```rust
//! Prefix-aware keymap. A binding is a non-empty `Vec<KeyChord>` (a sequence
//! of one or more keystrokes) mapped to a `CommandId`. Lookup tells the
//! caller whether a sequence is a complete `Match`, a `Prefix` of a longer
//! binding (caller should keep waiting), or `NoMatch`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::command::CommandId;
use crate::input::{KeyChord, KeyCode, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Match(CommandId),
    Prefix,
    NoMatch,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keymap {
    /// Internal storage. Both fields stay in sync; only `bindings` is
    /// (de)serialized.
    bindings: Vec<(Vec<KeyChord>, CommandId)>,
    #[serde(skip)]
    by_seq:   HashMap<Vec<KeyChord>, CommandId>,
}

impl Keymap {
    #[must_use]
    pub fn empty() -> Self { Self::default() }

    /// Build from a list of (sequence, command) pairs. Later entries override
    /// earlier ones with the same key sequence.
    #[must_use]
    pub fn from_bindings(bindings: Vec<(Vec<KeyChord>, CommandId)>) -> Self {
        let mut k = Self { bindings: Vec::new(), by_seq: HashMap::new() };
        for (seq, cmd) in bindings {
            k.set(seq, cmd);
        }
        k
    }

    pub fn set(&mut self, seq: Vec<KeyChord>, cmd: CommandId) {
        // Remove any prior binding for the same sequence.
        self.bindings.retain(|(s, _)| s != &seq);
        self.bindings.push((seq.clone(), cmd));
        self.by_seq.insert(seq, cmd);
    }

    pub fn unbind(&mut self, seq: &[KeyChord]) {
        self.bindings.retain(|(s, _)| s.as_slice() != seq);
        self.by_seq.remove(seq);
    }

    #[must_use]
    pub fn lookup(&self, seq: &[KeyChord]) -> Lookup {
        if let Some(cmd) = self.by_seq.get(seq) {
            return Lookup::Match(*cmd);
        }
        if self.is_strict_prefix(seq) {
            Lookup::Prefix
        } else {
            Lookup::NoMatch
        }
    }

    /// Returns `true` iff some binding is strictly longer than `seq` and
    /// starts with `seq`. The chord engine in `update()` uses this to decide
    /// whether to wait when an exact match is *also* a prefix of a longer
    /// chord (e.g. `Esc` is a complete `Cancel` binding *and* a prefix of
    /// `Esc 1` etc.).
    #[must_use]
    pub fn is_strict_prefix(&self, seq: &[KeyChord]) -> bool {
        self.bindings
            .iter()
            .any(|(s, _)| s.len() > seq.len() && s.starts_with(seq))
    }

    /// MC-faithful default keymap (spec §8). Phase 1 ships this verbatim;
    /// many of these resolve to `CommandId`s that `update()` does not yet
    /// handle, but they parse and dispatch as Phase 2 picks them up.
    #[must_use]
    pub fn defaults() -> Self {
        use CommandId::*;
        use KeyCode::*;

        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::ctrl();
        let shift = KeyModifiers::shift();
        let kc = |code| KeyChord::new(code, none);
        let ck = |code| KeyChord::new(code, ctrl);
        let sk = |code| KeyChord::new(code, shift);

        let bindings: Vec<(Vec<KeyChord>, CommandId)> = vec![
            // Navigation
            (vec![kc(Up)],          CursorUp),
            (vec![kc(Down)],        CursorDown),
            (vec![kc(PageUp)],      CursorPageUp),
            (vec![kc(PageDown)],    CursorPageDown),
            (vec![kc(Home)],        CursorHome),
            (vec![kc(End)],         CursorEnd),
            (vec![kc(Enter)],       EnterDir),
            (vec![kc(Backspace)],   ParentDir),
            (vec![kc(Tab)],         FocusOther),
            (vec![ck(Char('u'))],   SwapPanels),
            // Selection
            (vec![kc(Insert)],      ToggleSelect),
            (vec![kc(Char('*'))],   InvertSelection),
            (vec![ck(Char('a'))],   SelectAll),
            // F-keys
            (vec![kc(F(1))],        Help),
            (vec![kc(F(3))],        View),
            (vec![kc(F(5))],        Copy),
            (vec![kc(F(6))],        Move),
            (vec![kc(F(7))],        Mkdir),
            (vec![kc(F(8))],        Delete),
            (vec![kc(Delete)],      Delete),
            (vec![sk(F(6))],        Rename),
            (vec![ck(Char('t'))],   Rename),
            (vec![kc(F(10))],       QuitConfirm),
            (vec![ck(Char('q'))],   QuitConfirm),
            // Refresh / view toggles
            (vec![ck(Char('r'))],   RescanFocused),
            (vec![ck(Char('h'))],   ToggleHidden),
            (vec![ck(Char('s'))],   CycleSort),
            // Esc alone — Cancel
            (vec![kc(Esc)],         Cancel),
            // Esc-prefix chords (MC alt-meta style)
            (vec![kc(Esc), kc(Char('1'))], Help),
            (vec![kc(Esc), kc(Char('3'))], View),
            (vec![kc(Esc), kc(Char('5'))], Copy),
            (vec![kc(Esc), kc(Char('6'))], Move),
            (vec![kc(Esc), kc(Char('7'))], Mkdir),
            (vec![kc(Esc), kc(Char('8'))], Delete),
            (vec![kc(Esc), kc(Char('0'))], QuitConfirm),
        ];
        Keymap::from_bindings(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandId;
    use crate::input::{KeyChord, KeyCode};

    fn kc(c: KeyCode) -> KeyChord { KeyChord::bare(c) }

    #[test]
    fn match_returns_command_id() {
        let k = Keymap::defaults();
        assert_eq!(k.lookup(&[kc(KeyCode::F(5))]), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn esc_alone_is_cancel_and_also_a_prefix() {
        let k = Keymap::defaults();
        // Esc alone is a complete binding (Cancel)…
        assert_eq!(k.lookup(&[kc(KeyCode::Esc)]), Lookup::Match(CommandId::Cancel));
        // …but it's also a prefix of Esc-N chords. The chord-engine in
        // `update()` is what disambiguates via timeout; the keymap just
        // reports what it knows.
    }

    #[test]
    fn unknown_sequence_is_no_match() {
        let k = Keymap::defaults();
        assert_eq!(k.lookup(&[kc(KeyCode::F(11))]), Lookup::NoMatch);
    }

    #[test]
    fn unbind_removes_default() {
        let mut k = Keymap::defaults();
        k.unbind(&[kc(KeyCode::F(5))]);
        assert_eq!(k.lookup(&[kc(KeyCode::F(5))]), Lookup::NoMatch);
    }

    #[test]
    fn esc_prefix_chord_resolves() {
        let k = Keymap::defaults();
        let seq = vec![kc(KeyCode::Esc), kc(KeyCode::Char('5'))];
        assert_eq!(k.lookup(&seq), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn longer_binding_makes_shorter_prefix_visible() {
        // Build a keymap where 'a' is a prefix of 'a b'.
        use crate::input::KeyModifiers;
        let a   = KeyChord::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let b   = KeyChord::new(KeyCode::Char('b'), KeyModifiers::NONE);
        let k = Keymap::from_bindings(vec![
            (vec![a, b], CommandId::Help),
        ]);
        assert_eq!(k.lookup(&[a]), Lookup::Prefix);
        assert_eq!(k.lookup(&[a, b]), Lookup::Match(CommandId::Help));
    }
}
```

Modify `crates/mx-core/src/lib.rs`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod theme;
pub mod keymap;
pub mod state;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core keymap::tests`
Expected: PASS (6 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/keymap.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add prefix-aware Keymap with MC-faithful defaults"
```

---

## Task 10: `mx-core::config` — Config, sub-structs, defaults

**Files:**
- Create: `crates/mx-core/src/config.rs`
- Modify: `crates/mx-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/config.rs`:

```rust
//! Resolved application configuration. Defaults are produced here so any
//! crate can synthesize a working `Config` in tests without going through
//! `mx-config`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::keymap::Keymap;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruncatePath { Start, Middle, End }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiConfig {
    pub theme:         String,
    pub show_hidden:   bool,
    pub panel_ratio:   u8,
    pub truncate_path: TruncatePath,
    pub date_format:   String,
    pub columns:       UiColumns,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme:         "classic".into(),
            show_hidden:   false,
            panel_ratio:   50,
            truncate_path: TruncatePath::Middle,
            date_format:   "%Y-%m-%d %H:%M".into(),
            columns:       UiColumns::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiColumns { pub size: bool, pub modified: bool }
impl Default for UiColumns {
    fn default() -> Self { Self { size: true, modified: true } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfig {
    pub chord_timeout_ms: u64,
    pub double_click_ms:  u64,
}
impl Default for InputConfig {
    fn default() -> Self { Self { chord_timeout_ms: 500, double_click_ms: 250 } }
}
impl InputConfig {
    #[must_use]
    pub fn chord_timeout(&self) -> Duration { Duration::from_millis(self.chord_timeout_ms) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpsConfig {
    pub copy_buffer_kib:        u64,
    pub preserve_mtime:         bool,
    pub preserve_mode:          bool,
    pub follow_symlinks:        bool,
    pub max_concurrent_workers: u32,
}
impl Default for OpsConfig {
    fn default() -> Self {
        Self {
            copy_buffer_kib:        1024,
            preserve_mtime:         true,
            preserve_mode:          true,
            follow_symlinks:        false,
            max_concurrent_workers: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel { Off, Error, Warn, Info, Debug, Trace }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub file:  String,        // "auto", "off", or absolute path
}
impl Default for LoggingConfig {
    fn default() -> Self { Self { level: LogLevel::Warn, file: "auto".into() } }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub ui:      UiConfig,
    pub input:   InputConfig,
    pub ops:     OpsConfig,
    pub keymap:  Keymap,
    pub theme:   Theme,
    pub logging: LoggingConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ui:      UiConfig::default(),
            input:   InputConfig::default(),
            ops:     OpsConfig::default(),
            keymap:  Keymap::defaults(),
            theme:   Theme::CLASSIC,
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_classic_theme_and_full_keymap() {
        let c = Config::default();
        assert_eq!(c.theme, Theme::CLASSIC);
        // F5 must be Copy in defaults.
        use crate::input::{KeyChord, KeyCode};
        use crate::keymap::Lookup;
        use crate::command::CommandId;
        let f5 = vec![KeyChord::bare(KeyCode::F(5))];
        assert_eq!(c.keymap.lookup(&f5), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn input_chord_timeout_is_500ms_default() {
        assert_eq!(
            InputConfig::default().chord_timeout(),
            std::time::Duration::from_millis(500),
        );
    }
}
```

Modify `crates/mx-core/src/lib.rs`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod theme;
pub mod keymap;
pub mod config;
pub mod state;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core config::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/config.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add Config + UiConfig/InputConfig/OpsConfig/LoggingConfig defaults"
```

---

## Task 11: `mx-core::state` — State, PanelState, Modal, StatusLine

**Files:**
- Modify: `crates/mx-core/src/state.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/mx-core/src/state.rs` with:

```rust
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
pub enum PanelSide { Left, Right }

impl PanelSide {
    #[must_use]
    pub fn other(self) -> Self {
        match self { Self::Left => Self::Right, Self::Right => Self::Left }
    }

    #[must_use]
    pub fn index(self) -> usize {
        match self { Self::Left => 0, Self::Right => 1 }
    }
}

/// Stand-in for full directory entries. Phase 2 fleshes this out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind { File, Dir, Symlink, Unreadable }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode { ByName, BySize, ByModified }
impl Default for SortMode { fn default() -> Self { Self::ByName } }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelState {
    pub cwd:         Utf8PathBuf,
    pub entries:     Arc<[DirEntry]>,
    pub cursor:      usize,
    pub scroll:      usize,
    pub selection:   BTreeSet<usize>,
    pub sort:        SortMode,
    pub show_hidden: bool,
    pub loading:     bool,
}

impl PanelState {
    #[must_use]
    pub fn empty(cwd: impl Into<Utf8PathBuf>) -> Self {
        Self {
            cwd:         cwd.into(),
            entries:     Arc::new([]),
            cursor:      0,
            scroll:      0,
            selection:   BTreeSet::new(),
            sort:        SortMode::default(),
            show_hidden: false,
            loading:     false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
}

impl Default for StatusLine {
    fn default() -> Self { Self { text: String::new() } }
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmDialog {
    pub title:   String,
    pub body:    String,
    pub default_yes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDialog {
    pub title:  String,
    pub prompt: String,
    pub value:  String,
    pub cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressDialog {
    pub title:        String,
    pub current_path: Utf8PathBuf,
    pub bytes_done:   u64,
    pub bytes_total:  u64,
    pub worker_id:    WorkerId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDialog {
    pub title: String,
    pub body:  String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerState {
    pub id:   WorkerId,
    pub kind: WorkerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind { Copy, Move, Delete, DirScan }

#[derive(Debug, Clone)]
pub struct State {
    pub panels:        [PanelState; 2],
    pub focus:         PanelSide,
    pub modal:         Option<Modal>,
    pub status:        StatusLine,
    pub workers:       HashMap<WorkerId, WorkerState>,
    pub config:        Arc<Config>,
    pub pending_chord: Vec<KeyChord>,
    pub pending_since: Option<Instant>,
    pub should_quit:   bool,
}

impl State {
    #[must_use]
    pub fn new(config: Arc<Config>, left_cwd: Utf8PathBuf, right_cwd: Utf8PathBuf) -> Self {
        Self {
            panels:        [PanelState::empty(left_cwd), PanelState::empty(right_cwd)],
            focus:         PanelSide::Left,
            modal:         None,
            status:        StatusLine::default(),
            workers:       HashMap::new(),
            config,
            pending_chord: Vec::new(),
            pending_since: None,
            should_quit:   false,
        }
    }

    #[must_use]
    pub fn focused(&self) -> &PanelState { &self.panels[self.focus.index()] }

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
}
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core state::tests`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-core/src/state.rs
git commit -m "feat(core): add State, PanelState, Modal variants, StatusLine"
```

---

## Task 12: `mx-core::update` — Phase 1 update()

Phase 1 `update()` handles: chord engine (push/lookup/timeout flush), `Cancel` (close modal), `QuitConfirm` (open modal or quit if no workers), `Quit`, `Help` (open/close help modal), `FocusOther` (toggle panel focus), `Tick` (chord timeout flush + nothing else). All other `CommandId`s are matched and explicitly no-op'd; this is intentional so Phase 2 just edits the body.

**Files:**
- Create: `crates/mx-core/src/update.rs`
- Modify: `crates/mx-core/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-core/src/update.rs`:

```rust
//! Pure state-transition function. The only mutation in the entire app for
//! everything except the side effects in `Command`s.

use std::time::Instant;

use crate::command::{Command, CommandId};
use crate::event::Event;
use crate::input::{InputEvent, KeyChord, KeyCode};
use crate::keymap::Lookup;
use crate::state::{Modal, PanelSide, State};

/// Pure transition. Takes ownership of `state`, returns the new state and any
/// `Command`s the executor should run.
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
        Event::Worker(_, _) => { /* Phase 2 */ }
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
            (_, CommandId::Cancel) => {
                state.modal = None;
                return Vec::new();
            }
            (Modal::Help, CommandId::Help) => {
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
        // All other CommandId variants are valid keymap targets but Phase 2
        // is what wires them up. We exhaustively match so adding new variants
        // is a compile error here.
        CommandId::CursorUp
        | CommandId::CursorDown
        | CommandId::CursorPageUp
        | CommandId::CursorPageDown
        | CommandId::CursorHome
        | CommandId::CursorEnd
        | CommandId::EnterDir
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
            // Phase 2: implement.
        }
    }
    Vec::new()
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
        Event::Input(InputEvent::Key(KeyChord::new(KeyCode::Char(c), KeyModifiers::ctrl())))
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
        assert!(s.modal.is_some(), "esc must wait for chord_timeout before closing");
        // Now flush via a Tick after chord_timeout.
        let s2 = State { pending_since: Some(Instant::now() - Duration::from_secs(1)), ..s };
        let (s3, _) = update(s2, Event::Tick { dt: Duration::from_millis(100) });
        assert!(s3.modal.is_none(), "after timeout, esc resolves and closes the modal");
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
}
```

Modify `crates/mx-core/src/lib.rs` to expose `update`:

```rust
//! Midnight X core: data types, pure update() and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod theme;
pub mod keymap;
pub mod config;
pub mod state;
pub mod update;

pub use update::update;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-core update::tests`
Expected: PASS (7 tests).

- [ ] **Step 3: Run all mx-core tests**

Run: `cargo test -p mx-core`
Expected: PASS for every module.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs crates/mx-core/src/lib.rs
git commit -m "feat(core): add update() with chord engine, modal-aware dispatch, focus toggle"
```

---

## Task 13: `mx-config::keymap_str` — parse "ctrl-shift-pgup"

**Files:**
- Create: `crates/mx-config/src/keymap_str.rs`
- Modify: `crates/mx-config/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-config/src/keymap_str.rs`:

```rust
//! Parser for the `"ctrl-shift-pgup"` / `"esc 1"` key-string grammar used by
//! `[keymap]` TOML tables.

use mx_core::input::{KeyChord, KeyCode, KeyModifiers};

/// Parse a single key chord like `"ctrl-shift-f5"` or `"a"`.
pub fn parse_chord(s: &str) -> Result<KeyChord, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty chord".into()); }

    let mut mods = KeyModifiers::NONE;
    let mut last: Option<&str> = None;
    for part in s.split('-') {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => mods.ctrl  = true,
            "shift"            => mods.shift = true,
            "alt" | "meta"     => mods.alt   = true,
            _ => {
                if last.is_some() {
                    return Err(format!("invalid modifier: {part}"));
                }
                last = Some(part);
            }
        }
    }
    let key = last.ok_or_else(|| format!("no key after modifiers: {s}"))?;
    let code = parse_keycode(key)?;
    Ok(KeyChord::new(code, mods))
}

/// Parse a sequence like `"esc 1"` or `"ctrl-r r"` (space-separated chords).
pub fn parse_sequence(s: &str) -> Result<Vec<KeyChord>, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty sequence".into()); }
    s.split_whitespace().map(parse_chord).collect()
}

fn parse_keycode(s: &str) -> Result<KeyCode, String> {
    let lower = s.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "esc" | "escape"             => KeyCode::Esc,
        "tab"                        => KeyCode::Tab,
        "backtab"                    => KeyCode::BackTab,
        "enter" | "return"           => KeyCode::Enter,
        "backspace" | "bs"           => KeyCode::Backspace,
        "del" | "delete"             => KeyCode::Delete,
        "ins" | "insert"             => KeyCode::Insert,
        "home"                       => KeyCode::Home,
        "end"                        => KeyCode::End,
        "pgup" | "pageup"            => KeyCode::PageUp,
        "pgdn" | "pgdown" | "pagedown" => KeyCode::PageDown,
        "up"                         => KeyCode::Up,
        "down"                       => KeyCode::Down,
        "left"                       => KeyCode::Left,
        "right"                      => KeyCode::Right,
        "space"                      => KeyCode::Char(' '),
        "null"                       => KeyCode::Null,
        other if other.starts_with('f') && other[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u8 = other[1..].parse().map_err(|_| format!("bad fn key: {s}"))?;
            if !(1..=24).contains(&n) {
                return Err(format!("F-key out of range: {s}"));
            }
            KeyCode::F(n)
        }
        other if other.chars().count() == 1 => {
            KeyCode::Char(other.chars().next().expect("len 1"))
        }
        _ => return Err(format!("unknown key: {s}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_char() {
        assert_eq!(parse_chord("a").unwrap(),
            KeyChord::new(KeyCode::Char('a'), KeyModifiers::NONE));
    }

    #[test]
    fn ctrl_q_lowercase() {
        let c = parse_chord("ctrl-q").unwrap();
        assert!(c.mods.ctrl && !c.mods.shift && !c.mods.alt);
        assert_eq!(c.code, KeyCode::Char('q'));
    }

    #[test]
    fn ctrl_shift_pgup() {
        let c = parse_chord("ctrl-shift-pgup").unwrap();
        assert!(c.mods.ctrl && c.mods.shift);
        assert_eq!(c.code, KeyCode::PageUp);
    }

    #[test]
    fn function_key() {
        assert_eq!(parse_chord("f5").unwrap().code, KeyCode::F(5));
        assert_eq!(parse_chord("F12").unwrap().code, KeyCode::F(12));
    }

    #[test]
    fn esc_alias() {
        assert_eq!(parse_chord("esc").unwrap().code,    KeyCode::Esc);
        assert_eq!(parse_chord("escape").unwrap().code, KeyCode::Esc);
    }

    #[test]
    fn rejects_bad_key() {
        assert!(parse_chord("ctrl-asdf").is_err());
        assert!(parse_chord("").is_err());
        assert!(parse_chord("ctrl-shift-").is_err());
    }

    #[test]
    fn parses_sequence_with_spaces() {
        let seq = parse_sequence("esc 1").unwrap();
        assert_eq!(seq.len(), 2);
        assert_eq!(seq[0].code, KeyCode::Esc);
        assert_eq!(seq[1].code, KeyCode::Char('1'));
    }

    #[test]
    fn parses_multi_chord_sequence() {
        let seq = parse_sequence("ctrl-r r").unwrap();
        assert_eq!(seq.len(), 2);
        assert!(seq[0].mods.ctrl);
        assert_eq!(seq[1], KeyChord::new(KeyCode::Char('r'), KeyModifiers::NONE));
    }
}
```

Modify `crates/mx-config/src/lib.rs`:

```rust
//! Midnight X config: parses TOML into types defined in `mx-core` and reports
//! warnings for partially-bad input without ever panicking.

#![forbid(unsafe_code)]

pub mod keymap_str;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-config keymap_str::tests`
Expected: PASS (8 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-config/src/keymap_str.rs crates/mx-config/src/lib.rs
git commit -m "feat(config): parse 'ctrl-shift-pgup' / 'esc 1' key strings"
```

---

## Task 14: `mx-config::default.toml` + `DEFAULT_CONFIG_TOML`

**Files:**
- Create: `crates/mx-config/src/default.toml`
- Modify: `crates/mx-config/src/lib.rs`

- [ ] **Step 1: Write `crates/mx-config/src/default.toml`**

```toml
# Midnight X — default configuration. Every section is optional in user
# config files; missing fields fall back to these values.

[ui]
theme              = "classic"
show_hidden        = false
panel_ratio        = 50
truncate_path      = "middle"
date_format        = "%Y-%m-%d %H:%M"

[ui.columns]
size               = true
modified           = true

[input]
chord_timeout_ms   = 500
double_click_ms    = 250

[ops]
copy_buffer_kib    = 1024
preserve_mtime     = true
preserve_mode      = true
follow_symlinks    = false
max_concurrent_workers = 2

[logging]
level              = "warn"
file               = "auto"

# [keymap] is omitted: the built-in defaults from mx-core::Keymap::defaults()
# are the canonical source. Users may add overrides like:
#   "ctrl-q"  = "quit_confirm"
#   "f5"      = "<unbind>"
```

- [ ] **Step 2: Add a const + a round-trip test**

Modify `crates/mx-config/src/lib.rs`:

```rust
//! Midnight X config: parses TOML into types defined in `mx-core` and reports
//! warnings for partially-bad input without ever panicking.

#![forbid(unsafe_code)]

pub mod keymap_str;

/// Canonical default-config TOML. Same string is emitted by
/// `mx --print-default-config`.
pub const DEFAULT_CONFIG_TOML: &str = include_str!("default.toml");

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::config::{Config, UiConfig};

    /// The embedded default TOML must deserialize into the same `UiConfig` /
    /// etc. that `mx-core` produces with `Default::default()`.
    #[test]
    fn embedded_default_round_trip() {
        // We can deserialize each [section] separately because Config is a
        // composition of Deserialize sub-structs.
        #[derive(serde::Deserialize)]
        struct Doc {
            ui:      UiConfig,
            input:   mx_core::config::InputConfig,
            ops:     mx_core::config::OpsConfig,
            logging: mx_core::config::LoggingConfig,
        }
        let parsed: Doc = toml::from_str(DEFAULT_CONFIG_TOML)
            .expect("default.toml must parse");
        let c = Config::default();
        assert_eq!(parsed.ui, c.ui);
        assert_eq!(parsed.input, c.input);
        assert_eq!(parsed.ops, c.ops);
        assert_eq!(parsed.logging, c.logging);
    }
}
```

- [ ] **Step 3: Run tests; expect pass**

Run: `cargo test -p mx-config tests::embedded_default_round_trip`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-config/src/default.toml crates/mx-config/src/lib.rs
git commit -m "feat(config): embed canonical default.toml with round-trip test"
```

---

## Task 15: `mx-config::load` — TOML loader with warnings

**Files:**
- Modify: `crates/mx-config/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Replace `crates/mx-config/src/lib.rs` with:

```rust
//! Midnight X config: parses TOML into types defined in `mx-core` and reports
//! warnings for partially-bad input without ever panicking.

#![forbid(unsafe_code)]

pub mod keymap_str;

use std::fs;
use std::path::Path;

use mx_core::command::CommandId;
use mx_core::config::{Config, InputConfig, LoggingConfig, OpsConfig, UiConfig};
use mx_core::errors::ConfigWarning;
use mx_core::keymap::Keymap;
use mx_core::theme::Theme;

use serde::Deserialize;
use thiserror::Error;

pub const DEFAULT_CONFIG_TOML: &str = include_str!("default.toml");

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config file unreadable: {0}")]
    Io(String),
    #[error("config file does not parse as TOML: {0}")]
    Parse(String),
}

/// Load a config file. `path = None` → defaults silently. A non-existent
/// default-location file is also treated as "use defaults silently."
///
/// **Never panics.** Per-field problems become `ConfigWarning`s so the user
/// can still launch the app with a partially-broken config.
pub fn load(path: Option<&Path>) -> Result<(Config, Vec<ConfigWarning>), ConfigError> {
    let raw = match path {
        Some(p) => fs::read_to_string(p).map_err(|e| ConfigError::Io(e.to_string()))?,
        None => return Ok((Config::default(), Vec::new())),
    };
    parse_str(&raw)
}

/// Parse already-loaded TOML text. Useful for tests.
pub fn parse_str(s: &str) -> Result<(Config, Vec<ConfigWarning>), ConfigError> {
    let doc: TopLevel = toml::from_str(s).map_err(|e| ConfigError::Parse(e.to_string()))?;
    Ok(resolve(doc))
}

#[derive(Debug, Default, Deserialize)]
struct TopLevel {
    ui:      Option<UiConfig>,
    input:   Option<InputConfig>,
    ops:     Option<OpsConfig>,
    logging: Option<LoggingConfig>,
    keymap:  Option<toml::Table>,
}

fn resolve(doc: TopLevel) -> (Config, Vec<ConfigWarning>) {
    let mut warnings = Vec::new();

    let mut ui = doc.ui.unwrap_or_default();
    if !(1..=99).contains(&ui.panel_ratio) {
        warnings.push(ConfigWarning::new(
            "ui.panel_ratio",
            format!("clamped {} → 50 (must be 1..=99)", ui.panel_ratio),
        ));
        ui.panel_ratio = 50;
    }

    let theme = match Theme::named(&ui.theme) {
        Some(t) => t,
        None => {
            warnings.push(ConfigWarning::new(
                "ui.theme",
                format!("unknown theme \"{}\" — using \"classic\"", ui.theme),
            ));
            Theme::CLASSIC
        }
    };

    let input = doc.input.unwrap_or_default();
    let ops = doc.ops.unwrap_or_default();
    let logging = doc.logging.unwrap_or_default();

    let mut keymap = Keymap::defaults();
    if let Some(table) = doc.keymap {
        for (key, value) in table {
            let seq = match keymap_str::parse_sequence(&key) {
                Ok(s) => s,
                Err(e) => {
                    warnings.push(ConfigWarning::new(
                        format!("keymap.\"{key}\""),
                        format!("ignored: {e}"),
                    ));
                    continue;
                }
            };
            let cmd_str = value.as_str().map(str::to_owned);
            match cmd_str.as_deref() {
                Some("<unbind>") => { keymap.unbind(&seq); }
                Some(s) => {
                    match toml::from_str::<CmdHolder>(&format!("v = \"{s}\"")) {
                        Ok(holder) => keymap.set(seq, holder.v),
                        Err(_) => warnings.push(ConfigWarning::new(
                            format!("keymap.\"{key}\""),
                            format!("unknown command \"{s}\""),
                        )),
                    }
                }
                None => warnings.push(ConfigWarning::new(
                    format!("keymap.\"{key}\""),
                    "value must be a string".into(),
                )),
            }
        }
    }

    let config = Config { ui, input, ops, keymap, theme, logging };
    (config, warnings)
}

#[derive(Deserialize)]
struct CmdHolder { v: CommandId }

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::input::{KeyChord, KeyCode};
    use mx_core::keymap::Lookup;
    use mx_core::command::CommandId;

    #[test]
    fn embedded_default_yields_default_config() {
        let (c, warnings) = parse_str(DEFAULT_CONFIG_TOML).unwrap();
        assert!(warnings.is_empty(), "default config must yield no warnings: {warnings:?}");
        assert_eq!(c.ui, UiConfig::default());
        assert_eq!(c.theme, Theme::CLASSIC);
    }

    #[test]
    fn unknown_theme_warns_and_falls_back() {
        let toml = r#"[ui]
theme = "midnight"
"#;
        let (c, warnings) = parse_str(toml).unwrap();
        assert_eq!(c.theme, Theme::CLASSIC);
        assert!(warnings.iter().any(|w| w.key == "ui.theme"));
    }

    #[test]
    fn out_of_range_ratio_clamps_and_warns() {
        let toml = "[ui]\npanel_ratio = 200\n";
        let (c, warnings) = parse_str(toml).unwrap();
        assert_eq!(c.ui.panel_ratio, 50);
        assert!(warnings.iter().any(|w| w.key == "ui.panel_ratio"));
    }

    #[test]
    fn keymap_unbind_removes_default() {
        let toml = r#"[keymap]
"f5" = "<unbind>"
"#;
        let (c, _) = parse_str(toml).unwrap();
        let f5 = vec![KeyChord::bare(KeyCode::F(5))];
        assert_eq!(c.keymap.lookup(&f5), Lookup::NoMatch);
    }

    #[test]
    fn keymap_override_replaces_default() {
        let toml = r#"[keymap]
"ctrl-c" = "copy"
"#;
        let (c, warnings) = parse_str(toml).unwrap();
        assert!(warnings.is_empty());
        let chord = vec![KeyChord::new(
            KeyCode::Char('c'),
            mx_core::input::KeyModifiers::ctrl(),
        )];
        assert_eq!(c.keymap.lookup(&chord), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn unknown_command_warns_does_not_fail() {
        let toml = r#"[keymap]
"ctrl-c" = "explode_universe"
"#;
        let (_, warnings) = parse_str(toml).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("unknown command")));
    }

    #[test]
    fn malformed_toml_is_a_parse_error() {
        let bad = "this isn't = toml\n[oops";
        assert!(matches!(parse_str(bad), Err(ConfigError::Parse(_))));
    }
}
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-config tests`
Expected: PASS (7 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-config/src/lib.rs
git commit -m "feat(config): TOML loader with non-fatal warnings"
```

---

## Task 16: `mx-tui::input_xlate` — crossterm → mx_core::InputEvent

**Files:**
- Create: `crates/mx-tui/src/input_xlate.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-tui/src/input_xlate.rs`:

```rust
//! Translate `crossterm::event::Event` → `mx_core::input::InputEvent`. This
//! is the only place where crossterm's input vocabulary touches the rest of
//! the workspace.

use crossterm::event as ct;
use mx_core::input::{InputEvent, KeyChord, KeyCode, KeyModifiers};

/// Returns `None` for events `mx-core` doesn't model in Phase 1
/// (resize is handled by the input thread separately; mouse is reserved).
#[must_use]
pub fn translate(ev: ct::Event) -> Option<InputEvent> {
    match ev {
        ct::Event::Key(k) if k.kind == ct::KeyEventKind::Press => {
            Some(InputEvent::Key(translate_key(k)))
        }
        ct::Event::Paste(s) => Some(InputEvent::Paste(s)),
        ct::Event::Mouse(_) => Some(InputEvent::MouseStub),
        _ => None,
    }
}

fn translate_key(k: ct::KeyEvent) -> KeyChord {
    let code = match k.code {
        ct::KeyCode::Char(c)  => KeyCode::Char(c),
        ct::KeyCode::Enter    => KeyCode::Enter,
        ct::KeyCode::Esc      => KeyCode::Esc,
        ct::KeyCode::Tab      => KeyCode::Tab,
        ct::KeyCode::BackTab  => KeyCode::BackTab,
        ct::KeyCode::Backspace=> KeyCode::Backspace,
        ct::KeyCode::Delete   => KeyCode::Delete,
        ct::KeyCode::Insert   => KeyCode::Insert,
        ct::KeyCode::Home     => KeyCode::Home,
        ct::KeyCode::End      => KeyCode::End,
        ct::KeyCode::PageUp   => KeyCode::PageUp,
        ct::KeyCode::PageDown => KeyCode::PageDown,
        ct::KeyCode::Up       => KeyCode::Up,
        ct::KeyCode::Down     => KeyCode::Down,
        ct::KeyCode::Left     => KeyCode::Left,
        ct::KeyCode::Right    => KeyCode::Right,
        ct::KeyCode::F(n)     => KeyCode::F(n),
        ct::KeyCode::Null     => KeyCode::Null,
        // Other crossterm KeyCodes (CapsLock etc.) become Null in Phase 1.
        _                     => KeyCode::Null,
    };
    let mods = KeyModifiers {
        ctrl:  k.modifiers.contains(ct::KeyModifiers::CONTROL),
        shift: k.modifiers.contains(ct::KeyModifiers::SHIFT),
        alt:   k.modifiers.contains(ct::KeyModifiers::ALT),
    };
    KeyChord { code, mods }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event as ct;

    #[test]
    fn ctrl_q_translates() {
        let ev = ct::Event::Key(ct::KeyEvent::new(
            ct::KeyCode::Char('q'),
            ct::KeyModifiers::CONTROL,
        ));
        match translate(ev).unwrap() {
            InputEvent::Key(c) => {
                assert_eq!(c.code, KeyCode::Char('q'));
                assert!(c.mods.ctrl);
            }
            _ => panic!("expected Key"),
        }
    }

    #[test]
    fn release_events_are_dropped() {
        let mut k = ct::KeyEvent::new(ct::KeyCode::Esc, ct::KeyModifiers::NONE);
        k.kind = ct::KeyEventKind::Release;
        assert!(translate(ct::Event::Key(k)).is_none());
    }

    #[test]
    fn f5_translates() {
        let ev = ct::Event::Key(ct::KeyEvent::new(
            ct::KeyCode::F(5),
            ct::KeyModifiers::NONE,
        ));
        match translate(ev).unwrap() {
            InputEvent::Key(c) => assert_eq!(c.code, KeyCode::F(5)),
            _ => panic!(),
        }
    }
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-tui input_xlate::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/input_xlate.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): translate crossterm events to mx-core InputEvent"
```

---

## Task 17: `mx-tui::theme_styles` — Theme → ratatui::Style

**Files:**
- Create: `crates/mx-tui/src/theme_styles.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-tui/src/theme_styles.rs`:

```rust
//! Map the terminal-agnostic `mx_core::theme::Theme` onto concrete
//! `ratatui::style::Style` instances. This is the single chokepoint for the
//! "no hardcoded `Color::Blue` outside the theme module" rule.

use mx_core::theme::{Color, Theme};
use ratatui::style::{Color as RColor, Modifier, Style};

#[must_use]
pub fn rcolor(c: Color) -> RColor {
    match c {
        Color::Hex(r, g, b) => RColor::Rgb(r, g, b),
        Color::Ansi(n)      => RColor::Indexed(n),
        Color::Default      => RColor::Reset,
    }
}

#[must_use]
pub fn frame_style(t: &Theme) -> Style {
    Style::default().bg(rcolor(t.bg)).fg(rcolor(t.fg))
}

#[must_use]
pub fn panel_title_style(t: &Theme, focused: bool) -> Style {
    let base = Style::default().bg(rcolor(t.bg));
    if focused {
        base.fg(rcolor(t.accent)).add_modifier(Modifier::BOLD)
    } else {
        base.fg(rcolor(t.fg))
    }
}

#[must_use]
pub fn status_style(t: &Theme) -> Style {
    Style::default().bg(rcolor(t.status_bg)).fg(rcolor(t.status_fg))
}

#[must_use]
pub fn modal_style(t: &Theme) -> Style {
    Style::default().bg(rcolor(t.modal_bg)).fg(rcolor(t.modal_fg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_frame_style_is_blue_on_grey() {
        let s = frame_style(&Theme::CLASSIC);
        assert_eq!(s.bg, Some(RColor::Rgb(0x00, 0x00, 0xaa)));
        assert_eq!(s.fg, Some(RColor::Rgb(0xcc, 0xcc, 0xcc)));
    }

    #[test]
    fn dark_uses_reset_for_default_color() {
        let s = frame_style(&Theme::DARK);
        assert_eq!(s.bg, Some(RColor::Reset));
        assert_eq!(s.fg, Some(RColor::Reset));
    }

    #[test]
    fn focused_title_uses_accent_and_bold() {
        let s = panel_title_style(&Theme::CLASSIC, true);
        assert_eq!(s.fg, Some(RColor::Rgb(0xff, 0xff, 0x55)));
        assert!(s.add_modifier.contains(Modifier::BOLD));
    }
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod theme_styles;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-tui theme_styles::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/theme_styles.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): map Theme onto ratatui Style"
```

---

## Task 18: `mx-tui::layout` — FrameLayout

**Files:**
- Create: `crates/mx-tui/src/layout.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-tui/src/layout.rs`:

```rust
//! Compute frame regions. Exposed as a struct with named `Rect`s so any
//! future post-render effects pass can scope by region without recomputing
//! geometry.

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Minimum width (in columns) at which both panels are shown side by side.
/// Below this we collapse to one panel.
pub const NARROW_THRESHOLD: u16 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameLayout {
    pub left_panel:  Rect,
    pub right_panel: Option<Rect>,   // None when collapsed
    pub status:      Rect,
    pub hint:        Rect,
    pub modal:       Option<Rect>,
}

#[must_use]
pub fn compute(area: Rect, panel_ratio: u8, modal_open: bool) -> FrameLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    let body   = chunks[0];
    let status = chunks[1];
    let hint   = chunks[2];

    let (left_panel, right_panel) = if area.width < NARROW_THRESHOLD {
        // Narrow mode in Phase 1 always shows the left panel. Phase 2 will
        // swap which panel is visible based on focus.
        (body, None)
    } else {
        let pr = panel_ratio.clamp(1, 99) as u16;
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(u16::from(pr)),
                          Constraint::Percentage(100 - u16::from(pr))])
            .split(body);
        (cols[0], Some(cols[1]))
    };

    let modal = if modal_open {
        Some(centered_rect(area, 60, 50))
    } else {
        None
    };

    FrameLayout { left_panel, right_panel, status, hint, modal }
}

fn centered_rect(area: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = area.width.saturating_mul(pct_w) / 100;
    let h = area.height.saturating_mul(pct_h) / 100;
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(w: u16, h: u16) -> Rect { Rect { x: 0, y: 0, width: w, height: h } }

    #[test]
    fn wide_terminal_shows_both_panels() {
        let fl = compute(area(120, 30), 50, false);
        assert!(fl.right_panel.is_some());
        assert_eq!(fl.left_panel.width + fl.right_panel.unwrap().width, 120);
        assert_eq!(fl.status.height, 1);
        assert_eq!(fl.hint.height, 1);
    }

    #[test]
    fn narrow_terminal_collapses_to_one_panel() {
        let fl = compute(area(60, 24), 50, false);
        assert!(fl.right_panel.is_none());
        assert_eq!(fl.left_panel.width, 60);
    }

    #[test]
    fn modal_centered_when_open() {
        let fl = compute(area(120, 30), 50, true);
        let m = fl.modal.expect("modal_open=true must produce a modal rect");
        assert!(m.width  > 0 && m.width  < 120);
        assert!(m.height > 0 && m.height < 30);
    }

    #[test]
    fn extreme_panel_ratio_clamped() {
        let fl_low  = compute(area(120, 30), 0,   false);
        let fl_high = compute(area(120, 30), 200, false);
        // Both should render two panels (clamping to 1..=99).
        assert!(fl_low.right_panel.is_some());
        assert!(fl_high.right_panel.is_some());
    }
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod layout;
pub mod theme_styles;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-tui layout::tests`
Expected: PASS (4 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/layout.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): add FrameLayout with narrow-collapse and modal centering"
```

---

## Task 19: `mx-tui::view` — empty-panel view()

**Files:**
- Create: `crates/mx-tui/src/view.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-tui/src/view.rs`:

```rust
//! Pure render function. Writes the entire frame into `frame.buffer_mut()`
//! based only on `&State` and a `FrameLayout`. No I/O, no thread access.

use mx_core::state::{Modal, PanelSide, State};

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::layout::FrameLayout;
use crate::theme_styles::{frame_style, modal_style, panel_title_style, status_style};

pub fn view(state: &State, layout: &FrameLayout, frame: &mut Frame<'_>) {
    let theme = &state.config.theme;

    // Background fill: a Block with the theme background painted across the
    // full area covers any cells we don't otherwise write to.
    let full = frame.area();
    frame.render_widget(
        Block::default().style(frame_style(theme)),
        full,
    );

    render_panel(frame, layout.left_panel,  state, PanelSide::Left);
    if let Some(rp) = layout.right_panel {
        render_panel(frame, rp, state, PanelSide::Right);
    }
    render_status(frame, layout.status, state);
    render_hint(frame, layout.hint,   state);

    if let (Some(rect), Some(modal)) = (layout.modal, state.modal.as_ref()) {
        render_modal(frame, rect, modal, theme);
    }
}

fn render_panel(frame: &mut Frame<'_>, area: Rect, state: &State, side: PanelSide) {
    let theme = &state.config.theme;
    let panel = &state.panels[side.index()];
    let focused = state.focus == side;

    let title = format!(" {} ", panel.cwd);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(panel_title_style(theme, focused))
        .title(title)
        .title_style(panel_title_style(theme, focused))
        .style(frame_style(theme));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Phase 1: panels are empty. Render a centered "(empty)" line so the
    // user can see the panel exists at all. Phase 2 replaces this with the
    // entry list.
    let placeholder = Paragraph::new("(empty)").style(frame_style(theme));
    let one_line = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1) / 2,
        width: inner.width,
        height: 1,
    };
    frame.render_widget(placeholder, one_line);
}

fn render_status(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let text = if state.status.text.is_empty() {
        format!(" {} files, {} dirs           ", 0, 0)
    } else {
        format!(" {}", state.status.text)
    };
    let p = Paragraph::new(text).style(status_style(theme));
    frame.render_widget(p, area);
}

fn render_hint(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let hint = " F1 Help  F3 View  F5 Copy  F6 Move  F7 Mkdir  F8 Del  F10 Quit ";
    let p = Paragraph::new(hint).style(status_style(theme));
    frame.render_widget(p, area);
    let _ = state; // reserved for context-sensitive hints in Phase 2
}

fn render_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    modal: &Modal,
    theme: &mx_core::theme::Theme,
) {
    frame.render_widget(Clear, area);
    let title = match modal {
        Modal::Help        => " Help ",
        Modal::QuitConfirm => " Quit? ",
        Modal::Confirm(_)  => " Confirm ",
        Modal::Input(_)    => " Input ",
        Modal::Progress(_) => " Working… ",
        Modal::Error(_)    => " Error ",
    };
    let body = match modal {
        Modal::Help => "Phase 1 help: F10/Ctrl-Q quits, Tab toggles focus, Esc cancels.\n\n(Press F1 again or Esc to dismiss.)".to_string(),
        Modal::QuitConfirm => "Workers are still running. Quit anyway?\n\n[ Yes ]   [ No ]".to_string(),
        Modal::Error(d) => d.body.clone(),
        Modal::Confirm(d) => d.body.clone(),
        Modal::Input(d) => format!("{}\n> {}", d.prompt, d.value),
        Modal::Progress(d) => format!("{}\n{} / {} bytes", d.current_path, d.bytes_done, d.bytes_total),
    };
    let p = Paragraph::new(body)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(modal_style(theme))
                .title_style(modal_style(theme)),
        )
        .style(modal_style(theme));
    frame.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    // Snapshot tests against `ratatui::backend::TestBackend` live in
    // crates/mx-tui/tests/render_snapshots.rs (Task 21) so they share fixtures
    // with the renderer integration tests.
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod layout;
pub mod theme_styles;
pub mod view;
```

- [ ] **Step 2: Verify compilation**

Run: `cargo build -p mx-tui`
Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/view.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): add view() rendering empty-panel placeholders + status/hint/modal"
```

---

## Task 20: `mx-tui::renderer` — Renderer with explicit phase order

**Files:**
- Create: `crates/mx-tui/src/renderer.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mx-tui/src/renderer.rs`:

```rust
//! Single rendering chokepoint. Owns the `terminal.draw(|frame| …)` closure,
//! exposes a deterministic `draw_to_buffer` for snapshot tests, and runs the
//! four explicit phases (layout → widgets → effects → cursor) so the
//! tachyonfx animation layer can drop in later.

use std::io;
use std::time::Duration;

use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};

use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal;

use mx_core::state::State;

use crate::layout::{compute, FrameLayout};
use crate::view::view;

/// Boxed effect — empty in Phase 1; tachyonfx will populate this trait.
pub trait Effect: Send {
    fn process(&mut self, dt: Duration, buf: &mut Buffer, layout: &FrameLayout);
}

pub struct Renderer<B: Backend> {
    pub terminal: Terminal<B>,
    pub effects: Vec<Box<dyn Effect>>,
}

impl<B: Backend> Renderer<B> {
    pub fn new(terminal: Terminal<B>) -> Self {
        Self { terminal, effects: Vec::new() }
    }

    /// Run the four-phase render. `dt` is the elapsed wall-clock duration
    /// since the last `draw` (used by `Effect`s; ignored in Phase 1).
    pub fn draw(&mut self, state: &State, dt: Duration) -> io::Result<()> {
        let size = self.terminal.size()?;
        let area = Rect { x: 0, y: 0, width: size.width, height: size.height };
        let layout = compute(
            area,
            state.config.ui.panel_ratio,
            state.modal.is_some(),
        );

        // Split-borrow self into its two disjoint fields so the closure can
        // mutate `effects` while `terminal.draw` holds `terminal`.
        let Self { terminal, effects } = self;
        terminal.draw(|frame| {
            // Phase 1: widgets
            view(state, &layout, frame);
            // Phase 2: effects (no-op in Phase 1; tachyonfx-compat seam)
            for ef in effects.iter_mut() {
                ef.process(dt, frame.buffer_mut(), &layout);
            }
            // Phase 3: cursor — Phase 1 has no text-input modal active.
        })?;
        Ok(())
    }
}

/// Snapshot-test helper: render to a fixed-size off-screen buffer.
pub fn draw_to_buffer(state: &State, area: Rect) -> Buffer {
    let backend = TestBackend::new(area.width, area.height);
    let mut term = Terminal::new(backend).expect("TestBackend Terminal cannot fail");
    let mut renderer = Renderer::new(term);
    renderer.draw(state, Duration::ZERO).expect("TestBackend draw cannot fail");
    renderer.terminal.backend().buffer().clone()
}

/// Acquire the terminal: enable raw mode + alternate screen + hide cursor.
/// Returns a `Renderer` wrapped around a real `CrosstermBackend`.
pub fn acquire() -> io::Result<Renderer<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen, crossterm::cursor::Hide)?;
    let backend = CrosstermBackend::new(out);
    let term = Terminal::new(backend)?;
    Ok(Renderer::new(term))
}

/// Inverse of `acquire`: restore terminal to its prior mode. Idempotent.
pub fn release() -> io::Result<()> {
    let mut out = io::stdout();
    let _ = execute!(out, LeaveAlternateScreen, crossterm::cursor::Show);
    let _ = disable_raw_mode();
    Ok(())
}

/// Format a `Buffer` as a stable, diffable string for snapshot tests.
/// One line per row of cells, plus a separator and a per-cell-style table
/// listing only cells whose style differs from default.
#[must_use]
pub fn buffer_snapshot(buf: &Buffer) -> String {
    let mut out = String::new();
    let area = buf.area();
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out.push_str("---\n");
    let default = ratatui::style::Style::default();
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            let s = cell.style();
            if s != default {
                out.push_str(&format!("({x},{y}) {:?}\n", s));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mx_core::config::Config;
    use mx_core::state::State;
    use std::sync::Arc;

    #[test]
    fn draw_to_buffer_is_deterministic() {
        let s = State::new(Arc::new(Config::default()), "/".into(), "/".into());
        let area = Rect { x: 0, y: 0, width: 100, height: 30 };
        let a = draw_to_buffer(&s, area);
        let b = draw_to_buffer(&s, area);
        assert_eq!(a, b, "same state must produce identical buffers");
    }

    #[test]
    fn empty_state_produces_two_panel_titles() {
        let s = State::new(Arc::new(Config::default()), "/foo".into(), "/bar".into());
        let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
        let snap = buffer_snapshot(&buf);
        assert!(snap.contains("/foo"));
        assert!(snap.contains("/bar"));
    }

    #[test]
    fn narrow_state_only_shows_focused_panel_title() {
        let s = State::new(Arc::new(Config::default()), "/foo".into(), "/bar".into());
        let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 60, height: 24 });
        let snap = buffer_snapshot(&buf);
        assert!(snap.contains("/foo"));
        assert!(!snap.contains("/bar"));
    }
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod layout;
pub mod renderer;
pub mod theme_styles;
pub mod view;
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx-tui renderer::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/renderer.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): add Renderer with four-phase draw, TestBackend helper, buffer_snapshot"
```

---

## Task 21: `mx-tui` snapshot fixtures

Wire `insta` snapshot tests for the empty-panel render in both themes and both widths.

**Files:**
- Create: `crates/mx-tui/tests/render_snapshots.rs`

- [ ] **Step 1: Write the snapshot test file**

```rust
//! Snapshot tests for view() rendering. Reviewed via `cargo insta review`.

use std::sync::Arc;

use mx_core::config::Config;
use mx_core::state::{PanelSide, State};
use mx_core::theme::Theme;
use mx_tui::renderer::{buffer_snapshot, draw_to_buffer};
use ratatui::layout::Rect;

fn fresh(theme: Theme) -> State {
    let mut c = Config::default();
    c.theme = theme;
    let mut s = State::new(Arc::new(c), "/Users/boogie".into(), "/tmp".into());
    s.focus = PanelSide::Left;
    s
}

#[test]
fn empty_panels_classic_theme_wide() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("empty_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panels_dark_theme_wide() {
    let s = fresh(Theme::DARK);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("empty_dark_wide", buffer_snapshot(&buf));
}

#[test]
fn empty_panel_collapsed_narrow() {
    let s = fresh(Theme::CLASSIC);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 60, height: 24 });
    insta::assert_snapshot!("empty_classic_narrow", buffer_snapshot(&buf));
}

#[test]
fn help_modal_open() {
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(mx_core::state::Modal::Help);
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("help_modal_classic_wide", buffer_snapshot(&buf));
}
```

- [ ] **Step 2: Run tests to generate `.snap.new` files**

Run: `cargo test -p mx-tui --test render_snapshots`
Expected: tests fail because `.snap` files don't exist yet; `insta` writes `.snap.new` next to the test file.

- [ ] **Step 3: Accept the new snapshots**

Either install `cargo-insta`:
```bash
cargo install cargo-insta
cargo insta accept --workspace
```

…or manually rename each `.snap.new` → `.snap`:
```bash
for f in crates/mx-tui/tests/snapshots/*.snap.new; do mv "$f" "${f%.new}"; done
```

- [ ] **Step 4: Re-run; expect pass**

Run: `cargo test -p mx-tui --test render_snapshots`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mx-tui/tests/render_snapshots.rs crates/mx-tui/tests/snapshots
git commit -m "test(tui): snapshot empty panels (classic/dark, wide/narrow) and help modal"
```

---

## Task 22: `mx-tui::input_thread` — spawn input reader

**Files:**
- Create: `crates/mx-tui/src/input_thread.rs`
- Modify: `crates/mx-tui/src/lib.rs`

- [ ] **Step 1: Write `input_thread.rs`**

```rust
//! Spawn a background thread that polls `crossterm::event::read()` and forwards
//! translated `mx_core::Event`s onto the supplied `mpsc::Sender`. The thread
//! exits when `should_run.load() == false`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossterm::event as ct;

use mx_core::event::Event;

use crate::input_xlate::translate;

pub struct InputThread {
    pub handle:     JoinHandle<()>,
    pub should_run: Arc<AtomicBool>,
}

#[must_use]
pub fn spawn(tx: Sender<Event>) -> InputThread {
    let should_run = Arc::new(AtomicBool::new(true));
    let sr = Arc::clone(&should_run);
    let handle = thread::Builder::new()
        .name("mx-input".into())
        .spawn(move || {
            while sr.load(Ordering::Relaxed) {
                // Poll so the thread can notice should_run flipping.
                match ct::poll(Duration::from_millis(100)) {
                    Ok(true) => {
                        match ct::read() {
                            Ok(ct::Event::Resize(w, h)) => {
                                let _ = tx.send(Event::Resize { cols: w, rows: h });
                            }
                            Ok(ev) => {
                                if let Some(input) = translate(ev) {
                                    let _ = tx.send(Event::Input(input));
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok(false) => { /* timed out, loop and check should_run */ }
                    Err(_) => break,
                }
            }
        })
        .expect("input thread cannot fail to spawn");
    InputThread { handle, should_run }
}

impl InputThread {
    pub fn stop(self) {
        self.should_run.store(false, Ordering::Relaxed);
        let _ = self.handle.join();
    }
}
```

Modify `crates/mx-tui/src/lib.rs`:

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the crossterm → mx_core::InputEvent
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod input_thread;
pub mod layout;
pub mod renderer;
pub mod theme_styles;
pub mod view;
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p mx-tui`
Expected: clean compile.

(Input thread is hard to unit-test without a real TTY; it's exercised through the binary smoke tests.)

- [ ] **Step 3: Commit**

```bash
git add crates/mx-tui/src/input_thread.rs crates/mx-tui/src/lib.rs
git commit -m "feat(tui): spawn input thread that translates crossterm events into Event"
```

---

## Task 23: `mx::cli` — pico-args CLI parsing

**Files:**
- Create: `crates/mx/src/cli.rs`

- [ ] **Step 1: Write `crates/mx/src/cli.rs`**

```rust
//! CLI parser. v1 surface is tiny on purpose; if it grows past 4-5 flags
//! we'll switch to `clap`.

use std::path::PathBuf;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    Run(RunOpts),
    PrintDefaultConfig,
    PrintVersion,
    PrintHelp,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunOpts {
    pub config: Option<PathBuf>,
}

pub fn parse<I, S>(args: I) -> Result<CliAction>
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let mut a = pico_args::Arguments::from_vec(
        args.into_iter().map(Into::into).collect(),
    );
    if a.contains(["-h", "--help"])    { return Ok(CliAction::PrintHelp); }
    if a.contains(["-V", "--version"]) { return Ok(CliAction::PrintVersion); }
    if a.contains("--print-default-config") { return Ok(CliAction::PrintDefaultConfig); }

    let config: Option<PathBuf> = a.opt_value_from_str("--config")
        .map_err(|e| anyhow!("--config: {e}"))?;

    let leftover = a.finish();
    if !leftover.is_empty() {
        return Err(anyhow!("unexpected arguments: {leftover:?}"));
    }
    Ok(CliAction::Run(RunOpts { config }))
}

pub const HELP_TEXT: &str = "\
mx — Midnight X, a modern Rust two-pane terminal file manager.

USAGE:
    mx [--config PATH]
    mx --print-default-config
    mx -V | --version
    mx -h | --help

OPTIONS:
    --config PATH              Use the given config file instead of the default location.
    --print-default-config     Write the canonical default config to stdout and exit.
    -V, --version              Print version and exit.
    -h, --help                 Print this help and exit.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_runs_with_defaults() {
        let a = parse::<_, &str>(std::iter::empty()).unwrap();
        assert_eq!(a, CliAction::Run(RunOpts::default()));
    }

    #[test]
    fn version_flag() {
        assert_eq!(parse(["-V"]).unwrap(), CliAction::PrintVersion);
        assert_eq!(parse(["--version"]).unwrap(), CliAction::PrintVersion);
    }

    #[test]
    fn print_default_config_flag() {
        assert_eq!(parse(["--print-default-config"]).unwrap(), CliAction::PrintDefaultConfig);
    }

    #[test]
    fn config_flag_takes_path() {
        let a = parse(["--config", "/etc/mx/x.toml"]).unwrap();
        match a {
            CliAction::Run(o) => assert_eq!(o.config.unwrap(), PathBuf::from("/etc/mx/x.toml")),
            _ => panic!(),
        }
    }

    #[test]
    fn unknown_arg_errors() {
        let r = parse(["--gibberish"]);
        assert!(r.is_err());
    }
}
```

- [ ] **Step 2: Run tests; expect pass**

Run: `cargo test -p mx cli::tests`
Expected: PASS (5 tests).

(`cli` is private to the binary at this point; the tests live inside the module.)

- [ ] **Step 3: Commit**

```bash
git add crates/mx/src/cli.rs
git commit -m "feat(cli): parse --config / --print-default-config / --version / --help"
```

---

## Task 24: `mx::panic_hook` — restore terminal on panic

**Files:**
- Create: `crates/mx/src/panic_hook.rs`

- [ ] **Step 1: Write `crates/mx/src/panic_hook.rs`**

```rust
//! Install a panic hook that restores the terminal and logs the panic before
//! the process dies, so the user gets a usable shell instead of a wedged
//! raw-mode TTY.

use std::panic;

use mx_tui::renderer;

/// Install the hook. Idempotent.
pub fn install() {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = renderer::release();
        eprintln!("\nmx crashed: {info}");
        if let Ok(log) = std::env::var("MX_LOG_PATH") {
            eprintln!("see log: {log}");
        }
        tracing::error!(target: "mx", "panic: {info}");
        prev(info);
    }));
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p mx`
Expected: clean compile (panic_hook is just declared; it gets wired in Task 26).

- [ ] **Step 3: Commit**

```bash
git add crates/mx/src/panic_hook.rs
git commit -m "feat(mx): panic hook restores terminal and logs the crash"
```

---

## Task 25: `mx::app` — main loop wiring

**Files:**
- Create: `crates/mx/src/app.rs`

- [ ] **Step 1: Write `crates/mx/src/app.rs`**

```rust
//! Main event loop. Owns the State, the Renderer, the input thread, and the
//! mpsc channel. Translates raw `Command`s into terminal-mode changes and
//! exit signaling. Phase 1 handles only the `Quit` command.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use camino::Utf8PathBuf;

use mx_core::command::Command;
use mx_core::config::Config;
use mx_core::event::Event;
use mx_core::state::State;
use mx_core::update;
use mx_tui::input_thread;
use mx_tui::renderer;

const TICK: Duration = Duration::from_millis(100);

pub fn run(config: Config) -> Result<()> {
    let cfg = Arc::new(config);

    let cwd: Utf8PathBuf = std::env::current_dir()
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| Utf8PathBuf::from("/"));

    let mut state = State::new(Arc::clone(&cfg), cwd.clone(), cwd);

    let (tx, rx) = mpsc::channel::<Event>();
    let input = input_thread::spawn(tx.clone());

    let mut renderer = renderer::acquire()?;
    // Render initial frame so the user sees something immediately.
    let _ = renderer.draw(&state, Duration::ZERO);

    let mut last_frame = Instant::now();
    'main: loop {
        let event = match rx.recv_timeout(TICK) {
            Ok(ev) => ev,
            Err(RecvTimeoutError::Timeout) => Event::Tick { dt: TICK },
            Err(RecvTimeoutError::Disconnected) => break 'main,
        };

        let (new_state, cmds) = update::update(state, event);
        state = new_state;

        for cmd in cmds {
            if matches!(cmd, Command::Quit) {
                break 'main;
            }
            // Phase 2 wires StartCopy / StartMove / StartDelete / etc.
            // through an Executor. None of those exist yet, so other
            // Command variants are unreachable in Phase 1 and ignored.
        }

        if state.should_quit { break 'main; }

        let now = Instant::now();
        let dt = now.saturating_duration_since(last_frame);
        last_frame = now;
        let _ = renderer.draw(&state, dt);
    }

    let _ = renderer::release();
    input.stop();
    Ok(())
}
```

- [ ] **Step 2: Verify compile**

Run: `cargo build -p mx`
Expected: clean compile.

- [ ] **Step 3: Commit**

```bash
git add crates/mx/src/app.rs
git commit -m "feat(mx): main event loop with tick-driven chord flush and quit"
```

---

## Task 26: `mx::main` — wire CLI, panic hook, logging, app::run

**Files:**
- Modify: `crates/mx/src/main.rs`

- [ ] **Step 1: Replace `crates/mx/src/main.rs`**

```rust
#![forbid(unsafe_code)]

mod app;
mod cli;
mod panic_hook;

use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};

use mx_config::DEFAULT_CONFIG_TOML;

use crate::cli::{CliAction, RunOpts};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(args) {
        Ok(CliAction::PrintHelp) => {
            print!("{}", cli::HELP_TEXT);
            ExitCode::SUCCESS
        }
        Ok(CliAction::PrintVersion) => {
            println!("mx {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(CliAction::PrintDefaultConfig) => {
            print!("{DEFAULT_CONFIG_TOML}");
            ExitCode::SUCCESS
        }
        Ok(CliAction::Run(opts)) => match run(opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("mx: {e:#}"); ExitCode::from(2) }
        },
        Err(e) => {
            eprintln!("mx: {e:#}\n\n{}", cli::HELP_TEXT);
            ExitCode::from(2)
        }
    }
}

fn run(opts: RunOpts) -> Result<()> {
    install_logging();
    panic_hook::install();

    let (config, warnings) = mx_config::load(opts.config.as_deref())
        .context("loading config")?;
    if !warnings.is_empty() {
        for w in &warnings {
            tracing::warn!(key = %w.key, msg = %w.message, "config warning");
        }
        // Phase 2: also push these into a startup Modal::Error.
    }

    if let Err(e) = app::run(config) {
        return Err(anyhow!(e.to_string()));
    }
    Ok(())
}

fn install_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}
```

- [ ] **Step 2: Build and test the binary directly**

Run: `cargo build -p mx`
Expected: clean compile.

Run: `cargo run -p mx -- --version`
Expected: prints `mx 0.0.1`.

Run: `cargo run -p mx -- --print-default-config`
Expected: prints the contents of `default.toml`.

Run: `cargo run -p mx -- --help`
Expected: prints `HELP_TEXT`.

- [ ] **Step 3: Commit**

```bash
git add crates/mx/src/main.rs
git commit -m "feat(mx): wire CLI, panic hook, logging, and app::run together"
```

---

## Task 27: Smoke tests for the binary

**Files:**
- Create: `crates/mx/tests/smoke.rs`

- [ ] **Step 1: Write smoke tests**

```rust
//! Black-box tests against the compiled binary.

use assert_cmd::Command;

#[test]
fn version_prints_expected_string() {
    Command::cargo_bin("mx").unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("mx "));
}

#[test]
fn help_mentions_keyword() {
    Command::cargo_bin("mx").unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("Midnight X"))
        .stdout(predicates::str::contains("--print-default-config"));
}

#[test]
fn print_default_config_round_trips() {
    let out = Command::cargo_bin("mx").unwrap()
        .arg("--print-default-config")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("[ui]"));
    assert!(text.contains("theme = \"classic\""));
    // Parse it; must not warn.
    let (_cfg, warnings) = mx_config::parse_str(&text).expect("default must parse");
    assert!(warnings.is_empty(), "default config emits warnings: {warnings:?}");
}

#[test]
fn unknown_flag_exits_non_zero() {
    Command::cargo_bin("mx").unwrap()
        .arg("--no-such-flag")
        .assert()
        .failure();
}
```

- [ ] **Step 2: Add `predicates` to dev-deps**

Modify `crates/mx/Cargo.toml`. Under `[dev-dependencies]`, add `predicates`:

```toml
predicates = "3"
```

(`mx-config` is already a regular `[dependencies]` entry; it's reachable from tests too, so no extra dev-dep needed.)

- [ ] **Step 3: Run tests; expect pass**

Run: `cargo test -p mx --test smoke`
Expected: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/mx/tests/smoke.rs crates/mx/Cargo.toml
git commit -m "test(mx): smoke-test --version, --help, --print-default-config"
```

---

## Task 28: End-to-end manual sanity + tag

- [ ] **Step 1: Run the binary interactively in a real terminal**

Run: `cargo run -p mx --release`
Expected: terminal switches to alternate screen; classic-blue background fills the screen; two panel frames are drawn; bottom hint line is visible.

Press `Ctrl-Q`. Expected: terminal restores; cursor reappears; you're back at the prompt.

Run again: `cargo run -p mx --release`
Press `F1`. Expected: a Help modal appears centered.
Press `Esc`. Expected: after ~500ms, the modal closes (the chord engine waits for the timeout because `Esc` is a prefix of `Esc N`).
Press `F10`. Expected: app exits cleanly.

If F-keys don't reach the terminal (e.g. macOS Terminal default), use `Ctrl-Q` to quit and document that as expected behavior in the README — the F-key issue is exactly why the chord engine and `esc 0`/`Ctrl-Q` aliases exist.

- [ ] **Step 2: Final workspace sanity**

Run: `cargo fmt --all -- --check`
Expected: no diff.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

Run: `cargo test --workspace`
Expected: all tests pass.

Run: `cargo doc --workspace --no-deps`
Expected: builds without warnings.

- [ ] **Step 3: Update CHANGELOG**

Modify `CHANGELOG.md`:

```markdown
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
- Tests: 40+ unit tests, 4 snapshot fixtures (classic + dark, wide + narrow,
  help modal), 4 binary smoke tests.

### Known limitations (Phase 1)
- Panels are empty placeholders — no directory scanning yet.
- F3 viewer, file ops, multi-select, sort modes, hidden toggle, and the help
  table are scaffolded but inactive (Phase 2).
- Worker model and `mx-fs` crate do not exist yet.
```

- [ ] **Step 4: Commit and tag**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for v0.1.0-phase1"
git tag -a v0.1.0-phase1 -m "Midnight X — Phase 1: foundation, runnable empty TUI"
```

`git log --oneline` should now show roughly 26 commits leading up to the tag.

---

## Phase 1 — Acceptance criteria

- [ ] `cargo fmt --all -- --check` is clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean.
- [ ] `cargo test --workspace` is fully green.
- [ ] `cargo doc --workspace --no-deps` is warning-free.
- [ ] `cargo run -p mx -- --version`, `--help`, `--print-default-config` all behave as specified.
- [ ] `cargo run -p mx` enters the alternate screen, draws two empty panels in the classic-blue theme, processes the chord engine (incl. `Esc`-prefix), and exits cleanly via `F10` or `Ctrl-Q`.
- [ ] Snapshot fixtures (`empty_classic_wide`, `empty_dark_wide`, `empty_classic_narrow`, `help_modal_classic_wide`) exist and pass.
- [ ] CI is green on Ubuntu and macOS.
- [ ] Git history has clean Conventional Commit messages with **no** AI-tool attribution.

When all criteria pass, Phase 1 is done. Phase 2 (filesystem scan, navigation, F3 viewer) is its own plan.
