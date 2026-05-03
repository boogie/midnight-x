# Midnight X — Phase 2: Navigation & View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `mx` from a static empty-panel TUI into a usable read-only file browser. Phase 2 ships: real directory scanning via a worker thread, all navigation/selection/sort commands wired through `update()`, file/dir rendering with size and mtime columns, an `mx-preview` crate with a plain-text F3 viewer, and a populated Help modal. No copy/move/delete (Phase 3); no editor (later).

**Architecture:** New `mx-fs` crate owns the `Executor` and the synchronous `dir_scan` helper; new `mx-preview` crate owns the `Previewer` trait and `PlainTextPreviewer`. `mx-core` extends `DirEntry` (size, mtime, symlink target), populates `WorkerMsg::DirScanned { side, entries }`, and grows `update()` to dispatch every navigation / selection / sort / rescan / view command. `mx-tui` adds entry-row rendering with cursor + selection highlighting and a scrollable viewer modal. `mx::app` wires the `Executor` and emits initial scans on startup. Tachyonfx seams are untouched.

**Tech Stack:** Same as Phase 1, plus one new workspace dep: `chrono` (strftime-compatible mtime formatting honoring the config's `date_format`). Test-only dep: `tempfile` (already in workspace) for `mx-fs` integration tests.

**Reference design doc:** `docs/superpowers/specs/2026-05-03-midnight-x-v1-design.md` (sections 5, 6, 7, 11, 13.2).

---

## File map

```
midnight-x/
├── Cargo.toml                                 # MODIFY: add chrono, add mx-fs/mx-preview members
├── crates/
│   ├── mx-core/
│   │   └── src/
│   │       ├── state.rs                       # MODIFY: enrich DirEntry; WorkerKind::DirScan(side); helpers
│   │       ├── event.rs                       # MODIFY: WorkerMsg::DirScanned { side, entries }
│   │       ├── update.rs                      # MODIFY: navigation/selection/sort/rescan/view dispatch
│   │       └── lib.rs                         # (unchanged)
│   ├── mx-fs/                                  # NEW
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                         # re-exports
│   │       ├── dir_scan.rs                    # pure scan() function
│   │       ├── format.rs                      # human-readable size formatter
│   │       └── executor.rs                    # Executor + WorkerHandle + spawn/cancel
│   ├── mx-preview/                             # NEW
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── plain_text.rs                  # Previewer trait + PlainTextPreviewer
│   ├── mx-tui/
│   │   └── src/
│   │       ├── format.rs                      # NEW: mtime/size column rendering
│   │       ├── view.rs                        # MODIFY: render entry rows, cursor, selection, viewer modal, help table
│   │       └── lib.rs                         # MODIFY: pub mod format
│   └── mx/
│       ├── Cargo.toml                         # MODIFY: depend on mx-fs + mx-preview
│       └── src/
│           └── app.rs                         # MODIFY: wire Executor; emit initial scans; handle RescanDir/OpenViewer
└── docs/superpowers/plans/...                  # this file
```

---

## Conventions

- TDD per Phase 1: test → run-fail → impl → run-pass → commit.
- All new files start with `#![forbid(unsafe_code)]`.
- Conventional Commits. **No** AI-tool attribution anywhere.
- Run `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings` after every task; the plan only flags it explicitly when something specific must be checked.
- Tests use the same `PATH=/opt/homebrew/opt/rustup/bin:$PATH` prefix shown in Phase 1 (or whatever cargo binary the developer has on `$PATH`).

---

## Task 1: Add `chrono` and scaffold `mx-fs` + `mx-preview` crates

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/mx-fs/Cargo.toml`
- Create: `crates/mx-fs/src/lib.rs`
- Create: `crates/mx-preview/Cargo.toml`
- Create: `crates/mx-preview/src/lib.rs`

- [ ] **Step 1: Modify root `Cargo.toml`** — add `chrono` to `[workspace.dependencies]`, add the new members and internal-crate aliases.

```toml
# Under [workspace] members, add the two new crates:
[workspace]
resolver = "2"
members  = [
    "crates/mx-core",
    "crates/mx-config",
    "crates/mx-tui",
    "crates/mx-fs",
    "crates/mx-preview",
    "crates/mx",
]

# Under [workspace.dependencies], add:
chrono             = { version = "0.4", default-features = false, features = ["clock", "std"] }

# And add the new internal aliases:
mx-fs              = { path = "crates/mx-fs" }
mx-preview         = { path = "crates/mx-preview" }
```

- [ ] **Step 2: Write `crates/mx-fs/Cargo.toml`**

```toml
[package]
name        = "mx-fs"
version     = "0.0.1"
description = "Midnight X — filesystem ops, dir scanning, worker executor"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[lints]
workspace = true

[dependencies]
mx-core   = { workspace = true }
camino    = { workspace = true }
thiserror = { workspace = true }
tracing   = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 3: Write `crates/mx-fs/src/lib.rs`**

```rust
//! Midnight X filesystem layer. Owns the `Executor` (worker threads + the
//! event channel back to the main loop) and the synchronous `dir_scan`
//! helper. The only crate that calls `std::fs` or spawns worker threads.

#![forbid(unsafe_code)]

pub mod dir_scan;
pub mod executor;
pub mod format;
```

- [ ] **Step 4: Write `crates/mx-preview/Cargo.toml`**

```toml
[package]
name        = "mx-preview"
version     = "0.0.1"
description = "Midnight X — content preview trait + plain-text impl (mxview seam)"
edition.workspace      = true
rust-version.workspace = true
authors.workspace      = true
license.workspace      = true
repository.workspace   = true
homepage.workspace     = true

[lints]
workspace = true

[dependencies]
mx-core   = { workspace = true }
camino    = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

- [ ] **Step 5: Write `crates/mx-preview/src/lib.rs`**

```rust
//! Midnight X content preview. Defines the `Previewer` trait — the seam the
//! future `mxview` binary and rich previewers (markdown, image, hex) will
//! implement. Phase 2 ships only `PlainTextPreviewer`.

#![forbid(unsafe_code)]

pub mod plain_text;

pub use plain_text::PlainTextPreviewer;
```

- [ ] **Step 6: Add empty placeholder modules so the crate compiles**

Create `crates/mx-fs/src/dir_scan.rs`:
```rust
//! Synchronous directory scan. Filled in in Task 3.
```

Create `crates/mx-fs/src/executor.rs`:
```rust
//! Worker executor. Filled in in Task 4.
```

Create `crates/mx-fs/src/format.rs`:
```rust
//! Human-readable formatters used by `dir_scan` callers. Filled in in Task 3.
```

Create `crates/mx-preview/src/plain_text.rs`:
```rust
//! `Previewer` trait + plain-text implementation. Filled in in Task 13.
```

- [ ] **Step 7: Verify the workspace still builds**

Run: `cargo build --workspace`
Expected: clean compile; the two new crates compile as no-op libraries.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/mx-fs crates/mx-preview
git commit -m "chore: scaffold mx-fs and mx-preview crates; add chrono workspace dep"
```

---

## Task 2: Flesh out `DirEntry` and `WorkerMsg::DirScanned`

**Files:**
- Modify: `crates/mx-core/src/state.rs`
- Modify: `crates/mx-core/src/event.rs`

`DirEntry` currently has only `name` and `kind`. We need size, mtime, symlink target, and helpers for sort + display. `WorkerMsg::DirScanned` currently carries `placeholder: ()`; we change it to carry the destination side and the scanned entries. `WorkerKind::DirScan` takes a `PanelSide` payload so the executor knows which panel to update.

- [ ] **Step 1: Write the failing tests**

In `crates/mx-core/src/state.rs`, **add** at the bottom of the existing `mod tests`:

```rust
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
```

- [ ] **Step 2: Replace `DirEntry` and friends in `crates/mx-core/src/state.rs`**

Replace this block:

```rust
/// Stand-in for full directory entries. Phase 2 fleshes this out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind { File, Dir, Symlink, Unreadable }
```

with:

```rust
use std::time::SystemTime;

/// One row in a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name:           String,
    pub kind:           EntryKind,
    /// `Some` for files (and for resolved symlinks-to-files when followed).
    /// `None` for directories and unreadable entries.
    pub size:           Option<u64>,
    /// Modification time, if metadata was readable.
    pub mtime:          Option<SystemTime>,
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
```

- [ ] **Step 3: Update `WorkerKind` to carry the destination side**

In the same file, replace:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind { Copy, Move, Delete, DirScan }
```

with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerKind {
    Copy,
    Move,
    Delete,
    /// Dir scan is keyed by destination panel so the executor and the
    /// `WorkerMsg::DirScanned` handler know whose `entries` to swap.
    DirScan(PanelSide),
}
```

- [ ] **Step 4: Update `WorkerMsg::DirScanned` to carry entries**

Modify `crates/mx-core/src/event.rs`. Replace the `DirScanned` variant:

```rust
    DirScanned {
        // Filled out in Phase 2 once DirEntry exists.
        // For Phase 1 we just need the variant to exist so update() can pattern-match.
        placeholder: (),
    },
```

with:

```rust
    DirScanned {
        side:    crate::state::PanelSide,
        entries: Vec<crate::state::DirEntry>,
    },
```

- [ ] **Step 5: Run mx-core tests**

Run: `cargo test -p mx-core`
Expected: PASS, including the two new constructor tests. Total mx-core tests should rise by 2 (currently 34 → 36).

- [ ] **Step 6: Verify nothing else broke**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. The `WorkerMsg::DirScanned` change might require a touch in `update.rs` — Phase 1's update() doesn't pattern-match on this variant explicitly, so it'll compile (the catchall `Event::Worker(_, _)` handles it). Confirm: re-grep for `DirScanned` in `crates/mx-core/src/update.rs`; no hits expected.

- [ ] **Step 7: Commit**

```bash
git add crates/mx-core/src/state.rs crates/mx-core/src/event.rs
git commit -m "feat(core): enrich DirEntry (size, mtime, symlink); WorkerKind::DirScan(side)"
```

---

## Task 3: `mx-fs::format::format_size` and `mx-fs::dir_scan::scan`

**Files:**
- Modify: `crates/mx-fs/src/format.rs`
- Modify: `crates/mx-fs/src/dir_scan.rs`
- Modify: `crates/mx-fs/src/lib.rs` (re-exports)

- [ ] **Step 1: Write the failing tests for `format_size`**

Replace `crates/mx-fs/src/format.rs`:

```rust
//! Formatters for human-readable display of file metadata.

/// Format a byte count using binary units (`K = 1024`). Output width is
/// always ≤ 7 characters: `"   42"`, `"1.2 K"`, `" 999 K"`, `"  1.5 M"`,
/// `"  9.4 G"`, `" 12.0 T"`. `None` renders as 4 spaces.
#[must_use]
pub fn format_size(bytes: Option<u64>) -> String {
    let Some(b) = bytes else { return "    ".to_string(); };
    if b < 1024 {
        return format!("{b:>7}");
    }
    const UNITS: [&str; 5] = ["K", "M", "G", "T", "P"];
    let mut value = b as f64 / 1024.0;
    let mut unit = UNITS[0];
    for (i, u) in UNITS.iter().enumerate().skip(1) {
        if value < 1024.0 { break; }
        value /= 1024.0;
        unit = u;
        let _ = i;
    }
    if value >= 100.0 {
        format!("{value:>4.0} {unit}")
    } else if value >= 10.0 {
        format!("{value:>4.1} {unit}")
    } else {
        format!("{value:>4.2} {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_files_show_raw_byte_count() {
        assert_eq!(format_size(Some(0)),    "      0");
        assert_eq!(format_size(Some(42)),   "     42");
        assert_eq!(format_size(Some(999)),  "    999");
        assert_eq!(format_size(Some(1023)), "   1023");
    }

    #[test]
    fn kibibytes_use_k_suffix() {
        assert_eq!(format_size(Some(1024)),         "1.00 K");
        assert_eq!(format_size(Some(2 * 1024)),     "2.00 K");
        assert_eq!(format_size(Some(15 * 1024)),    "15.0 K");
        assert_eq!(format_size(Some(150 * 1024)),   " 150 K");
    }

    #[test]
    fn megabytes_and_gigabytes() {
        assert_eq!(format_size(Some(1024 * 1024)),         "1.00 M");
        assert_eq!(format_size(Some(1024 * 1024 * 1024)),  "1.00 G");
    }

    #[test]
    fn none_is_blank() {
        assert_eq!(format_size(None), "    ");
    }
}
```

- [ ] **Step 2: Run format tests**

Run: `cargo test -p mx-fs format::tests`
Expected: PASS (4 tests).

- [ ] **Step 3: Write the failing tests for `dir_scan::scan`**

Replace `crates/mx-fs/src/dir_scan.rs`:

```rust
//! Synchronous directory scan. Returns a sorted `Vec<DirEntry>` honoring the
//! requested sort mode and the `show_hidden` flag. The first row, when the
//! caller asks for it, is `..` (parent navigation).
//!
//! This function is the *pure helper*. Threading and worker messaging live in
//! `executor.rs`.

use std::cmp::Ordering;
use std::fs;
use std::time::SystemTime;

use camino::{Utf8Path, Utf8PathBuf};

use mx_core::errors::FsError;
use mx_core::state::{DirEntry, EntryKind, SortMode};

/// Read the directory at `dir`, returning entries sorted per `sort` and
/// optionally including hidden (dot-prefixed) names.
///
/// The first entry is always `..` *unless* `dir` has no parent (rare on
/// Unix; only `/` qualifies).
///
/// # Errors
///
/// Returns `FsError::NotFound` / `PermissionDenied` / etc. for the directory
/// itself. Per-entry metadata failures degrade the entry to `EntryKind::Unreadable`
/// rather than failing the whole scan.
pub fn scan(dir: &Utf8Path, show_hidden: bool, sort: SortMode) -> Result<Vec<DirEntry>, FsError> {
    let read = fs::read_dir(dir).map_err(map_io)?;

    let mut entries: Vec<DirEntry> = Vec::new();
    for r in read {
        let entry = match r {
            Ok(e) => e,
            Err(_) => continue, // skip per-entry read errors silently
        };
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(os) => {
                // Non-UTF-8 names: surface as Unreadable with lossy text.
                let lossy = os.to_string_lossy().into_owned();
                entries.push(DirEntry {
                    name: lossy,
                    kind: EntryKind::Unreadable,
                    size: None,
                    mtime: None,
                    symlink_target: None,
                    symlink_broken: false,
                });
                continue;
            }
        };

        if !show_hidden && name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let lstat = fs::symlink_metadata(&path);
        let dirent = match lstat {
            Ok(meta) if meta.is_symlink() => {
                let target = fs::read_link(&path)
                    .ok()
                    .and_then(|p| p.into_os_string().into_string().ok());
                let resolved = fs::metadata(&path).ok();
                DirEntry {
                    name,
                    kind: EntryKind::Symlink,
                    size: resolved.as_ref().map(std::fs::Metadata::len),
                    mtime: resolved.as_ref().and_then(|m| m.modified().ok()),
                    symlink_target: target,
                    symlink_broken: resolved.is_none(),
                }
            }
            Ok(meta) if meta.is_dir() => DirEntry {
                name,
                kind: EntryKind::Dir,
                size: None,
                mtime: meta.modified().ok(),
                symlink_target: None,
                symlink_broken: false,
            },
            Ok(meta) => DirEntry {
                name,
                kind: EntryKind::File,
                size: Some(meta.len()),
                mtime: meta.modified().ok(),
                symlink_target: None,
                symlink_broken: false,
            },
            Err(_) => DirEntry {
                name,
                kind: EntryKind::Unreadable,
                size: None,
                mtime: None,
                symlink_target: None,
                symlink_broken: false,
            },
        };
        entries.push(dirent);
    }

    sort_entries(&mut entries, sort);

    // Prepend ".." unless we're at the filesystem root.
    if has_parent(dir) {
        let mut all = Vec::with_capacity(entries.len() + 1);
        all.push(DirEntry::parent());
        all.extend(entries);
        Ok(all)
    } else {
        Ok(entries)
    }
}

fn has_parent(dir: &Utf8Path) -> bool {
    let p: Utf8PathBuf = dir.to_path_buf();
    p.parent().map(|x| !x.as_str().is_empty()).unwrap_or(false)
}

fn sort_entries(entries: &mut [DirEntry], sort: SortMode) {
    let cmp = |a: &DirEntry, b: &DirEntry| -> Ordering {
        // Dirs always come before files within a sort mode.
        match (a.is_dir_like(), b.is_dir_like()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        match sort {
            SortMode::ByName     => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortMode::BySize     => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0))
                                     .then_with(|| a.name.cmp(&b.name)),
            SortMode::ByModified => mtime_key(&a.mtime).cmp(&mtime_key(&b.mtime))
                                     .then_with(|| a.name.cmp(&b.name)),
        }
    };
    entries.sort_by(cmp);
}

fn mtime_key(t: &Option<SystemTime>) -> u128 {
    t.and_then(|x| x.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn map_io(e: std::io::Error) -> FsError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound          => FsError::NotFound,
        K::PermissionDenied  => FsError::PermissionDenied,
        K::AlreadyExists     => FsError::AlreadyExists,
        K::NotADirectory     => FsError::NotADirectory,
        K::IsADirectory      => FsError::IsADirectory,
        _                    => FsError::Io(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn td_path(t: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(t.path().to_path_buf()).unwrap()
    }

    #[test]
    fn scan_lists_parent_then_dirs_then_files_by_name() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("zeta.txt"), b"z").unwrap();
        std::fs::write(t.path().join("alpha.txt"), b"abcde").unwrap();
        std::fs::create_dir(t.path().join("src")).unwrap();
        let entries = scan(&td_path(&t), false, SortMode::ByName).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "src", "alpha.txt", "zeta.txt"]);
        assert_eq!(entries[2].size, Some(5));
    }

    #[test]
    fn scan_omits_hidden_when_flag_off() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(".secret"), b"x").unwrap();
        std::fs::write(t.path().join("public"), b"y").unwrap();
        let entries = scan(&td_path(&t), false, SortMode::ByName).unwrap();
        assert!(entries.iter().any(|e| e.name == "public"));
        assert!(!entries.iter().any(|e| e.name == ".secret"));
    }

    #[test]
    fn scan_includes_hidden_when_flag_on() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join(".secret"), b"x").unwrap();
        std::fs::write(t.path().join("public"), b"y").unwrap();
        let entries = scan(&td_path(&t), true, SortMode::ByName).unwrap();
        assert!(entries.iter().any(|e| e.name == ".secret"));
    }

    #[test]
    fn scan_sorts_by_size_with_dirs_first() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("big"),    vec![0u8; 100]).unwrap();
        std::fs::write(t.path().join("small"),  vec![0u8; 10]).unwrap();
        std::fs::create_dir(t.path().join("d")).unwrap();
        let entries = scan(&td_path(&t), false, SortMode::BySize).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "d", "small", "big"]);
    }

    #[test]
    fn scan_returns_not_found_for_missing_dir() {
        let err = scan(&Utf8PathBuf::from("/no-such-path-mx-test"), false, SortMode::ByName)
            .unwrap_err();
        assert_eq!(err, FsError::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_renders_as_symlink_kind() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("real"), b"hi").unwrap();
        std::os::unix::fs::symlink(t.path().join("real"), t.path().join("link")).unwrap();
        let entries = scan(&td_path(&t), false, SortMode::ByName).unwrap();
        let link = entries.iter().find(|e| e.name == "link").unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);
        assert!(!link.symlink_broken);
        assert!(link.symlink_target.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_marked() {
        let t = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink("/no-such-target", t.path().join("dead")).unwrap();
        let entries = scan(&td_path(&t), false, SortMode::ByName).unwrap();
        let link = entries.iter().find(|e| e.name == "dead").unwrap();
        assert_eq!(link.kind, EntryKind::Symlink);
        assert!(link.symlink_broken);
    }
}
```

- [ ] **Step 4: Run dir_scan tests**

Run: `cargo test -p mx-fs dir_scan::tests`
Expected: PASS (7 tests on Unix; 5 on Windows since two are `cfg(unix)`).

- [ ] **Step 5: Update `mx-fs::lib.rs` re-exports**

Replace `crates/mx-fs/src/lib.rs`:

```rust
//! Midnight X filesystem layer. Owns the `Executor` (worker threads + the
//! event channel back to the main loop) and the synchronous `dir_scan`
//! helper. The only crate that calls `std::fs` or spawns worker threads.

#![forbid(unsafe_code)]

pub mod dir_scan;
pub mod executor;
pub mod format;

pub use dir_scan::scan;
pub use format::format_size;
```

- [ ] **Step 6: Verify clippy / fmt clean**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mx-fs
git commit -m "feat(fs): add format_size and pure dir_scan with tempdir-backed tests"
```

---

## Task 4: `mx-fs::executor::Executor` — worker spawn

**Files:**
- Modify: `crates/mx-fs/src/executor.rs`

The executor is the only place `std::thread::spawn` happens for workers. It owns a clone of the main `Sender<Event>`, an atomic `WorkerId` counter, and a `HashMap<WorkerId, JoinHandle<()>>`. Phase 2 implements only `start_dir_scan`; Phase 3 will add copy/move/delete here.

- [ ] **Step 1: Write the failing tests**

Replace `crates/mx-fs/src/executor.rs`:

```rust
//! Worker executor — the only `std::thread::spawn` site for app workers.
//! Phase 2 ships only directory scanning; Phase 3 adds copy/move/delete.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use camino::Utf8PathBuf;

use mx_core::event::{Event, WorkerId, WorkerMsg};
use mx_core::state::{PanelSide, SortMode};

use crate::dir_scan::scan;

pub struct Executor {
    tx:        Sender<Event>,
    next_id:   AtomicU64,
    handles:   HashMap<WorkerId, JoinHandle<()>>,
}

impl Executor {
    #[must_use]
    pub fn new(tx: Sender<Event>) -> Self {
        Self { tx, next_id: AtomicU64::new(1), handles: HashMap::new() }
    }

    fn alloc_id(&self) -> WorkerId {
        WorkerId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Spawn a directory scan in a worker thread. Sends exactly one
    /// `WorkerMsg`: `DirScanned` on success, or `Failed` on error.
    pub fn start_dir_scan(
        &mut self,
        side: PanelSide,
        dir: Utf8PathBuf,
        show_hidden: bool,
        sort: SortMode,
    ) -> WorkerId {
        let id = self.alloc_id();
        let tx = self.tx.clone();
        let handle = thread::Builder::new()
            .name(format!("mx-fs-scan-{}", id.0))
            .spawn(move || {
                let msg = match scan(&dir, show_hidden, sort) {
                    Ok(entries) => WorkerMsg::DirScanned { side, entries },
                    Err(e) => WorkerMsg::Failed {
                        errors: vec![(dir.clone(), e)],
                    },
                };
                let _ = tx.send(Event::Worker(id, msg));
            })
            .expect("worker thread cannot fail to spawn");
        self.handles.insert(id, handle);
        id
    }

    /// Drop completed workers from the handle map. Called periodically by
    /// the main loop so the map doesn't grow unbounded.
    pub fn reap(&mut self) {
        self.handles.retain(|_, h| !h.is_finished());
    }

    /// Wait for all in-flight workers to finish. Used during shutdown.
    pub fn join_all(&mut self) {
        let handles: Vec<_> = self.handles.drain().collect();
        for (_, h) in handles {
            let _ = h.join();
        }
    }

    #[must_use]
    pub fn active_count(&self) -> usize { self.handles.len() }
}

/// Shared facade some callers prefer (cheaper to clone than to thread an
/// `&mut Executor` through the world). Phase 2 doesn't use it; Phase 3 may.
#[derive(Clone)]
pub struct SharedExecutor(pub Arc<std::sync::Mutex<Executor>>);

impl SharedExecutor {
    #[must_use]
    pub fn new(executor: Executor) -> Self {
        Self(Arc::new(std::sync::Mutex::new(executor)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn drain<F: Fn(&Event) -> bool>(rx: &mpsc::Receiver<Event>, want: F) -> Event {
        // Wait up to 2 seconds for an event matching `want`.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let timeout = deadline.saturating_duration_since(std::time::Instant::now());
            let ev = rx.recv_timeout(timeout).expect("expected an Event before deadline");
            if want(&ev) {
                return ev;
            }
        }
    }

    #[test]
    fn dir_scan_emits_DirScanned_with_entries() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("a.txt"), b"x").unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(t.path().to_path_buf()).unwrap();

        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let id = ex.start_dir_scan(PanelSide::Left, dir.clone(), false, SortMode::ByName);

        let ev = drain(&rx, |e| matches!(
            e,
            Event::Worker(_, WorkerMsg::DirScanned { side: PanelSide::Left, .. })
        ));
        match ev {
            Event::Worker(got, WorkerMsg::DirScanned { entries, .. }) => {
                assert_eq!(got, id);
                assert!(entries.iter().any(|e| e.name == "a.txt"));
            }
            _ => unreachable!(),
        }

        ex.join_all();
    }

    #[test]
    fn dir_scan_emits_Failed_for_missing_path() {
        let (tx, rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let _ = ex.start_dir_scan(
            PanelSide::Right,
            camino::Utf8PathBuf::from("/no-such-path-mx-exec-test"),
            false,
            SortMode::ByName,
        );
        let ev = drain(&rx, |e| matches!(e, Event::Worker(_, WorkerMsg::Failed { .. })));
        if let Event::Worker(_, WorkerMsg::Failed { errors }) = ev {
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].1, mx_core::errors::FsError::NotFound);
        }
        ex.join_all();
    }

    #[test]
    fn worker_ids_are_unique_and_monotonic() {
        let (tx, _rx) = mpsc::channel::<Event>();
        let mut ex = Executor::new(tx);
        let t = tempfile::tempdir().unwrap();
        let dir = camino::Utf8PathBuf::from_path_buf(t.path().to_path_buf()).unwrap();
        let a = ex.start_dir_scan(PanelSide::Left,  dir.clone(), false, SortMode::ByName);
        let b = ex.start_dir_scan(PanelSide::Right, dir,         false, SortMode::ByName);
        assert!(b.0 > a.0);
        ex.join_all();
    }
}
```

- [ ] **Step 2: Run executor tests**

Run: `cargo test -p mx-fs executor::tests`
Expected: PASS (3 tests).

- [ ] **Step 3: Verify lints / fmt**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: clean. The `non_snake_case` test name (`dir_scan_emits_DirScanned_with_entries`) will warn — wrap it with `#[allow(non_snake_case)]` immediately above:

```rust
    #[test]
    #[allow(non_snake_case)]
    fn dir_scan_emits_DirScanned_with_entries() { /* … */ }

    #[test]
    #[allow(non_snake_case)]
    fn dir_scan_emits_Failed_for_missing_path() { /* … */ }
```

Run clippy again. Should be clean.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-fs/src/executor.rs
git commit -m "feat(fs): add Executor with start_dir_scan + tempdir tests"
```

---

## Task 5: `mx-core::update` handles `WorkerMsg::DirScanned`

**Files:**
- Modify: `crates/mx-core/src/update.rs`

When the executor finishes a scan, the main loop receives `Event::Worker(id, WorkerMsg::DirScanned { side, entries })`. We swap the panel's entries, clear `loading`, and clamp `cursor`. Other `WorkerMsg`s remain ignored in Phase 2 (`Failed` will surface an error modal in Phase 3 once we have one for it).

- [ ] **Step 1: Write the failing test**

Add to `update.rs` `mod tests`:

```rust
    #[test]
    fn dir_scanned_replaces_panel_entries_and_clamps_cursor() {
        use mx_core::event::{WorkerId, WorkerMsg};
        let mut s = st();
        s.panels[0].cursor = 999; // out of bounds
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
        assert_eq!(s.panels[0].cursor, 2); // clamped
        assert!(!s.panels[0].loading);
        assert!(cmds.is_empty());
    }
```

(Note: the import path inside the test changes from `crate::` to `mx_core::` if writing in an external test file; here we keep `crate::` because tests live inside the `mx-core` crate.)

Wait — `update.rs` is in `mx-core/src/`, and the test imports types from elsewhere. Use `crate::state::DirEntry` (works because we're inside the same crate). Adjust the test code above to match — already uses `crate::state::DirEntry`.

- [ ] **Step 2: Implement the handler**

In `crates/mx-core/src/update.rs`, replace the existing `Event::Worker(_, _)` arm in `update`:

```rust
        Event::Worker(_, _) => { /* Phase 2 */ }
```

with:

```rust
        Event::Worker(id, msg) => {
            handle_worker_msg(&mut state, id, msg);
        }
```

Then add this function near the other helpers in the same file:

```rust
fn handle_worker_msg(state: &mut State, _id: crate::event::WorkerId, msg: crate::event::WorkerMsg) {
    use crate::event::WorkerMsg;
    match msg {
        WorkerMsg::DirScanned { side, entries } => {
            let panel = &mut state.panels[side.index()];
            let new_entries: std::sync::Arc<[_]> = entries.into();
            panel.entries = new_entries;
            panel.loading = false;
            // Clamp cursor to last index, or 0 when the list is empty.
            let last = panel.entries.len().saturating_sub(1);
            if panel.cursor > last { panel.cursor = last; }
            // Bring scroll back into range too.
            if panel.scroll > last { panel.scroll = last; }
        }
        // Phase 3 surfaces these in modals.
        WorkerMsg::Progress { .. }
        | WorkerMsg::Conflict { .. }
        | WorkerMsg::Done
        | WorkerMsg::Failed { .. } => {}
    }
}
```

Also add the matching `use` at the top if not already there:

```rust
use crate::state::PanelSide;
```

(Phase 1 already imports `PanelSide` indirectly via `state::*`; verify.)

- [ ] **Step 3: Run mx-core tests**

Run: `cargo test -p mx-core update::tests`
Expected: PASS (the new test plus all 7 from Phase 1 = 8 total). Workspace test count climbs by 1 for this task.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): handle WorkerMsg::DirScanned by swapping panel entries"
```

---

## Task 6: `mx-core::update` cursor commands

Wire `CursorUp`, `CursorDown`, `CursorPageUp`, `CursorPageDown`, `CursorHome`, `CursorEnd`. Page-size is configurable later; Phase 2 hardcodes a sensible 10. Scroll follows the cursor.

**Files:**
- Modify: `crates/mx-core/src/update.rs`

- [ ] **Step 1: Write the failing tests**

Append to `update.rs` `mod tests`:

```rust
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
```

- [ ] **Step 2: Implement**

In `crates/mx-core/src/update.rs::handle_command_no_modal`, replace the `// Phase 2: implement.` body for the cursor commands. Move them out of the catchall pattern. The function should now look like:

```rust
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

        CommandId::CursorUp        => move_cursor(state, -1),
        CommandId::CursorDown      => move_cursor(state,  1),
        CommandId::CursorPageUp    => move_cursor(state, -(PAGE as isize)),
        CommandId::CursorPageDown  => move_cursor(state,  PAGE as isize),
        CommandId::CursorHome      => set_cursor(state, 0),
        CommandId::CursorEnd       => {
            let last = state.focused().entries.len().saturating_sub(1);
            set_cursor(state, last);
        }

        // Still Phase 2 / Phase 3 work below.
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

fn move_cursor(state: &mut State, delta: isize) {
    let panel = state.focused_mut();
    if panel.entries.is_empty() { return; }
    let last = panel.entries.len() - 1;
    let cur  = panel.cursor as isize;
    let next = (cur + delta).clamp(0, last as isize) as usize;
    panel.cursor = next;
}

fn set_cursor(state: &mut State, i: usize) {
    let panel = state.focused_mut();
    if panel.entries.is_empty() { return; }
    let last = panel.entries.len() - 1;
    panel.cursor = i.min(last);
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mx-core update::tests`
Expected: PASS (5 new + previous = 13 total in `update::tests`).

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): cursor up/down/page/home/end commands"
```

---

## Task 7: `mx-core::update` `EnterDir` / `ParentDir`

**Files:**
- Modify: `crates/mx-core/src/update.rs`

`EnterDir` on a directory entry rewrites the focused panel's `cwd`, sets `loading = true`, clears the cursor/scroll/selection, and emits `Command::RescanDir(side)`. On a non-directory, it's a no-op (Phase 2 doesn't open files via Enter — F3 is the viewer).

`ParentDir` walks `cwd.parent()` if any, same effect.

- [ ] **Step 1: Write the failing tests**

Append to `update::tests`:

```rust
    #[test]
    fn enter_dir_on_dir_changes_cwd_and_emits_rescan() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::dir("subdir"),
        ].into();
        s.panels[0].cursor = 1; // on "subdir"
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
```

- [ ] **Step 2: Implement**

In `handle_command_no_modal`, **remove** `EnterDir` and `ParentDir` from the catchall pattern and add explicit arms (and a helper):

```rust
        CommandId::EnterDir => {
            let side = state.focus;
            let panel = state.focused();
            if panel.entries.is_empty() { return Vec::new(); }
            let entry = &panel.entries[panel.cursor];
            if !entry.is_dir_like() { return Vec::new(); }
            let target = if entry.name == ".." {
                match panel.cwd.parent() {
                    Some(p) if !p.as_str().is_empty() => p.to_path_buf(),
                    _ => return Vec::new(),
                }
            } else {
                panel.cwd.join(&entry.name)
            };
            cd_to(state, side, target);
            return vec![Command::RescanDir(side)];
        }
        CommandId::ParentDir => {
            let side = state.focus;
            let parent = match state.focused().cwd.parent() {
                Some(p) if !p.as_str().is_empty() => p.to_path_buf(),
                _ => return Vec::new(),
            };
            cd_to(state, side, parent);
            return vec![Command::RescanDir(side)];
        }
