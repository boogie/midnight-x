//! Recursive delete with per-file error tolerance and cooperative
//! cancellation. The synchronous helpers are reusable (tests and the
//! Executor share them); threading lives in `executor.rs`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use camino::Utf8Path;

use mx_core::errors::FsError;

use crate::dir_scan;

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
            r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(&e)));
            return;
        }
    };
    if meta.is_dir() && !meta.is_symlink() {
        let read = match std::fs::read_dir(path) {
            Ok(r) => r,
            Err(e) => {
                r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(&e)));
                return;
            }
        };
        for child in read {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let Ok(child) = child else { continue };
            let Some(child_name) = child.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let child_path = path.join(child_name);
            delete_inner(&child_path, cancel, r);
        }
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        match std::fs::remove_dir(path) {
            Ok(()) => r.deleted += 1,
            Err(e) => r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(&e))),
        }
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => r.deleted += 1,
            Err(e) => r.errors.push((path.to_path_buf(), dir_scan::map_io_pub(&e))),
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
    fn cancel_off() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

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
        assert!(report.deleted >= 5);
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
        assert!(t.path().join("real/sentinel").exists());
        assert!(!t.path().join("link").exists());
    }
}
