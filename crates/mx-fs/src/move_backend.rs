//! `MoveBackend` decouples `std::fs::rename` from `move_op` so tests can
//! force the cross-device fallback without two real devices.

use camino::Utf8Path;
use mx_core::errors::FsError;

pub trait MoveBackend: Send + Sync {
    /// Try a same-device rename. Returns `FsError::CrossDevice` to signal
    /// the caller should fall back to copy + delete.
    ///
    /// # Errors
    ///
    /// Returns `FsError::CrossDevice` for `EXDEV`, or another `FsError` for
    /// any other failure.
    fn rename(&self, from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError>;
}

pub struct LocalBackend;

/// `EXDEV` on Linux / macOS / BSD. `errno` 18 on those platforms.
#[cfg(unix)]
const EXDEV: i32 = 18;

impl MoveBackend for LocalBackend {
    fn rename(&self, from: &Utf8Path, to: &Utf8Path) -> Result<(), FsError> {
        match std::fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(e) => {
                #[cfg(unix)]
                if e.raw_os_error() == Some(EXDEV) {
                    return Err(FsError::CrossDevice);
                }
                Err(crate::dir_scan::map_io_pub(&e))
            }
        }
    }
}