```

Then add `cd_to` near the other helpers:

```rust
fn cd_to(state: &mut State, side: PanelSide, dir: camino::Utf8PathBuf) {
    let panel = &mut state.panels[side.index()];
    panel.cwd = dir;
    panel.entries = std::sync::Arc::new([]);
    panel.cursor = 0;
    panel.scroll = 0;
    panel.selection.clear();
    panel.loading = true;
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mx-core update::tests`
Expected: PASS (5 new + previous = 18 total).

- [ ] **Step 4: Commit**

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): EnterDir / ParentDir change cwd and emit RescanDir"
```

---

## Task 8: `mx-core::update` `ToggleHidden` / `CycleSort`

**Files:**
- Modify: `crates/mx-core/src/update.rs`

Both flip a panel flag and request a rescan with the new flag.

- [ ] **Step 1: Write the failing tests**

Append:

```rust
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
```

- [ ] **Step 2: Implement**

In `handle_command_no_modal`, lift `ToggleHidden` and `CycleSort` out of the catchall:

```rust
        CommandId::ToggleHidden => {
            let side = state.focus;
            state.focused_mut().show_hidden = !state.focused().show_hidden;
            return vec![Command::RescanDir(side)];
        }
        CommandId::CycleSort => {
            use crate::state::SortMode;
            let side = state.focus;
            let panel = state.focused_mut();
            panel.sort = match panel.sort {
                SortMode::ByName     => SortMode::BySize,
                SortMode::BySize     => SortMode::ByModified,
                SortMode::ByModified => SortMode::ByName,
            };
            return vec![Command::RescanDir(side)];
        }
```

- [ ] **Step 3: Run tests + commit**

Run: `cargo test -p mx-core update::tests`
Expected: PASS.

```bash
git add crates/mx-core/src/update.rs
git commit -m "feat(core): ToggleHidden and CycleSort with rescan"
```

---

## Task 9: `mx-core::update` `RescanFocused` / `RescanBoth`

**Files:**
- Modify: `crates/mx-core/src/update.rs`

- [ ] **Step 1: Write the failing tests**

Append:

```rust
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
```

- [ ] **Step 2: Implement**

Lift the two arms out of the catchall:

```rust
        CommandId::RescanFocused => {
            let side = state.focus;
            // Mark loading immediately so the UI shows the spinner before
            // the worker reports back.
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
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p mx-core update::tests
git add crates/mx-core/src/update.rs
git commit -m "feat(core): RescanFocused and RescanBoth commands"
```

---

## Task 10: `mx-core::update` selection commands

**Files:**
- Modify: `crates/mx-core/src/update.rs`

`ToggleSelect` flips the focused row's membership; `SelectAll` adds every index *except* `..` if present; `SelectNone` clears; `InvertSelection` flips every index (also skipping `..`).

- [ ] **Step 1: Write the failing tests**

Append:

```rust
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
        ].into();
        let (s, _) = update(s, Event::Command(CommandId::SelectAll));
        assert!(!s.panels[0].selection.contains(&0));   // ".." skipped
        assert!( s.panels[0].selection.contains(&1));
        assert!( s.panels[0].selection.contains(&2));
    }

    #[test]
    fn invert_selection_skips_parent_dot_dot() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("a", 1),
            crate::state::DirEntry::file("b", 2),
        ].into();
        s.panels[0].selection.insert(1);
        let (s, _) = update(s, Event::Command(CommandId::InvertSelection));
        assert!(!s.panels[0].selection.contains(&0));   // ".." stays out
        assert!(!s.panels[0].selection.contains(&1));   // flipped off
        assert!( s.panels[0].selection.contains(&2));   // flipped on
    }

    #[test]
    fn select_none_clears() {
        let mut s = st_with_entries(5);
        s.panels[0].selection.insert(1);
        s.panels[0].selection.insert(3);
        let (s, _) = update(s, Event::Command(CommandId::SelectNone));
        assert!(s.panels[0].selection.is_empty());
    }
```

- [ ] **Step 2: Implement**

Lift the four arms out of the catchall:

```rust
        CommandId::ToggleSelect => {
            let panel = state.focused_mut();
            if panel.entries.is_empty() { return Vec::new(); }
            let i = panel.cursor;
            // Don't allow selecting `..`.
            if panel.entries[i].name == ".." { return Vec::new(); }
            if !panel.selection.insert(i) {
                panel.selection.remove(&i);
            }
        }
        CommandId::SelectAll => {
            let panel = state.focused_mut();
            for (i, e) in panel.entries.iter().enumerate() {
                if e.name != ".." { panel.selection.insert(i); }
            }
        }
        CommandId::SelectNone => {
            state.focused_mut().selection.clear();
        }
        CommandId::InvertSelection => {
            let panel = state.focused_mut();
            let n = panel.entries.len();
            for i in 0..n {
                if panel.entries[i].name == ".." { continue; }
                if !panel.selection.insert(i) {
                    panel.selection.remove(&i);
                }
            }
        }
```

- [ ] **Step 3: Run tests + commit**

```bash
cargo test -p mx-core update::tests
git add crates/mx-core/src/update.rs
git commit -m "feat(core): selection commands (ToggleSelect/SelectAll/SelectNone/InvertSelection)"
```

---

## Task 11: `mx-tui::format` and entry-row rendering in `view.rs`

**Files:**
- Create: `crates/mx-tui/src/format.rs`
- Modify: `crates/mx-tui/src/view.rs`
- Modify: `crates/mx-tui/src/lib.rs`
- Modify: `crates/mx-tui/Cargo.toml`

- [ ] **Step 1: Add `chrono` to `mx-tui` deps**

In `crates/mx-tui/Cargo.toml`, under `[dependencies]`, add:

```toml
chrono  = { workspace = true }
mx-fs   = { workspace = true }
```

(`mx-fs` for the size formatter; `chrono` for mtime formatting.)

- [ ] **Step 2: Write `crates/mx-tui/src/format.rs`**

```rust
//! Display formatters used by the panel renderer.

use std::time::SystemTime;

use chrono::{DateTime, Local};

/// Format an mtime per the user's `date_format`. Returns blanks when the
/// metadata wasn't available.
#[must_use]
pub fn format_mtime(t: Option<SystemTime>, fmt: &str) -> String {
    let Some(t) = t else { return "                ".to_string(); };
    let dt: DateTime<Local> = t.into();
    dt.format(fmt).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn none_renders_as_padding() {
        let s = format_mtime(None, "%Y-%m-%d %H:%M");
        assert_eq!(s.len(), 16);
        assert!(s.chars().all(|c| c == ' '));
    }

    #[test]
    fn known_epoch_round_trips() {
        // 2024-01-02 03:04:05 UTC
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_704_164_645);
        let s = format_mtime(Some(t), "%Y-%m-%d");
        // Local time may shift the day; just assert the year matches.
        assert!(s.starts_with("2024"));
    }
}
```

- [ ] **Step 3: Update `mx-tui/src/lib.rs`**

```rust
//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the `crossterm` → `mx_core::InputEvent`
//! translation.

#![forbid(unsafe_code)]

pub mod format;
pub mod input_thread;
pub mod input_xlate;
pub mod layout;
pub mod renderer;
pub mod theme_styles;
pub mod view;
```

- [ ] **Step 4: Modify `crates/mx-tui/src/view.rs`** — replace the `render_panel` function with one that renders entries:

```rust
fn render_panel(frame: &mut Frame<'_>, area: Rect, state: &State, side: PanelSide) {
    use mx_core::state::EntryKind;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let theme   = &state.config.theme;
    let panel   = &state.panels[side.index()];
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

    if panel.loading {
        let p = Paragraph::new("loading…").style(frame_style(theme));
        let one_line = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1) / 2,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, one_line);
        return;
    }

    if panel.entries.is_empty() {
        let p = Paragraph::new("(empty)").style(frame_style(theme));
        let one_line = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1) / 2,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, one_line);
        return;
    }

    // Visible window: keep cursor on screen.
    let visible_h = inner.height as usize;
    let scroll = clamp_scroll(panel.scroll, panel.cursor, visible_h, panel.entries.len());

    // Column widths. Mtime is always 16 cols ("YYYY-MM-DD HH:MM"); size is 7.
    // Drop mtime when inner width < 50; drop size too when < 30.
    let w = inner.width as usize;
    let show_mtime = w >= 50;
    let show_size  = w >= 30;
    let mtime_w: usize = if show_mtime { 17 } else { 0 }; // 1 leading space + 16
    let size_w:  usize = if show_size  {  8 } else { 0 }; // 1 leading space + 7
    let name_w           = w.saturating_sub(mtime_w + size_w + 2 /* select gutter + trailing space */);

    let bg_focus = panel_title_style(theme, focused).bg.unwrap_or_default();
    let _ = bg_focus;

    for row in 0..visible_h {
        let entry_index = scroll + row;
        if entry_index >= panel.entries.len() { break; }
        let entry = &panel.entries[entry_index];

        let is_cursor   = entry_index == panel.cursor && focused;
        let is_selected = panel.selection.contains(&entry_index);

        let kind_style = match entry.kind {
            EntryKind::Dir        => Style::default().fg(super::theme_styles::rcolor(theme.dir_fg)),
            EntryKind::Symlink if entry.symlink_broken
                                  => Style::default().fg(super::theme_styles::rcolor(theme.error_fg)),
            EntryKind::Symlink    => Style::default().fg(super::theme_styles::rcolor(theme.symlink_fg)),
            EntryKind::Unreadable => Style::default().fg(super::theme_styles::rcolor(theme.error_fg)),
            EntryKind::File       => Style::default().fg(super::theme_styles::rcolor(theme.fg)),
        };

        let mut row_style = frame_style(theme).patch(kind_style);
        if is_selected {
            row_style = row_style
                .bg(super::theme_styles::rcolor(theme.selection_bg))
                .fg(super::theme_styles::rcolor(theme.selection_fg))
                .add_modifier(Modifier::BOLD);
        }
        if is_cursor {
            row_style = row_style.add_modifier(Modifier::REVERSED);
        }

        // Build the row text: select-gutter, name, size, mtime.
        let gutter = if is_selected { "•" } else { " " };
        let name_render = render_name(&entry.name, name_w);
        let size_render = if show_size {
            format!(" {}", mx_fs::format::format_size(entry.size))
        } else {
            String::new()
        };
        let mtime_render = if show_mtime {
            format!(" {}", crate::format::format_mtime(entry.mtime, &state.config.ui.date_format))
        } else {
            String::new()
        };

        let line = Line::from(vec![
            Span::raw(gutter),
            Span::raw(name_render),
            Span::raw(size_render),
            Span::raw(mtime_render),
        ]);
        let p = Paragraph::new(line).style(row_style);
        let row_area = Rect {
            x: inner.x,
            y: inner.y + row as u16,
            width: inner.width,
            height: 1,
        };
        frame.render_widget(p, row_area);
    }
}

fn render_name(name: &str, width: usize) -> String {
    use std::fmt::Write as _;
    let chars: Vec<char> = name.chars().collect();
    let mut s = String::with_capacity(width);
    if chars.len() <= width {
        s.push_str(name);
        for _ in chars.len()..width { s.push(' '); }
    } else if width >= 1 {
        // Truncate with ellipsis.
        let take = width.saturating_sub(1);
        for c in chars.iter().take(take) { s.push(*c); }
        let _ = write!(s, "…");
    }
    s
}

/// Choose a `scroll` so that `cursor` is on-screen and we don't scroll past
/// the end.
fn clamp_scroll(scroll: usize, cursor: usize, visible_h: usize, len: usize) -> usize {
    if visible_h == 0 || len == 0 { return 0; }
    let max_scroll = len.saturating_sub(visible_h);
    let mut s = scroll.min(max_scroll);
    if cursor < s { s = cursor; }
    if cursor >= s + visible_h { s = cursor + 1 - visible_h; }
    s.min(max_scroll)
}
```

Also update the imports at the top of `view.rs`:

```rust
use mx_core::state::{Modal, PanelSide, State};

use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::layout::FrameLayout;
use crate::theme_styles::{frame_style, modal_style, panel_title_style, status_style};
```

`render_panel`'s `clamp_scroll` is intentionally pure — Phase 2 leaves the in-State `panel.scroll` untouched (we only adjust the *displayed* scroll). Storing the chosen scroll back into State after `view()` would couple rendering and update; we keep them decoupled.

- [ ] **Step 5: Status line shows entry counts**

Replace `render_status` to compute counts from the focused panel:

```rust
fn render_status(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let panel = state.focused();
    let (files, dirs, bytes) = panel.entries.iter().fold((0u64, 0u64, 0u64), |(f, d, b), e| {
        match e.kind {
            mx_core::state::EntryKind::Dir       => (f, d + 1, b),
            mx_core::state::EntryKind::Symlink   => (f + 1, d, b + e.size.unwrap_or(0)),
            mx_core::state::EntryKind::File      => (f + 1, d, b + e.size.unwrap_or(0)),
            mx_core::state::EntryKind::Unreadable => (f, d, b),
        }
    });
    let text = if state.status.text.is_empty() {
        format!(" {files} files, {dirs} dirs, {} ", mx_fs::format::format_size(Some(bytes)).trim_start())
    } else {
        format!(" {}", state.status.text)
    };
    frame.render_widget(Paragraph::new(text).style(status_style(theme)), area);
}
```

- [ ] **Step 6: Build + test**

Run: `cargo build -p mx-tui`
Expected: clean.

Run: `cargo test -p mx-tui --lib`
Expected: previous unit tests still pass; renderer determinism test still passes.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. If pedantic flags `cast_possible_truncation` on the `as u16`, replace with `u16::try_from(row).unwrap_or(u16::MAX)` or annotate `#[allow(clippy::cast_possible_truncation)]` on the loop.

- [ ] **Step 7: Commit**

```bash
git add crates/mx-tui crates/mx-tui/Cargo.toml
git commit -m "feat(tui): render panel entries with size/mtime columns, cursor + selection"
```

---

## Task 12: Snapshot fixtures for populated panels

**Files:**
- Modify: `crates/mx-tui/tests/render_snapshots.rs`

The Phase 1 fixtures (empty panels, narrow, help modal) stay; we add fixtures for populated panels and selection.

- [ ] **Step 1: Append new tests**

Append to `crates/mx-tui/tests/render_snapshots.rs`:

```rust
fn populated_state(theme: Theme) -> State {
    use mx_core::state::DirEntry;
    let c = Config { theme, ..Config::default() };
    let mut s = State::new(Arc::new(c), "/Users/boogie/proj".into(), "/tmp".into());
    s.focus = PanelSide::Left;
    let entries = vec![
        DirEntry::parent(),
        DirEntry::dir("src"),
        DirEntry::dir("target"),
        DirEntry::file("Cargo.toml", 1234),
        DirEntry::file("README.md", 4321),
        DirEntry::file("rust-toolchain.toml", 80),
        DirEntry::symlink("link-to-src", "src"),
    ];
    s.panels[0].entries = entries.into();
    s.panels[0].cursor = 3; // on Cargo.toml
    s.panels[0].selection.insert(4); // README.md selected
    s
}

#[test]
fn populated_classic_wide_with_selection_and_cursor() {
    let s = populated_state(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect { x: 0, y: 0, width: 100, height: 30 },
    );
    insta::assert_snapshot!("populated_classic_wide", buffer_snapshot(&buf));
}

#[test]
fn populated_dark_wide() {
    let s = populated_state(Theme::DARK);
    let buf = draw_to_buffer(
        &s,
        Rect { x: 0, y: 0, width: 100, height: 30 },
    );
    insta::assert_snapshot!("populated_dark_wide", buffer_snapshot(&buf));
}

#[test]
fn populated_classic_narrow_drops_columns() {
    let s = populated_state(Theme::CLASSIC);
    let buf = draw_to_buffer(
        &s,
        Rect { x: 0, y: 0, width: 60, height: 24 },
    );
    insta::assert_snapshot!("populated_classic_narrow", buffer_snapshot(&buf));
}
```

- [ ] **Step 2: Generate and accept snapshots**

Run: `INSTA_UPDATE=new cargo test -p mx-tui --test render_snapshots`
Expected: 3 new tests fail; `.snap.new` files created in `crates/mx-tui/tests/snapshots/`.

Inspect each new file (visually verify): the populated wide snapshot must show the directory `src` and `target` highlighted (dir color), `README.md` with the selection bullet, `Cargo.toml` reverse-video as the cursor, plus a size column. The narrow snapshot drops the mtime column.

Accept:

```bash
cd crates/mx-tui/tests/snapshots
for f in *.snap.new; do mv "$f" "${f%.new}"; done
cd -
```

- [ ] **Step 3: Re-run tests**

Run: `cargo test -p mx-tui --test render_snapshots`
Expected: PASS — 4 prior + 3 new = 7 fixtures.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-tui/tests
git commit -m "test(tui): snapshot populated panels with cursor, selection, narrow mode"
```

---

## Task 13: `mx-preview::PlainTextPreviewer`

**Files:**
- Modify: `crates/mx-preview/src/plain_text.rs`
- Modify: `crates/mx-preview/src/lib.rs`

The `Previewer` trait is the seam the future `mxview` binary builds on. v1 ships one impl that handles the common case: read up to N bytes, decode lossily, return a string.

- [ ] **Step 1: Write the failing tests**

Replace `crates/mx-preview/src/plain_text.rs`:

```rust
//! `Previewer` trait + plain-text implementation.

use std::fs::File;
use std::io::Read;

use camino::Utf8Path;

use mx_core::errors::FsError;

/// Strategy for converting a path to a previewable text body.
pub trait Previewer {
    /// Returns the preview body. The caller renders it with whatever
    /// styling the modal provides; previewers don't paint.
    ///
    /// # Errors
    ///
    /// Returns `FsError` for filesystem failures. A binary file is *not* an
    /// error — `PlainTextPreviewer::preview` returns a `[binary file]`
    /// placeholder so the viewer modal can show the file size and that's it.
    fn preview(&self, path: &Utf8Path) -> Result<Preview, FsError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    pub body:     String,
    pub truncated: bool,
    pub binary:   bool,
}

pub struct PlainTextPreviewer {
    pub max_bytes: usize,
}

impl Default for PlainTextPreviewer {
    fn default() -> Self {
        // 1 MiB cap for v1.
        Self { max_bytes: 1024 * 1024 }
    }
}

impl Previewer for PlainTextPreviewer {
    fn preview(&self, path: &Utf8Path) -> Result<Preview, FsError> {
        let mut f = File::open(path).map_err(map_io)?;
        let mut buf = Vec::with_capacity(self.max_bytes.min(64 * 1024));
        let mut chunk = [0u8; 8192];
        let mut truncated = false;
        loop {
            let n = f.read(&mut chunk).map_err(map_io)?;
            if n == 0 { break; }
            if buf.len() + n > self.max_bytes {
                let take = self.max_bytes - buf.len();
                buf.extend_from_slice(&chunk[..take]);
                truncated = true;
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let binary = looks_binary(&buf);
        let body = if binary {
            format!("[binary file: {} bytes]", buf.len())
        } else {
            String::from_utf8_lossy(&buf).into_owned()
        };
        Ok(Preview { body, truncated, binary })
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    // Heuristic: a NUL byte in the first 8 KiB usually means binary.
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

fn map_io(e: std::io::Error) -> FsError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound          => FsError::NotFound,
        K::PermissionDenied  => FsError::PermissionDenied,
        K::IsADirectory      => FsError::IsADirectory,
        _                    => FsError::Io(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn td_path(p: &std::path::Path) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(p.to_path_buf()).unwrap()
    }

    #[test]
    fn previews_small_text_file() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("hello.txt");
        std::fs::write(&path, b"hello\nworld\n").unwrap();
        let pv = PlainTextPreviewer::default();
        let p = pv.preview(&td_path(&path)).unwrap();
        assert_eq!(p.body, "hello\nworld\n");
        assert!(!p.truncated);
        assert!(!p.binary);
    }

    #[test]
    fn truncates_oversize_text() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("big.txt");
        std::fs::write(&path, vec![b'a'; 100]).unwrap();
        let pv = PlainTextPreviewer { max_bytes: 50 };
        let p = pv.preview(&td_path(&path)).unwrap();
        assert_eq!(p.body.len(), 50);
        assert!(p.truncated);
    }

    #[test]
    fn binary_files_show_placeholder() {
        let t = tempfile::tempdir().unwrap();
        let path = t.path().join("bin.dat");
        std::fs::write(&path, vec![0u8, 1, 2, 3, 0, 4, 5]).unwrap();
        let pv = PlainTextPreviewer::default();
        let p = pv.preview(&td_path(&path)).unwrap();
        assert!(p.binary);
        assert!(p.body.starts_with("[binary file"));
    }

    #[test]
    fn missing_path_returns_not_found() {
        let pv = PlainTextPreviewer::default();
        assert_eq!(
            pv.preview(&Utf8PathBuf::from("/no/such/file/mx-preview-test")).unwrap_err(),
            FsError::NotFound,
        );
    }
}
```

- [ ] **Step 2: Update `mx-preview/src/lib.rs` re-exports**

```rust
//! Midnight X content preview. Defines the `Previewer` trait — the seam the
//! future `mxview` binary and rich previewers (markdown, image, hex) will
//! implement. Phase 2 ships only `PlainTextPreviewer`.

#![forbid(unsafe_code)]

pub mod plain_text;

pub use plain_text::{PlainTextPreviewer, Preview, Previewer};
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mx-preview`
Expected: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/mx-preview
git commit -m "feat(preview): Previewer trait + PlainTextPreviewer with binary heuristic"
```

---

## Task 14: `Modal::Viewer` + `View` command opens it

**Files:**
- Modify: `crates/mx-core/src/state.rs` (add Viewer modal variant)
- Modify: `crates/mx-core/src/update.rs`

The `View` command must:

1. Confirm the focused entry is a regular file (no Enter/View on dirs in v1).
2. Build a `Modal::Viewer { path, body, scroll, truncated, binary }`.
3. Open it.

Phase 2 reads the file synchronously inside `update()` for now — file viewing is bounded by `PlainTextPreviewer::max_bytes` so we won't stall the loop on multi-MB files. (A worker-driven async viewer is a Phase 3 polish item.) Since `mx-core` may not depend on `mx-preview` (boundary rule: `mx-core` is the data hub), we instead emit `Command::OpenViewer(path)` and let the `Executor` (or main loop) load it. Already what spec §3 prescribes.

So Phase 2's wiring is:
- `update()`: when `View` fires, emit `Command::OpenViewer(path)`. Open `Modal::Viewer { path, body: "loading…".into(), … }` immediately so the user gets feedback.
- `app.rs`: handle `Command::OpenViewer(path)` by calling `PlainTextPreviewer::preview(&path)` synchronously and feeding the result back via a new `Event` variant: `Event::PreviewLoaded { path, body, truncated, binary }` — or simpler: the result becomes a `WorkerMsg::DirScanned`-style message. Since this isn't a worker, we can introduce `Event::PreviewLoaded { path: Utf8PathBuf, result: Result<Preview, FsError> }` and have `update()` swap the modal body when it arrives.

This task creates **the modal type and the dispatch path**. Task 17 wires `app.rs`. The `Event::PreviewLoaded` variant is added in this task too.

- [ ] **Step 1: Add the `Modal::Viewer` variant**

In `crates/mx-core/src/state.rs`, replace the `Modal` enum:

```rust
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
    pub path:      Utf8PathBuf,
    pub body:      String,
    pub scroll:    usize,
    pub truncated: bool,
    pub binary:    bool,
    pub loading:   bool,
}
```

(The `Utf8PathBuf` import already exists at the top of `state.rs`.)

- [ ] **Step 2: Add the `Event::PreviewLoaded` variant**

In `crates/mx-core/src/event.rs`, replace the `Event` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Input(InputEvent),
    Command(CommandId),
    Worker(WorkerId, WorkerMsg),
    Tick { dt: Duration },
    Resize { cols: u16, rows: u16 },
    PreviewLoaded { path: camino::Utf8PathBuf, body: String, truncated: bool, binary: bool },
    PreviewFailed { path: camino::Utf8PathBuf, error: FsError },
}
```

- [ ] **Step 3: Write the failing tests**

Append to `update::tests`:

```rust
    #[test]
    fn view_command_on_file_opens_loading_viewer_and_emits_open_viewer() {
        let mut s = st();
        s.panels[0].entries = vec![
            crate::state::DirEntry::parent(),
            crate::state::DirEntry::file("README.md", 100),
        ].into();
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
            scroll: 0, truncated: false, binary: false, loading: true,
        }));
        let (s, _) = update(s, Event::PreviewLoaded {
            path: "/x/README.md".into(),
            body: "actual content".into(),
            truncated: false,
            binary: false,
        });
        match s.modal {
            Some(Modal::Viewer(v)) => {
                assert_eq!(v.body, "actual content");
                assert!(!v.loading);
            }
            _ => panic!(),
        }
    }
