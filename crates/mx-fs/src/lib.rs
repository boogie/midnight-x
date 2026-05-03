//! Midnight X filesystem layer. Owns the `Executor` (worker threads + the
//! event channel back to the main loop) and the synchronous `dir_scan`
//! helper. The only crate that calls `std::fs` or spawns worker threads.

#![forbid(unsafe_code)]

pub mod dir_scan;
pub mod executor;
pub mod format;
pub mod ops;

pub use dir_scan::scan;
pub use format::format_size;
pub use ops::{mkdir, rename};
