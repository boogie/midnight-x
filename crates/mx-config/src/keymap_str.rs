//! Parser for the `"ctrl-shift-pgup"` / `"esc 1"` key-string grammar used by
//! `[keymap]` TOML tables.

use mx_core::input::{KeyChord, KeyCode, KeyModifiers};

/// Parse a single key chord like `"ctrl-shift-f5"` or `"a"`.
///
/// # Errors
///
/// Returns a human-readable error message when the input is empty, contains
/// an unknown modifier, or names an unknown key.
pub fn parse_chord(s: &str) -> Result<KeyChord, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty chord".into()); }

    let mut mods = KeyModifiers::NONE;
    let mut last: Option<&str> = None;
    for part in s.split('-') {
        let lower = part.to_ascii_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => mods.ctrl  = true,
            "shift"            => mods.shift = true,
            "alt" | "meta"     => mods.alt   = true,
            _ => {
                if last.is_some() {
                    return Err(format!("invalid modifier: {part}"));
                }
                last = Some(part);
            }
        }
    }
    let key = last.ok_or_else(|| format!("no key after modifiers: {s}"))?;
    let code = parse_keycode(key)?;
    Ok(KeyChord::new(code, mods))
}

/// Parse a sequence like `"esc 1"` or `"ctrl-r r"` (space-separated chords).
///
/// # Errors
///
/// Returns the first chord-parse error encountered, or an error when the
/// sequence is empty.
pub fn parse_sequence(s: &str) -> Result<Vec<KeyChord>, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty sequence".into()); }
    s.split_whitespace().map(parse_chord).collect()
}

fn parse_keycode(s: &str) -> Result<KeyCode, String> {
    let lower = s.to_ascii_lowercase();
    Ok(match lower.as_str() {
        "esc" | "escape"             => KeyCode::Esc,
        "tab"                        => KeyCode::Tab,
        "backtab"                    => KeyCode::BackTab,
        "enter" | "return"           => KeyCode::Enter,
        "backspace" | "bs"           => KeyCode::Backspace,
        "del" | "delete"             => KeyCode::Delete,
        "ins" | "insert"             => KeyCode::Insert,
        "home"                       => KeyCode::Home,
        "end"                        => KeyCode::End,
        "pgup" | "pageup"            => KeyCode::PageUp,
        "pgdn" | "pgdown" | "pagedown" => KeyCode::PageDown,
        "up"                         => KeyCode::Up,
        "down"                       => KeyCode::Down,
        "left"                       => KeyCode::Left,
        "right"                      => KeyCode::Right,
        "space"                      => KeyCode::Char(' '),
        "null"                       => KeyCode::Null,
        other if other.starts_with('f')
              && other.len() > 1
              && other[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u8 = other[1..].parse().map_err(|_| format!("bad fn key: {s}"))?;
            if !(1..=24).contains(&n) {
                return Err(format!("F-key out of range: {s}"));
            }
            KeyCode::F(n)
        }
        other if other.chars().count() == 1 => {
            KeyCode::Char(other.chars().next().expect("len 1"))
        }
        _ => return Err(format!("unknown key: {s}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_char() {
        assert_eq!(parse_chord("a").unwrap(),
            KeyChord::new(KeyCode::Char('a'), KeyModifiers::NONE));
    }

    #[test]
    fn ctrl_q_lowercase() {
        let c = parse_chord("ctrl-q").unwrap();
        assert!(c.mods.ctrl && !c.mods.shift && !c.mods.alt);
        assert_eq!(c.code, KeyCode::Char('q'));
    }

    #[test]
    fn ctrl_shift_pgup() {
        let c = parse_chord("ctrl-shift-pgup").unwrap();
        assert!(c.mods.ctrl && c.mods.shift);
        assert_eq!(c.code, KeyCode::PageUp);
    }

    #[test]
    fn function_key() {
        assert_eq!(parse_chord("f5").unwrap().code, KeyCode::F(5));
        assert_eq!(parse_chord("F12").unwrap().code, KeyCode::F(12));
    }

    #[test]
    fn esc_alias() {
        assert_eq!(parse_chord("esc").unwrap().code,    KeyCode::Esc);
        assert_eq!(parse_chord("escape").unwrap().code, KeyCode::Esc);
    }

    #[test]
    fn rejects_bad_key() {
        assert!(parse_chord("ctrl-asdf").is_err());
        assert!(parse_chord("").is_err());
        assert!(parse_chord("ctrl-shift-").is_err());
    }

    #[test]
    fn parses_sequence_with_spaces() {
        let seq = parse_sequence("esc 1").unwrap();
        assert_eq!(seq.len(), 2);
        assert_eq!(seq[0].code, KeyCode::Esc);
        assert_eq!(seq[1].code, KeyCode::Char('1'));
    }

    #[test]
    fn parses_multi_chord_sequence() {
        let seq = parse_sequence("ctrl-r r").unwrap();
        assert_eq!(seq.len(), 2);
        assert!(seq[0].mods.ctrl);
        assert_eq!(seq[1], KeyChord::new(KeyCode::Char('r'), KeyModifiers::NONE));
    }
}
