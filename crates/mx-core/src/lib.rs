//! Midnight X core: data types, pure `update()` and the public type vocabulary
//! shared across crates. No terminal, no I/O, no threads.

#![forbid(unsafe_code)]

pub mod command;
pub mod config;
pub mod errors;
pub mod event;
pub mod input;
pub mod keymap;
pub mod state;
pub mod theme;
pub mod update;

pub use update::update;
