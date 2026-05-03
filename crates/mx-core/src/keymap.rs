//! Prefix-aware keymap. A binding is a non-empty `Vec<KeyChord>` (a sequence
//! of one or more keystrokes) mapped to a `CommandId`. Lookup tells the
//! caller whether a sequence is a complete `Match`, a `Prefix` of a longer
//! binding (caller should keep waiting), or `NoMatch`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::command::CommandId;
use crate::input::{KeyChord, KeyCode, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    Match(CommandId),
    Prefix,
    NoMatch,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Keymap {
    /// Internal storage. Both fields stay in sync; only `bindings` is
    /// (de)serialized.
    bindings: Vec<(Vec<KeyChord>, CommandId)>,
    #[serde(skip)]
    by_seq:   HashMap<Vec<KeyChord>, CommandId>,
}

impl Keymap {
    #[must_use]
    pub fn empty() -> Self { Self::default() }

    /// Build from a list of (sequence, command) pairs. Later entries override
    /// earlier ones with the same key sequence.
    #[must_use]
    pub fn from_bindings(bindings: Vec<(Vec<KeyChord>, CommandId)>) -> Self {
        let mut k = Self { bindings: Vec::new(), by_seq: HashMap::new() };
        for (seq, cmd) in bindings {
            k.set(seq, cmd);
        }
        k
    }

    pub fn set(&mut self, seq: Vec<KeyChord>, cmd: CommandId) {
        // Remove any prior binding for the same sequence.
        self.bindings.retain(|(s, _)| s != &seq);
        self.bindings.push((seq.clone(), cmd));
        self.by_seq.insert(seq, cmd);
    }

    pub fn unbind(&mut self, seq: &[KeyChord]) {
        self.bindings.retain(|(s, _)| s.as_slice() != seq);
        self.by_seq.remove(seq);
    }

    #[must_use]
    pub fn lookup(&self, seq: &[KeyChord]) -> Lookup {
        if let Some(cmd) = self.by_seq.get(seq) {
            return Lookup::Match(*cmd);
        }
        if self.is_strict_prefix(seq) {
            Lookup::Prefix
        } else {
            Lookup::NoMatch
        }
    }

    /// Returns `true` iff some binding is strictly longer than `seq` and
    /// starts with `seq`. The chord engine in `update()` uses this to decide
    /// whether to wait when an exact match is *also* a prefix of a longer
    /// chord (e.g. `Esc` is a complete `Cancel` binding *and* a prefix of
    /// `Esc 1` etc.).
    #[must_use]
    pub fn is_strict_prefix(&self, seq: &[KeyChord]) -> bool {
        self.bindings
            .iter()
            .any(|(s, _)| s.len() > seq.len() && s.starts_with(seq))
    }

    /// MC-faithful default keymap (spec §8). Phase 1 ships this verbatim;
    /// many of these resolve to `CommandId`s that `update()` does not yet
    /// handle, but they parse and dispatch as Phase 2 picks them up.
    #[must_use]
    pub fn defaults() -> Self {
        use CommandId::{
            Cancel, Copy, CursorDown, CursorEnd, CursorHome, CursorPageDown, CursorPageUp,
            CursorUp, CycleSort, Delete, EnterDir, FocusOther, Help, InvertSelection, Mkdir,
            Move, ParentDir, QuitConfirm, Rename, RescanFocused, SelectAll, SwapPanels,
            ToggleHidden, ToggleSelect, View,
        };
        use KeyCode::{
            Backspace, Char, Delete as DelKey, Down, End, Enter, Esc, F, Home, Insert, PageDown,
            PageUp, Tab, Up,
        };

        let none = KeyModifiers::NONE;
        let ctrl = KeyModifiers::ctrl();
        let shift = KeyModifiers::shift();
        let kc = |code| KeyChord::new(code, none);
        let ck = |code| KeyChord::new(code, ctrl);
        let sk = |code| KeyChord::new(code, shift);

        let bindings: Vec<(Vec<KeyChord>, CommandId)> = vec![
            // Navigation
            (vec![kc(Up)],          CursorUp),
            (vec![kc(Down)],        CursorDown),
            (vec![kc(PageUp)],      CursorPageUp),
            (vec![kc(PageDown)],    CursorPageDown),
            (vec![kc(Home)],        CursorHome),
            (vec![kc(End)],         CursorEnd),
            (vec![kc(Enter)],       EnterDir),
            (vec![kc(Backspace)],   ParentDir),
            (vec![kc(Tab)],         FocusOther),
            (vec![ck(Char('u'))],   SwapPanels),
            // Selection
            (vec![kc(Insert)],      ToggleSelect),
            (vec![kc(Char('*'))],   InvertSelection),
            (vec![ck(Char('a'))],   SelectAll),
            // F-keys
            (vec![kc(F(1))],        Help),
            (vec![kc(F(3))],        View),
            (vec![kc(F(5))],        Copy),
            (vec![kc(F(6))],        Move),
            (vec![kc(F(7))],        Mkdir),
            (vec![kc(F(8))],        Delete),
            (vec![kc(DelKey)],      Delete),
            (vec![sk(F(6))],        Rename),
            (vec![ck(Char('t'))],   Rename),
            (vec![kc(F(10))],       QuitConfirm),
            (vec![ck(Char('q'))],   QuitConfirm),
            // Refresh / view toggles
            (vec![ck(Char('r'))],   RescanFocused),
            (vec![ck(Char('h'))],   ToggleHidden),
            (vec![ck(Char('s'))],   CycleSort),
            // Esc alone — Cancel
            (vec![kc(Esc)],         Cancel),
            // Esc-prefix chords (MC alt-meta style)
            (vec![kc(Esc), kc(Char('1'))], Help),
            (vec![kc(Esc), kc(Char('3'))], View),
            (vec![kc(Esc), kc(Char('5'))], Copy),
            (vec![kc(Esc), kc(Char('6'))], Move),
            (vec![kc(Esc), kc(Char('7'))], Mkdir),
            (vec![kc(Esc), kc(Char('8'))], Delete),
            (vec![kc(Esc), kc(Char('0'))], QuitConfirm),
        ];
        Keymap::from_bindings(bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandId;
    use crate::input::{KeyChord, KeyCode};

    fn kc(c: KeyCode) -> KeyChord { KeyChord::bare(c) }

    #[test]
    fn match_returns_command_id() {
        let k = Keymap::defaults();
        assert_eq!(k.lookup(&[kc(KeyCode::F(5))]), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn esc_alone_is_cancel_and_also_a_prefix() {
        let k = Keymap::defaults();
        // Esc alone is a complete binding (Cancel)…
        assert_eq!(k.lookup(&[kc(KeyCode::Esc)]), Lookup::Match(CommandId::Cancel));
        // …but it's also a prefix of Esc-N chords. The chord-engine in
        // `update()` is what disambiguates via timeout; the keymap just
        // reports what it knows.
        assert!(k.is_strict_prefix(&[kc(KeyCode::Esc)]));
    }

    #[test]
    fn unknown_sequence_is_no_match() {
        let k = Keymap::defaults();
        assert_eq!(k.lookup(&[kc(KeyCode::F(11))]), Lookup::NoMatch);
    }

    #[test]
    fn unbind_removes_default() {
        let mut k = Keymap::defaults();
        k.unbind(&[kc(KeyCode::F(5))]);
        assert_eq!(k.lookup(&[kc(KeyCode::F(5))]), Lookup::NoMatch);
    }

    #[test]
    fn esc_prefix_chord_resolves() {
        let k = Keymap::defaults();
        let seq = vec![kc(KeyCode::Esc), kc(KeyCode::Char('5'))];
        assert_eq!(k.lookup(&seq), Lookup::Match(CommandId::Copy));
    }

    #[test]
    fn longer_binding_makes_shorter_prefix_visible() {
        // Build a keymap where 'a' is a prefix of 'a b'.
        use crate::input::KeyModifiers;
        let a = KeyChord::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let b = KeyChord::new(KeyCode::Char('b'), KeyModifiers::NONE);
        let k = Keymap::from_bindings(vec![
            (vec![a, b], CommandId::Help),
        ]);
        assert_eq!(k.lookup(&[a]), Lookup::Prefix);
        assert_eq!(k.lookup(&[a, b]), Lookup::Match(CommandId::Help));
    }
}
