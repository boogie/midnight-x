//! Translate `crossterm::event::Event` → `mx_core::input::InputEvent`. This
//! is the only place where crossterm's input vocabulary touches the rest of
//! the workspace.

use crossterm::event as ct;
use mx_core::input::{InputEvent, KeyChord, KeyCode, KeyModifiers};

/// Returns `None` for events `mx-core` doesn't model in Phase 1
/// (resize is handled by the input thread separately; mouse is reserved).
#[must_use]
pub fn translate(ev: ct::Event) -> Option<InputEvent> {
    match ev {
        ct::Event::Key(k) if k.kind == ct::KeyEventKind::Press => {
            Some(InputEvent::Key(translate_key(k)))
        }
        ct::Event::Paste(s) => Some(InputEvent::Paste(s)),
        ct::Event::Mouse(_) => Some(InputEvent::MouseStub),
        _ => None,
    }
}

fn translate_key(k: ct::KeyEvent) -> KeyChord {
    #[allow(clippy::match_same_arms)]  // wildcard collapses unmodeled keys to Null
    let code = match k.code {
        ct::KeyCode::Char(c)   => KeyCode::Char(c),
        ct::KeyCode::Enter     => KeyCode::Enter,
        ct::KeyCode::Esc       => KeyCode::Esc,
        ct::KeyCode::Tab       => KeyCode::Tab,
        ct::KeyCode::BackTab   => KeyCode::BackTab,
        ct::KeyCode::Backspace => KeyCode::Backspace,
        ct::KeyCode::Delete    => KeyCode::Delete,
        ct::KeyCode::Insert    => KeyCode::Insert,
        ct::KeyCode::Home      => KeyCode::Home,
        ct::KeyCode::End       => KeyCode::End,
        ct::KeyCode::PageUp    => KeyCode::PageUp,
        ct::KeyCode::PageDown  => KeyCode::PageDown,
        ct::KeyCode::Up        => KeyCode::Up,
        ct::KeyCode::Down      => KeyCode::Down,
        ct::KeyCode::Left      => KeyCode::Left,
        ct::KeyCode::Right     => KeyCode::Right,
        ct::KeyCode::F(n)      => KeyCode::F(n),
        // `Null` is its own variant; CapsLock, MediaKey, etc. fall into the
        // wildcard. Both flatten to `KeyCode::Null` in Phase 1.
        _                      => KeyCode::Null,
    };
    let mods = KeyModifiers {
        ctrl:  k.modifiers.contains(ct::KeyModifiers::CONTROL),
        shift: k.modifiers.contains(ct::KeyModifiers::SHIFT),
        alt:   k.modifiers.contains(ct::KeyModifiers::ALT),
    };
    KeyChord { code, mods }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event as ct;

    #[test]
    fn ctrl_q_translates() {
        let ev = ct::Event::Key(ct::KeyEvent::new(
            ct::KeyCode::Char('q'),
            ct::KeyModifiers::CONTROL,
        ));
        match translate(ev).unwrap() {
            InputEvent::Key(c) => {
                assert_eq!(c.code, KeyCode::Char('q'));
                assert!(c.mods.ctrl);
            }
            _ => panic!("expected Key"),
        }
    }

    #[test]
    fn release_events_are_dropped() {
        let mut k = ct::KeyEvent::new(ct::KeyCode::Esc, ct::KeyModifiers::NONE);
        k.kind = ct::KeyEventKind::Release;
        assert!(translate(ct::Event::Key(k)).is_none());
    }

    #[test]
    fn f5_translates() {
        let ev = ct::Event::Key(ct::KeyEvent::new(
            ct::KeyCode::F(5),
            ct::KeyModifiers::NONE,
        ));
        match translate(ev).unwrap() {
            InputEvent::Key(c) => assert_eq!(c.code, KeyCode::F(5)),
            _ => panic!(),
        }
    }
}
