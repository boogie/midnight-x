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
    pub body: String,
    pub truncated: bool,
    pub binary: bool,
}

pub struct PlainTextPreviewer {
    pub max_bytes: usize,
}

impl Default for PlainTextPreviewer {
    fn default() -> Self {
        Self {
            max_bytes: 1024 * 1024,
        }
    }
}

impl Previewer for PlainTextPreviewer {
    fn preview(&self, path: &Utf8Path) -> Result<Preview, FsError> {
        let mut f = File::open(path).map_err(|e| map_io(&e))?;
        let mut buf = Vec::with_capacity(self.max_bytes.min(64 * 1024));
        let mut chunk = [0u8; 8192];
        let mut truncated = false;
        loop {
            let n = f.read(&mut chunk).map_err(|e| map_io(&e))?;
            if n == 0 {
                break;
            }
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
        Ok(Preview {
            body,
            truncated,
            binary,
        })
    }
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8 * 1024).any(|&b| b == 0)
}

fn map_io(e: &std::io::Error) -> FsError {
    use std::io::ErrorKind as K;
    match e.kind() {
        K::NotFound => FsError::NotFound,
        K::PermissionDenied => FsError::PermissionDenied,
        K::IsADirectory => FsError::IsADirectory,
        _ => FsError::Io(e.to_string()),
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
            pv.preview(&Utf8PathBuf::from("/no/such/file/mx-preview-test"))
                .unwrap_err(),
            FsError::NotFound,
        );
    }
}