```

- [ ] **Step 4: Implement `View` and `PreviewLoaded`/`PreviewFailed`**

Lift `View` out of the catchall in `handle_command_no_modal`:

```rust
        CommandId::View => {
            use crate::state::{EntryKind, Modal, ViewerDialog};
            let panel = state.focused();
            if panel.entries.is_empty() { return Vec::new(); }
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
```

In `update()`, add arms for the two new events:

```rust
        Event::PreviewLoaded { path, body, truncated, binary } => {
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
```

- [ ] **Step 5: Allow `Esc` and `View`/`F3` to close the viewer modal**

In `dispatch()`, extend the modal-aware match:

```rust
            (Modal::Viewer(_), CommandId::View | CommandId::Cancel) => {
                state.modal = None;
                return Vec::new();
            }
```

(Place this arm above the existing `(_, CommandId::Cancel) | (Modal::Help, CommandId::Help)` line so the more specific match wins.)

- [ ] **Step 6: Wire viewer scrolling**

Inside `dispatch()`, *before* the existing modal-aware match, add scroll handling for the viewer:

```rust
    if let Some(Modal::Viewer(v)) = &mut state.modal {
        match id {
            CommandId::CursorDown     => { v.scroll = v.scroll.saturating_add(1); return Vec::new(); }
            CommandId::CursorUp       => { v.scroll = v.scroll.saturating_sub(1); return Vec::new(); }
            CommandId::CursorPageDown => { v.scroll = v.scroll.saturating_add(20); return Vec::new(); }
            CommandId::CursorPageUp   => { v.scroll = v.scroll.saturating_sub(20); return Vec::new(); }
            CommandId::CursorHome     => { v.scroll = 0;                          return Vec::new(); }
            CommandId::CursorEnd      => { v.scroll = usize::MAX;                 return Vec::new(); }
            _ => {}
        }
    }
```

(`usize::MAX` will be clamped against the body line count by the renderer in Task 15.)

- [ ] **Step 7: Run mx-core tests**

Run: `cargo test -p mx-core`
Expected: PASS — 3 new tests bring the count up.

- [ ] **Step 8: Commit**

```bash
git add crates/mx-core
git commit -m "feat(core): add Modal::Viewer; View opens it and emits OpenViewer"
```

---

## Task 15: Render `Modal::Viewer` in `mx-tui::view::render_modal`

**Files:**
- Modify: `crates/mx-tui/src/view.rs`

- [ ] **Step 1: Extend `render_modal`**

In the existing `render_modal` function, add the `Viewer` arms to the two matches and add a separate body renderer below:

Replace the `title` match:

```rust
    let title = match modal {
        Modal::Help        => " Help ",
        Modal::QuitConfirm => " Quit? ",
        Modal::Confirm(_)  => " Confirm ",
        Modal::Input(_)    => " Input ",
        Modal::Progress(_) => " Working… ",
        Modal::Error(_)    => " Error ",
        Modal::Viewer(_)   => " View (Esc/F3 close, ↑↓ scroll) ",
    };
```

Replace the `body` match:

```rust
    let body = match modal {
        Modal::Help => {
            "Phase 1 help: F10/Ctrl-Q quits, Tab toggles focus, Esc cancels.\n\n\
             (Press F1 again or Esc to dismiss.)"
                .to_string()
        }
        Modal::QuitConfirm => {
            "Workers are still running. Quit anyway?\n\n[ Yes ]   [ No ]".to_string()
        }
        Modal::Error(d)    => d.body.clone(),
        Modal::Confirm(d)  => d.body.clone(),
        Modal::Input(d)    => format!("{}\n> {}", d.prompt, d.value),
        Modal::Progress(d) => {
            format!(
                "{}\n{} / {} bytes",
                d.current_path, d.bytes_done, d.bytes_total
            )
        }
        Modal::Viewer(d) => render_viewer_body(d, area),
    };
```

Then add the helper at the bottom of the file:

```rust
fn render_viewer_body(d: &mx_core::state::ViewerDialog, area: Rect) -> String {
    if d.loading { return "loading…".to_string(); }
    let total: Vec<&str> = d.body.lines().collect();
    let visible = area.height.saturating_sub(2) as usize; // borders take 2
    let max_scroll = total.len().saturating_sub(visible);
    let scroll = d.scroll.min(max_scroll);
    let slice = total.get(scroll..(scroll + visible).min(total.len())).unwrap_or(&[]);
    let mut out = slice.join("\n");
    if d.truncated { out.push_str("\n[truncated]"); }
    out
}
```

- [ ] **Step 2: Add a snapshot fixture for the viewer**

Append to `crates/mx-tui/tests/render_snapshots.rs`:

```rust
#[test]
fn viewer_modal_with_content() {
    use mx_core::state::{Modal, ViewerDialog};
    let mut s = fresh(Theme::CLASSIC);
    s.modal = Some(Modal::Viewer(ViewerDialog {
        path: "/x/hello.txt".into(),
        body: "line one\nline two\nline three".into(),
        scroll: 0,
        truncated: false,
        binary: false,
        loading: false,
    }));
    let buf = draw_to_buffer(&s, Rect { x: 0, y: 0, width: 100, height: 30 });
    insta::assert_snapshot!("viewer_classic_wide", buffer_snapshot(&buf));
}
```

- [ ] **Step 3: Generate + accept**

```bash
INSTA_UPDATE=new cargo test -p mx-tui --test render_snapshots
mv crates/mx-tui/tests/snapshots/render_snapshots__viewer_classic_wide.snap.new \
   crates/mx-tui/tests/snapshots/render_snapshots__viewer_classic_wide.snap
cargo test -p mx-tui --test render_snapshots
```

Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add crates/mx-tui
git commit -m "feat(tui): render viewer modal with scrollable body"
```

---

## Task 16: Help modal renders the actual keymap table

**Files:**
- Modify: `crates/mx-tui/src/view.rs`
- Modify: `crates/mx-core/src/keymap.rs` (add a `pub fn pretty_bindings` or similar)

The Help modal currently shows static text. We replace it with a grouped table generated from the live `Keymap`.

- [ ] **Step 1: Add a `Keymap::sorted_bindings` method**

In `crates/mx-core/src/keymap.rs`, add to `impl Keymap`:

```rust
    /// Borrow the internal binding list in insertion order. Used by the
    /// Help modal renderer.
    #[must_use]
    pub fn bindings_view(&self) -> &[(Vec<KeyChord>, CommandId)] {
        &self.bindings
    }
```

- [ ] **Step 2: Add a chord-stringifier in `mx-core::keymap`**

Append to `keymap.rs`:

```rust
/// Render a `KeyChord` back to the same string the keymap parser accepts
/// (`"ctrl-shift-pgup"`, `"esc"`, `"f5"`).
#[must_use]
pub fn chord_to_string(c: KeyChord) -> String {
    let mut s = String::new();
    if c.mods.ctrl  { s.push_str("ctrl-"); }
    if c.mods.shift { s.push_str("shift-"); }
    if c.mods.alt   { s.push_str("alt-"); }
    s.push_str(&keycode_to_string(c.code));
    s
}

#[must_use]
pub fn sequence_to_string(seq: &[KeyChord]) -> String {
    seq.iter().map(|c| chord_to_string(*c)).collect::<Vec<_>>().join(" ")
}

fn keycode_to_string(c: KeyCode) -> String {
    match c {
        KeyCode::Char(c)  => c.to_string(),
        KeyCode::Enter    => "enter".into(),
        KeyCode::Esc      => "esc".into(),
        KeyCode::Tab      => "tab".into(),
        KeyCode::BackTab  => "backtab".into(),
        KeyCode::Backspace=> "backspace".into(),
        KeyCode::Delete   => "delete".into(),
        KeyCode::Insert   => "insert".into(),
        KeyCode::Home     => "home".into(),
        KeyCode::End      => "end".into(),
        KeyCode::PageUp   => "pgup".into(),
        KeyCode::PageDown => "pgdn".into(),
        KeyCode::Up       => "up".into(),
        KeyCode::Down     => "down".into(),
        KeyCode::Left     => "left".into(),
        KeyCode::Right    => "right".into(),
        KeyCode::F(n)     => format!("f{n}"),
        KeyCode::Null     => "null".into(),
    }
}

#[cfg(test)]
mod stringify_tests {
    use super::*;

    #[test]
    fn round_trip_via_strings() {
        let c = KeyChord::new(KeyCode::PageUp, KeyModifiers::ctrl());
        assert_eq!(chord_to_string(c), "ctrl-pgup");
    }

    #[test]
    fn sequence_round_trip() {
        let seq = vec![
            KeyChord::bare(KeyCode::Esc),
            KeyChord::bare(KeyCode::Char('1')),
        ];
        assert_eq!(sequence_to_string(&seq), "esc 1");
    }
}
```

- [ ] **Step 3: Help modal renders bindings**

In `crates/mx-tui/src/view.rs`, replace the `Modal::Help` arm of the `body` match:

```rust
        Modal::Help => render_help_body(&state.config.keymap),
```

But wait — `render_modal` doesn't take `state`. We need to thread it. Change `render_modal`'s signature in this file:

```rust
fn render_modal(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &State,
) {
    let modal = state.modal.as_ref().expect("render_modal called without a modal");
    let theme = &state.config.theme;
```

…and update the `view()` call to it:

```rust
    if let (Some(rect), Some(_)) = (layout.modal, state.modal.as_ref()) {
        let m_area = rect;
        let _ = m_area;
        // re-fetch via state inside render_modal
        let saved_area = rect;
        render_modal_at(frame, saved_area, state);
    }
```

Simpler approach: rename `render_modal` to take `state` and re-derive the `Modal`. Replace the function:

```rust
fn render_modal(frame: &mut Frame<'_>, area: Rect, state: &State) {
    let theme = &state.config.theme;
    let modal = state.modal.as_ref().expect("render_modal requires modal");
    frame.render_widget(Clear, area);
    let title = match modal {
        Modal::Help        => " Help ",
        Modal::QuitConfirm => " Quit? ",
        Modal::Confirm(_)  => " Confirm ",
        Modal::Input(_)    => " Input ",
        Modal::Progress(_) => " Working… ",
        Modal::Error(_)    => " Error ",
        Modal::Viewer(_)   => " View (Esc/F3 close, ↑↓ scroll) ",
    };
    let body = match modal {
        Modal::Help        => render_help_body(&state.config.keymap),
        Modal::QuitConfirm => "Workers are still running. Quit anyway?\n\n[ Yes ]   [ No ]".to_string(),
        Modal::Error(d)    => d.body.clone(),
        Modal::Confirm(d)  => d.body.clone(),
        Modal::Input(d)    => format!("{}\n> {}", d.prompt, d.value),
        Modal::Progress(d) => {
            format!("{}\n{} / {} bytes", d.current_path, d.bytes_done, d.bytes_total)
        }
        Modal::Viewer(d)   => render_viewer_body(d, area),
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
```

And update the `view()` call site:

```rust
    if let (Some(rect), Some(_)) = (layout.modal, state.modal.as_ref()) {
        render_modal(frame, rect, state);
    }
```

Then add `render_help_body`:

```rust
fn render_help_body(keymap: &mx_core::keymap::Keymap) -> String {
    use mx_core::command::CommandId;
    use mx_core::keymap::sequence_to_string;
    let mut groups: Vec<(&'static str, Vec<&CommandId>)> = vec![
        ("Navigation", vec![&CommandId::CursorUp, &CommandId::CursorDown,
                            &CommandId::CursorPageUp, &CommandId::CursorPageDown,
                            &CommandId::CursorHome, &CommandId::CursorEnd,
                            &CommandId::EnterDir, &CommandId::ParentDir,
                            &CommandId::FocusOther, &CommandId::SwapPanels]),
        ("Selection",  vec![&CommandId::ToggleSelect, &CommandId::SelectAll,
                            &CommandId::SelectNone, &CommandId::InvertSelection]),
        ("View",       vec![&CommandId::View, &CommandId::ToggleHidden, &CommandId::CycleSort]),
        ("File ops",   vec![&CommandId::Copy, &CommandId::Move, &CommandId::Delete,
                            &CommandId::Mkdir, &CommandId::Rename]),
        ("App",        vec![&CommandId::Help, &CommandId::QuitConfirm,
                            &CommandId::Quit, &CommandId::Cancel,
                            &CommandId::RescanFocused, &CommandId::RescanBoth]),
    ];

    let mut out = String::new();
    for (group, cmds) in groups.iter_mut() {
        out.push_str(group);
        out.push('\n');
        for c in cmds {
            let label = command_label(**c);
            let bindings: Vec<String> = keymap.bindings_view()
                .iter()
                .filter(|(_, cmd)| cmd == *c)
                .map(|(seq, _)| sequence_to_string(seq))
                .collect();
            if bindings.is_empty() { continue; }
            out.push_str(&format!("  {:<22} {}\n", label, bindings.join(", ")));
        }
        out.push('\n');
    }
    out.push_str("(Esc closes this help.)");
    out
}

fn command_label(c: mx_core::command::CommandId) -> &'static str {
    use mx_core::command::CommandId::*;
    match c {
        CursorUp        => "Move cursor up",
        CursorDown      => "Move cursor down",
        CursorPageUp    => "Page up",
        CursorPageDown  => "Page down",
        CursorHome      => "Top",
        CursorEnd       => "Bottom",
        EnterDir        => "Enter directory",
        ParentDir       => "Parent directory",
        FocusOther      => "Other panel",
        SwapPanels      => "Swap panels",
        ToggleSelect    => "Toggle select",
        SelectAll       => "Select all",
        SelectNone      => "Select none",
        InvertSelection => "Invert selection",
        View            => "View file",
        ToggleHidden    => "Toggle hidden",
        CycleSort       => "Cycle sort mode",
        Copy            => "Copy",
        Move            => "Move",
        Delete          => "Delete",
        Mkdir           => "Make directory",
        Rename          => "Rename",
        Help            => "Help",
        QuitConfirm     => "Quit (confirm)",
        Quit            => "Quit",
        Cancel          => "Cancel / close modal",
        RescanFocused   => "Rescan",
        RescanBoth      => "Rescan both",
    }
}
```

- [ ] **Step 4: The existing `help_modal_classic_wide` snapshot will change**

Run: `cargo test -p mx-tui --test render_snapshots help_modal_open`
Expected: FAIL (snapshot drift). Inspect the new content; it should be a populated keymap table, not the old static text.

Accept the new snapshot:

```bash
INSTA_UPDATE=new cargo test -p mx-tui --test render_snapshots help_modal_open
mv crates/mx-tui/tests/snapshots/render_snapshots__help_modal_classic_wide.snap.new \
   crates/mx-tui/tests/snapshots/render_snapshots__help_modal_classic_wide.snap
cargo test -p mx-tui --test render_snapshots
```

Expected: green.

- [ ] **Step 5: Run mx-core stringify tests**

Run: `cargo test -p mx-core stringify_tests`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/mx-core/src/keymap.rs crates/mx-tui/src/view.rs crates/mx-tui/tests/snapshots
git commit -m "feat(tui): Help modal renders live keymap grouped by category"
```

---

## Task 17: `mx::app` wires `Executor`, initial scans, viewer

**Files:**
- Modify: `crates/mx/Cargo.toml`
- Modify: `crates/mx/src/app.rs`

The main loop now owns an `Executor`. On startup it emits two `RescanDir` commands so both panels populate. It also handles `Command::OpenViewer(path)` synchronously (Phase 2 reads via `PlainTextPreviewer`; Phase 3 may move this to a worker).

- [ ] **Step 1: Add deps**

In `crates/mx/Cargo.toml`, under `[dependencies]`, add:

```toml
mx-fs              = { workspace = true }
mx-preview         = { workspace = true }
```

- [ ] **Step 2: Replace `crates/mx/src/app.rs`**

```rust
//! Main event loop. Owns the State, the Renderer, the input thread, the
//! Executor, and the `mpsc` channel.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use camino::Utf8PathBuf;

use mx_core::command::Command;
use mx_core::config::Config;
use mx_core::event::Event;
use mx_core::state::{PanelSide, State};
use mx_core::update;
use mx_fs::executor::Executor;
use mx_preview::{PlainTextPreviewer, Previewer};
use mx_tui::input_thread;
use mx_tui::renderer;

const TICK: Duration = Duration::from_millis(100);

/// Run the main event loop.
///
/// # Errors
///
/// Propagates io errors from terminal acquisition / rendering.
pub fn run(config: Config) -> Result<()> {
    let cfg = Arc::new(config);

    let cwd: Utf8PathBuf = std::env::current_dir()
        .ok()
        .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        .unwrap_or_else(|| Utf8PathBuf::from("/"));

    let mut state = State::new(Arc::clone(&cfg), cwd.clone(), cwd);
    state.panels[0].loading = true;
    state.panels[1].loading = true;

    let (tx, rx) = mpsc::channel::<Event>();
    let input = input_thread::spawn(tx.clone());

    let mut executor = Executor::new(tx.clone());
    let previewer = PlainTextPreviewer::default();

    // Kick off the initial scan for both panels.
    schedule_rescan(&mut executor, &state, PanelSide::Left);
    schedule_rescan(&mut executor, &state, PanelSide::Right);

    let mut renderer = renderer::acquire()?;
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
            match cmd {
                Command::Quit => break 'main,
                Command::RescanDir(side) => schedule_rescan(&mut executor, &state, side),
                Command::OpenViewer(path) => run_preview(&previewer, &tx, path),
                // Phase 3: copy/move/delete/mkdir/rename/cancel-worker.
                Command::StartCopy { .. }
                | Command::StartMove { .. }
                | Command::StartDelete { .. }
                | Command::Mkdir { .. }
                | Command::Rename { .. }
                | Command::CancelWorker(_) => {}
            }
        }

        if state.should_quit { break 'main; }

        executor.reap();

        let now = Instant::now();
        let dt = now.saturating_duration_since(last_frame);
        last_frame = now;
        let _ = renderer.draw(&state, dt);
    }

    executor.join_all();
    let _ = renderer::release();
    input.stop();
    Ok(())
}

fn schedule_rescan(executor: &mut Executor, state: &State, side: PanelSide) {
    let panel = &state.panels[side.index()];
    let _ = executor.start_dir_scan(
        side,
        panel.cwd.clone(),
        panel.show_hidden,
        panel.sort,
    );
}

fn run_preview(previewer: &PlainTextPreviewer, tx: &mpsc::Sender<Event>, path: Utf8PathBuf) {
    match previewer.preview(&path) {
        Ok(p) => {
            let _ = tx.send(Event::PreviewLoaded {
                path,
                body: p.body,
                truncated: p.truncated,
                binary: p.binary,
            });
        }
        Err(e) => {
            let _ = tx.send(Event::PreviewFailed { path, error: e });
        }
    }
}

```

(The `const _: fn(FsError) = ...;` line keeps `FsError` imported — handy when the only references are inside `Event::PreviewFailed`'s struct literal at the call sites; remove if rustc doesn't complain.)

- [ ] **Step 3: Build + verify the binary**

Run: `cargo build -p mx`
Expected: clean.

Run: `cargo run -p mx -- --version`
Expected: `mx 0.0.1`.

(Interactive run is for the user; no CI-friendly way to verify the rendered TUI here.)

- [ ] **Step 4: Workspace sanity**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: clean. Existing snapshot count should be 7 view-snapshots + 1 viewer + new help-modal = 9 total (or matching whatever was generated above).

- [ ] **Step 5: Commit**

```bash
git add crates/mx
git commit -m "feat(mx): wire Executor; initial scan; sync preview pump"
```

---

## Task 18: CHANGELOG, manual verification, tag

**Files:**
- Modify: `CHANGELOG.md`

- [ ] **Step 1: Manual interactive verification**

Run `cargo run -p mx --release` in your terminal. Verify:

- Both panels populate with the directory you started in.
- Cursor moves with `↑ ↓`, `PgUp`, `PgDn`, `Home`, `End`.
- `Tab` toggles focus (title accent flips).
- `Enter` on a directory descends into it.
- `Backspace` walks up to the parent.
- `Insert` toggles selection on the cursor row (bullet appears).
- `Ctrl-A` selects all (skipping `..`); `*` inverts.
- `Ctrl-H` toggles hidden file display; the panel rescans.
- `Ctrl-S` cycles sort modes (name → size → modified → name); panel rescans.
- `Ctrl-R` rescans the current panel.
- `F1` opens the Help modal — bindings list is populated.
- Place the cursor on a text file, press `F3`. Viewer modal appears with file contents.
- In the viewer: `↑ ↓ PgUp PgDn Home End` scrolls; `Esc` or `F3` closes.
- Place the cursor on a binary file (like a compiled binary) and press `F3` — modal shows `[binary file: N bytes]`.
- `F10` / `Ctrl-Q` quits cleanly.

If any of these fails, file the discrepancy as a bug task and fix before tagging.

- [ ] **Step 2: Update `CHANGELOG.md`**

Replace the file:

```markdown
# Changelog

All notable changes to this project will be documented in this file. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0-phase2] - 2026-05-03

