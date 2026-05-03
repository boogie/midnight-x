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
/// The first entry is always `..` *unless* `dir` has no parent (only `/` on
/// Unix qualifies).
///
/// # Errors
///
/// Returns `FsError::NotFound` / `PermissionDenied` / etc. for the directory
/// itself. Per-entry metadata failures degrade the entry to `EntryKind::Unreadable`
/// rather than failing the whole scan.
pub fn scan(dir: &Utf8Path, show_hidden: bool, sort: SortMode) -> Result<Vec<DirEntry>, FsError> {
    let read = fs::read_dir(dir).map_err(|e| map_io(&e))?;

    let mut entries: Vec<DirEntry> = Vec::new();
    for r in read {
        let Ok(entry) = r else { continue };
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(os) => {
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
    p.parent().is_some_and(|x| !x.as_str().is_empty())
}

fn sort_entries(entries: &mut [DirEntry], sort: SortMode) {
    let cmp = |a: &DirEntry, b: &DirEntry| -> Ordering {
        match (a.is_dir_like(), b.is_dir_like()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        match sort {
            SortMode::ByName => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortMode::BySize => a
                .size
                .unwrap_or(0)
                .cmp(&b.size.unwrap_or(0))
                .then_with(|| a.name.cmp(&b.name)),
            SortMode::ByModified => mtime_key(a.mtime)
                .cmp(&mtime_key(b.mtime))
                .then_with(|| a.name.cmp(&b.name)),
        }
    };
    entries.sort_by(cmp);
}

fn mtime_key(t: Option<SystemTime>) -> u128 {
    t.and_then(|x| x.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

fn map_io(e: &std::io::Error) -> FsError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound => FsError::NotFound,
        K::PermissionDenied => FsError::PermissionDenied,
        K::AlreadyExists => FsError::AlreadyExists,
        K::NotADirectory => FsError::NotADirectory,
        K::IsADirectory => FsError::IsADirectory,
        _ => FsError::Io(e.to_string()),
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
        std::fs::write(t.path().join("big"), vec![0u8; 100]).unwrap();
        std::fs::write(t.path().join("small"), vec![0u8; 10]).unwrap();
        std::fs::create_dir(t.path().join("d")).unwrap();
        let entries = scan(&td_path(&t), false, SortMode::BySize).unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["..", "d", "small", "big"]);
    }

    #[test]
    fn scan_returns_not_found_for_missing_dir() {
        let err = scan(
            &Utf8PathBuf::from("/no-such-path-mx-test"),
            false,
            SortMode::ByName,
        )
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
