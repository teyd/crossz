use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A keyboard key or mouse button, as a Windows virtual-key code. Mouse buttons have VK
/// codes too, which lets one type cover both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(pub u16);

impl Key {
    pub const MOUSE1: Key = Key(0x01);
    pub const MOUSE2: Key = Key(0x02);
    pub const MOUSE3: Key = Key(0x04);
    pub const MOUSE4: Key = Key(0x05);
    pub const MOUSE5: Key = Key(0x06);
    pub const SHIFT: Key = Key(0x10);
    pub const CTRL: Key = Key(0x11);
    pub const ALT: Key = Key(0x12);
    pub const WIN: Key = Key(0x5B);

    /// Folds left/right modifier variants into one key, so "Ctrl" matches either Ctrl.
    pub fn normalized(self) -> Key {
        match self.0 {
            0xA0 | 0xA1 => Key::SHIFT,
            0xA2 | 0xA3 => Key::CTRL,
            0xA4 | 0xA5 => Key::ALT,
            0x5C => Key::WIN,
            _ => self,
        }
    }

    pub fn name(self) -> Option<String> {
        match self.0 {
            0x30..=0x39 | 0x41..=0x5A => Some(char::from(self.0 as u8).to_string()),
            0x70..=0x87 => Some(format!("F{}", self.0 - 0x6F)),
            code => NAMED
                .iter()
                .find(|&&(_, c)| c == code)
                .map(|&(n, _)| n.to_owned()),
        }
    }

    pub fn from_name(name: &str) -> Option<Key> {
        let upper = name.to_ascii_uppercase();
        if let [c] = upper.as_bytes()
            && c.is_ascii_alphanumeric()
        {
            return Some(Key(u16::from(*c)));
        }
        if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u16>().ok())
            && (1..=24).contains(&n)
        {
            return Some(Key(0x6F + n));
        }
        if let Some(hex) = upper.strip_prefix("VK0X") {
            return u16::from_str_radix(hex, 16).ok().map(Key);
        }
        NAMED
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|&(_, code)| Key(code))
    }
}

const NAMED: &[(&str, u16)] = &[
    ("Mouse1", 0x01),
    ("Mouse2", 0x02),
    ("Mouse3", 0x04),
    ("Mouse4", 0x05),
    ("Mouse5", 0x06),
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Shift", 0x10),
    ("Ctrl", 0x11),
    ("Alt", 0x12),
    ("Pause", 0x13),
    ("CapsLock", 0x14),
    ("Escape", 0x1B),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("PrintScreen", 0x2C),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Win", 0x5B),
    ("Num0", 0x60),
    ("Num1", 0x61),
    ("Num2", 0x62),
    ("Num3", 0x63),
    ("Num4", 0x64),
    ("Num5", 0x65),
    ("Num6", 0x66),
    ("Num7", 0x67),
    ("Num8", 0x68),
    ("Num9", 0x69),
    ("NumMultiply", 0x6A),
    ("NumAdd", 0x6B),
    ("NumSubtract", 0x6D),
    ("NumDecimal", 0x6E),
    ("NumDivide", 0x6F),
    ("NumLock", 0x90),
    ("ScrollLock", 0x91),
    (";", 0xBA),
    ("=", 0xBB),
    (",", 0xBC),
    ("-", 0xBD),
    (".", 0xBE),
    ("/", 0xBF),
    ("`", 0xC0),
    ("[", 0xDB),
    ("\\", 0xDC),
    ("]", 0xDD),
    ("'", 0xDE),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Mods {
    fn as_keys(self) -> impl Iterator<Item = Key> {
        [
            (self.ctrl, Key::CTRL),
            (self.shift, Key::SHIFT),
            (self.alt, Key::ALT),
            (self.win, Key::WIN),
        ]
        .into_iter()
        .filter_map(|(on, key)| on.then_some(key))
    }
}

/// Modifiers plus one key, written like `Ctrl+Alt+X` or `Mouse2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Chord {
    pub mods: Mods,
    pub key: Key,
}

impl Chord {
    pub const fn key(key: Key) -> Chord {
        Chord {
            mods: Mods {
                ctrl: false,
                shift: false,
                alt: false,
                win: false,
            },
            key,
        }
    }

    /// Whether the required modifiers are held. Extra modifiers don't prevent a match, so
    /// holding Shift to walk doesn't break a `Mouse2` binding.
    pub fn mods_held(&self, held: impl Fn(Key) -> bool) -> bool {
        self.mods.as_keys().all(held)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown key {0:?}")]
pub struct ParseChordError(String);

impl FromStr for Chord {
    type Err = ParseChordError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseChordError(s.to_owned());
        // "Ctrl++" binds the plus key; split from the right so the key part keeps its '+'.
        let (mods_part, key_part) = match s.strip_suffix("++") {
            Some(rest) => (rest, "+"),
            None => s.rsplit_once('+').unwrap_or(("", s)),
        };
        let key = if key_part == "+" {
            Key(0xBB)
        } else {
            Key::from_name(key_part.trim()).ok_or_else(err)?
        };
        let mut mods = Mods::default();
        for m in mods_part
            .split('+')
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" => mods.alt = true,
                "win" | "super" => mods.win = true,
                _ => return Err(err()),
            }
        }
        Ok(Chord { mods, key })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for key in self.mods.as_keys() {
            write!(f, "{}+", key.name().expect("modifiers are named"))?;
        }
        match self.key.name() {
            Some(name) => f.write_str(&name),
            None => write!(f, "VK{:#04X}", self.key.0),
        }
    }
}

impl TryFrom<String> for Chord {
    type Error = ParseChordError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Chord> for String {
    fn from(c: Chord) -> String {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_prints() {
        for s in [
            "Ctrl+Alt+X",
            "Mouse2",
            "F13",
            "Shift+Mouse5",
            "Ctrl+Num0",
            "Alt+]",
            "VK0xE9",
        ] {
            assert_eq!(s.parse::<Chord>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn is_case_insensitive() {
        let c: Chord = "ctrl+alt+x".parse().unwrap();
        assert_eq!(c.to_string(), "Ctrl+Alt+X");
        assert_eq!(c.key, Key(0x58));
    }

    #[test]
    fn rejects_unknown() {
        assert!("Hyper+X".parse::<Chord>().is_err());
        assert!("Mouse9".parse::<Chord>().is_err());
        assert!("F25".parse::<Chord>().is_err());
    }

    #[test]
    fn plus_key() {
        let c: Chord = "Ctrl++".parse().unwrap();
        assert!(c.mods.ctrl);
        assert_eq!(c.key, Key(0xBB));
    }

    #[test]
    fn folds_left_right_modifiers() {
        assert_eq!(Key(0xA3).normalized(), Key::CTRL);
        assert_eq!(Key(0x5C).normalized(), Key::WIN);
        assert_eq!(Key::MOUSE4.normalized(), Key::MOUSE4);
    }
}