### Added
- New `mx-fs` crate: `Executor` (worker spawning + worker-id allocation),
  synchronous `dir_scan` helper, `format_size` formatter.
- New `mx-preview` crate: `Previewer` trait + `PlainTextPreviewer` with
  binary-file detection via NUL heuristic.
- `chrono` workspace dep for date_format-driven mtime rendering.
- `DirEntry` enriched: `size`, `mtime`, `symlink_target`, `symlink_broken`.
- `WorkerKind::DirScan(side)` and `WorkerMsg::DirScanned { side, entries }`.
- All navigation commands wired in `update()`: `CursorUp/Down/PageUp/Down/Home/End`,
  `EnterDir`, `ParentDir`, `FocusOther`, `SwapPanels`.
- Selection commands: `ToggleSelect`, `SelectAll`, `SelectNone`, `InvertSelection`.
- Sort/hidden/rescan commands: `ToggleHidden`, `CycleSort`, `RescanFocused`, `RescanBoth`.
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

### Changed
- `view.rs::render_panel` now renders entries (rows with name/size/mtime),
  cursor reverse-video, selection bullets, dir/symlink/error coloring.
- `view.rs::render_status` summarises focused-panel entry counts and total bytes.

### Known limitations (Phase 2)
- File operations (copy/move/delete/mkdir/rename) still scaffolded but inactive
  — that's Phase 3.
