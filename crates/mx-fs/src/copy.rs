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
#[allow(clippy::missing_panics_doc)]
pub fn copy_file(
    src: &Utf8Path,
    dst: &Utf8Path,
    cancel: &Arc<AtomicBool>,
    progress: ProgressHandler<'_>,
) -> Result<u64, FsError> {
    let src_meta = std::fs::metadata(src).map_err(|e| dir_scan::map_io_pub(&e))?;
    let total = src_meta.len();

    let mut input = File::open(src).map_err(|e| dir_scan::map_io_pub(&e))?;
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(dst)
        .map_err(|e| dir_scan::map_io_pub(&e))?;

    let mut buf = vec![0u8; COPY_BUFFER];
    let mut copied: u64 = 0;
    let mut last_tick = Instant::now();
    progress(dst, 0, total);

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(output);
            let _ = std::fs::remove_file(dst);
            return Err(FsError::Cancelled);
        }
        let n = input.read(&mut buf).map_err(|e| dir_scan::map_io_pub(&e))?;
        if n == 0 {
            break;
        }
        output
            .write_all(&buf[..n])
            .map_err(|e| dir_scan::map_io_pub(&e))?;
        copied += n as u64;
        if last_tick.elapsed().as_millis() >= PROGRESS_DEBOUNCE_MS {
            progress(dst, copied, total);
            last_tick = Instant::now();
        }
    }
    output.flush().map_err(|e| dir_scan::map_io_pub(&e))?;
    drop(output);

    #[cfg(unix)]
    {
        // Mode preservation is safe; mtime preservation needs `utimensat`,
        // which would require an `unsafe` block forbidden workspace-wide.
        // Tracked as a Phase 4 polish item via the `filetime` crate.
        use std::os::unix::fs::PermissionsExt;
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
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let meta = match std::fs::symlink_metadata(src) {
        Ok(m) => m,
        Err(e) => {
            r.errors.push((src.to_path_buf(), dir_scan::map_io_pub(&e)));
            return;
        }
    };

    if meta.is_symlink() {
        #[cfg(unix)]
        if let Ok(target) = std::fs::read_link(src) {
            if dst.exists() {
                match handler(src, dst) {
                    OverwriteAction::Overwrite => {
                        let _ = std::fs::remove_file(dst);
                    }
                    OverwriteAction::Skip => return,
                    OverwriteAction::Cancel => {
                        cancel.store(true, Ordering::Relaxed);
                        return;
                    }
                }
            }
            if let Err(e) = std::os::unix::fs::symlink(&target, dst) {
                r.errors.push((dst.to_path_buf(), dir_scan::map_io_pub(&e)));
            }
        }
        return;
    }

    if meta.is_dir() {
        if !dst.exists() {
            if let Err(e) = std::fs::create_dir_all(dst) {
                r.errors.push((dst.to_path_buf(), dir_scan::map_io_pub(&e)));
                return;
            }
        }
        let read = match std::fs::read_dir(src) {
            Ok(r) => r,
            Err(e) => {
                r.errors.push((src.to_path_buf(), dir_scan::map_io_pub(&e)));
                return;
            }
        };
        for child in read {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let Ok(child) = child else { continue };
            let Some(name) = child.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let child_src = src.join(&name);
            let child_dst = dst.join(&name);
            copy_inner(&child_src, &child_dst, cancel, progress, handler, r);
        }
        return;
    }

    if dst.exists() {
        match handler(src, dst) {
            OverwriteAction::Overwrite => {}
            OverwriteAction::Skip => return,
            OverwriteAction::Cancel => {
                cancel.store(true, Ordering::Relaxed);
                return;
            }
        }
    }
    match copy_file(src, dst, cancel, progress) {
        Ok(n) => r.copied_bytes += n,
        Err(e) => r.errors.push((dst.to_path_buf(), e)),
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
    fn no_progress() -> impl Fn(&Utf8Path, u64, u64) + Send + Sync {
        |_, _, _| {}
    }
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
        let cancel = Arc::new(AtomicBool::new(true));
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
