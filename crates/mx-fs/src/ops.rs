//! Synchronous filesystem helpers run on the main thread (mkdir/rename).
//! They return `FsError` on failure; the caller surfaces that via a modal.

use camino::{Utf8Path, Utf8PathBuf};

use mx_core::errors::FsError;

use crate::dir_scan;

/// Create a single new directory under `parent`.
///
/// # Errors
///
/// Returns `FsError::AlreadyExists` if the target already exists,
/// `FsError::NotFound` if `parent` doesn't exist, etc.
pub fn mkdir(parent: &Utf8Path, name: &str) -> Result<Utf8PathBuf, FsError> {
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(FsError::Io(format!("invalid directory name: {name:?}")));
    }
    let path = parent.join(name);
    std::fs::create_dir(&path).map_err(|e| dir_scan::map_io_pub(&e))?;
    Ok(path)
}

/// Rename `from` to `to` (same directory expected; the caller validates).
///
/// # Errors
///
/// Returns `FsError::AlreadyExists` if `to` already exists, etc.
pub fn rename(from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError> {
    if to.exists() {
        return Err(FsError::AlreadyExists);
    }
    std::fs::rename(from, to).map_err(|e| dir_scan::map_io_pub(&e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn td() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn p(d: &tempfile::TempDir) -> Utf8PathBuf {
        Utf8PathBuf::from_path_buf(d.path().to_path_buf()).unwrap()
    }

    #[test]
    fn mkdir_creates_a_new_directory() {
        let t = td();
        let path = mkdir(&p(&t), "newdir").unwrap();
        assert!(path.is_dir());
    }

    #[test]
    fn mkdir_rejects_slash_in_name() {
        let t = td();
        assert!(matches!(mkdir(&p(&t), "a/b"), Err(FsError::Io(_))));
    }

    #[test]
    fn mkdir_already_exists_returns_error() {
        let t = td();
        std::fs::create_dir(t.path().join("there")).unwrap();
        assert_eq!(
            mkdir(&p(&t), "there").unwrap_err(),
            FsError::AlreadyExists
        );
    }

    #[test]
    fn rename_works_and_rejects_overwrite() {
        let t = td();
        std::fs::write(t.path().join("a"), b"x").unwrap();
        let from = p(&t).join("a");
        let to = p(&t).join("b");
        rename(&from, &to).unwrap();
        assert!(to.exists() && !from.exists());

        std::fs::write(t.path().join("c"), b"y").unwrap();
        let c = p(&t).join("c");
        assert_eq!(rename(&c, &to).unwrap_err(), FsError::AlreadyExists);
    }
}