- The viewer reads files synchronously on the main thread (capped at 1 MiB by
  default). Phase 3 may move this to a worker.
- Filesystem watching (`notify`) is still out — manual `Ctrl-R` is the way to
  refresh.

## [0.1.0-phase1] - 2026-05-03

… (unchanged from Phase 1 entry).
```

(Preserve the Phase 1 section; only the Phase 2 entry is new at the top.)

- [ ] **Step 3: Commit + tag**

```bash
git add CHANGELOG.md
git commit -m "docs: changelog for v0.2.0-phase2"
git tag -a v0.2.0-phase2 -m "Midnight X — Phase 2: navigation, view, F3 plain-text viewer"
```

- [ ] **Step 4: Push (optional)**

If the remote is configured (Phase 1 set `boogie/midnight-x`):

```bash
git push origin main
git push origin v0.2.0-phase2
```

---

## Phase 2 — Acceptance criteria

- [ ] `cargo fmt --all -- --check` clean.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo test --workspace` fully green.
- [ ] `cargo doc --workspace --no-deps` warning-free with `RUSTDOCFLAGS=-D warnings`.
- [ ] `mx` opens a real two-pane file browser of the current directory.
- [ ] All navigation, selection, sort, hidden-toggle, rescan, and view commands behave per the manual checklist above.
- [ ] Help modal lists the live keymap grouped by category.
- [ ] F3 viewer opens a text file, scrolls correctly, closes on `Esc`/`F3`, and shows `[binary file: N bytes]` for binaries.
- [ ] Phase 2 snapshot fixtures (`populated_*`, `viewer_*`, regenerated `help_*`) all exist and pass.
- [ ] `v0.2.0-phase2` tag exists.
- [ ] No AI-tool attribution in commits or files.

When all criteria pass, Phase 2 is done. Phase 3 (file ops + workers + progress dialog) is its own plan.
