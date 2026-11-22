//! Hotkey strings such as "Ctrl+Alt+O", parsed into a platform-independent form.
//! A hotkey must include at least one modifier so it can't swallow normal typing.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// 'A'..='Z' or '0'..='9'.
    Char(char),
    /// F1..=F24.
    F(u8),
    Space,
    Insert,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: Key,
}

impl Hotkey {
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut hk = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key: Key::Space,
        };
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            let lower = part.to_ascii_lowercase();
            match lower.as_str() {
                "" => return Err(format!("empty part in hotkey {s:?}")),
                "ctrl" | "control" => hk.ctrl = true,
                "alt" => hk.alt = true,
                "shift" => hk.shift = true,
                "win" | "super" | "meta" => hk.win = true,
                _ => {
                    if key.replace(parse_key(&lower)?).is_some() {
                        return Err(format!("more than one non-modifier key in {s:?}"));
                    }
                }
            }
        }
        hk.key = key.ok_or_else(|| format!("no key in hotkey {s:?}"))?;
        if !(hk.ctrl || hk.alt || hk.shift || hk.win) {
            return Err(format!("hotkey {s:?} needs at least one modifier"));
        }
        Ok(hk)
    }
}

fn parse_key(lower: &str) -> Result<Key, String> {
    let mut chars = lower.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Ok(Key::Char(c.to_ascii_uppercase()));
        }
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=24).contains(&n) {
            return Ok(Key::F(n));
        }
    }
    match lower {
        "space" => Ok(Key::Space),
        "insert" | "ins" => Ok(Key::Insert),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" | "pgup" => Ok(Key::PageUp),
        "pagedown" | "pgdn" => Ok(Key::PageDown),
        _ => Err(format!("unknown key {lower:?}")),
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        match self.key {
            Key::Char(c) => write!(f, "{c}"),
            Key::F(n) => write!(f, "F{n}"),
            Key::Space => write!(f, "Space"),
            Key::Insert => write!(f, "Insert"),
            Key::Delete => write!(f, "Delete"),
            Key::Home => write!(f, "Home"),
            Key::End => write!(f, "End"),
            Key::PageUp => write!(f, "PageUp"),
            Key::PageDown => write!(f, "PageDown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_letter() {
        let h = Hotkey::parse("ctrl + Alt+o").unwrap();
        assert!(h.ctrl && h.alt && !h.shift && !h.win);
        assert_eq!(h.key, Key::Char('O'));
    }

    #[test]
    fn parses_function_and_named_keys() {
        assert_eq!(Hotkey::parse("Shift+F12").unwrap().key, Key::F(12));
        assert_eq!(Hotkey::parse("Win+PageUp").unwrap().key, Key::PageUp);
    }

    #[test]
    fn display_round_trips() {
        for s in ["Ctrl+Alt+O", "Shift+F9", "Ctrl+Shift+Space", "Win+7"] {
            assert_eq!(Hotkey::parse(s).unwrap().to_string(), s);
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert!(Hotkey::parse("O").is_err(), "no modifier");
        assert!(Hotkey::parse("Ctrl").is_err(), "no key");
        assert!(Hotkey::parse("Ctrl+A+B").is_err(), "two keys");
        assert!(Hotkey::parse("Ctrl+F25").is_err(), "F25 out of range");
        assert!(Hotkey::parse("Ctrl++A").is_err(), "empty part");
        assert!(Hotkey::parse("").is_err());
    }
}
