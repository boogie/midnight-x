//! Input vocabulary: terminal-free analogues of crossterm's `KeyCode` /
//! `KeyModifiers`, plus `KeyChord` (one keystroke) and `InputEvent` (anything
//! the input thread can deliver). Defining these here keeps `mx-core`
//! terminal-agnostic so `update()` can be tested without crossterm.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    F(u8),
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
    };

    #[must_use]
    pub const fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Self::NONE
        }
    }
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            ..Self::NONE
        }
    }
    #[must_use]
    pub const fn alt() -> Self {
        Self {
            alt: true,
            ..Self::NONE
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyChord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyChord {
    #[must_use]
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    /// Convenience: a chord with no modifiers.
    #[must_use]
    pub const fn bare(code: KeyCode) -> Self {
        Self {
            code,
            mods: KeyModifiers::NONE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyChord),
    Paste(String),
    /// Reserved for future mouse support.
    MouseStub,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_chord_constructors() {
        let bare = KeyChord::bare(KeyCode::Esc);
        assert_eq!(bare.code, KeyCode::Esc);
        assert!(!bare.mods.ctrl && !bare.mods.shift && !bare.mods.alt);

        let ctrl_q = KeyChord::new(KeyCode::Char('q'), KeyModifiers::ctrl());
        assert!(ctrl_q.mods.ctrl);
        assert_eq!(ctrl_q.code, KeyCode::Char('q'));
    }

    #[test]
    fn key_modifiers_default_is_none() {
        assert_eq!(KeyModifiers::default(), KeyModifiers::NONE);
    }

    #[test]
    fn key_chord_eq_and_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(KeyChord::bare(KeyCode::F(5)));
        assert!(set.contains(&KeyChord::bare(KeyCode::F(5))));
        assert!(!set.contains(&KeyChord::new(KeyCode::F(5), KeyModifiers::ctrl())));
    }
}
