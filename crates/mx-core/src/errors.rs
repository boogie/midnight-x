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
    Fs {
        path: Utf8PathBuf,
        #[source]
        source: FsError,
    },
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
        Self {
            key: key.into(),
            message: message.into(),
        }
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
        let e = AppError::Fs {
            path: p.clone(),
            source: FsError::PermissionDenied,
        };
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
