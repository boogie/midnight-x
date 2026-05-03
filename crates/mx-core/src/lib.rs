//! Midnight X core: data types, pure `update()` and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod input;
pub mod command;
pub mod errors;
pub mod event;
pub mod theme;
pub mod state;
