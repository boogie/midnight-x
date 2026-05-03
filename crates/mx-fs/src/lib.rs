//! Midnight X filesystem layer. Owns the `Executor` (worker threads + the
//! event channel back to the main loop) and the synchronous `dir_scan`
//! helper. The only crate that calls `std::fs` or spawns worker threads.

#![forbid(unsafe_code)]

pub mod copy;
pub mod delete;
pub mod dir_scan;
pub mod executor;
pub mod format;
pub mod move_backend;
pub mod move_op;
pub mod ops;

pub use copy::{copy_file, copy_tree, CopyReport, OverwriteAction};
pub use delete::{delete_tree, DeleteReport};
pub use dir_scan::scan;
pub use format::format_size;
pub use move_backend::{LocalBackend, MoveBackend};
pub use move_op::{move_tree, MoveReport};
pub use ops::{mkdir, rename};
