//! Midnight X TUI: the only crate that talks to the terminal. Owns the
//! `Renderer`, the input thread, and the `crossterm` → `mx_core::InputEvent`
//! translation.

#![forbid(unsafe_code)]

pub mod input_xlate;
pub mod layout;
pub mod theme_styles;
pub mod view;
