//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the `crossterm` → `mx_core::InputEvent`
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
