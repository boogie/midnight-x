//! Move = rename when same-device, otherwise copy + delete.

use std::sync::atomic::{AtomicBool, Ordering};
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
                cancel.store(true, Ordering::Relaxed);
                report.cancelled = true;
                return report;
            }
        }
    }

    match backend.rename(src, dst) {
        Ok(()) => {
            report.moved += 1;
        }
        Err(FsError::CrossDevice) => {
            let copy = copy_tree(src, dst, cancel, progress, handler);
            report.errors.extend(copy.errors);
            if copy.cancelled {
                report.cancelled = true;
                return report;
            }
            let del = delete_tree(src, cancel);
            report.errors.extend(del.errors);
            if del.cancelled {
                report.cancelled = true;
            }
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
    fn cancel_off() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    fn no_progress() -> impl Fn(&Utf8Path, u64, u64) + Send + Sync {
        |_, _, _| {}
    }
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
