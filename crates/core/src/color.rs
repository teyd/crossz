use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Straight (non-premultiplied) sRGB color, written as `#rrggbb` or `#rrggbbaa` in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const GREEN: Color = Color::rgb(0, 255, 0);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid color {0:?}, expected #rrggbb or #rrggbbaa")]
pub struct ParseColorError(String);

impl FromStr for Color {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_owned());
        let hex = s.strip_prefix('#').ok_or_else(err)?;
        if !matches!(hex.len(), 6 | 8) || !hex.is_ascii() {
            return Err(err());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        Ok(Color {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

impl TryFrom<String> for Color {
    type Error = ParseColorError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Color> for String {
    fn from(c: Color) -> String {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for s in ["#00ff00", "#12345678", "#ffffff"] {
            assert_eq!(s.parse::<Color>().unwrap().to_string(), s);
        }
    }

    #[test]
    fn opaque_alpha_is_omitted() {
        assert_eq!("#00ff00ff".parse::<Color>().unwrap().to_string(), "#00ff00");
    }

    #[test]
    fn rejects_garbage() {
        for s in ["00ff00", "#00ff0", "#gg0000", "#00ff00f", "#ää00"] {
            assert!(s.parse::<Color>().is_err(), "{s}");
        }
    }
}
