use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The resolution a crosshair is drawn for, written `native` or like `1280x960` in
/// config. A lower one is stretched over the screen, like a stretched game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Resolution {
    #[default]
    Native,
    Custom {
        width: u32,
        height: u32,
    },
}

/// The resolutions offered in the UI; anything else can still be typed in.
pub const COMMON: &[Resolution] = &[
    Resolution::Native,
    Resolution::custom(1920, 1080),
    Resolution::custom(1440, 1080),
    Resolution::custom(1280, 960),
    Resolution::custom(1280, 1024),
    Resolution::custom(1024, 768),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub x: f32,
    pub y: f32,
}

impl Scale {
    pub const ONE: Scale = Scale { x: 1.0, y: 1.0 };
}

impl Resolution {
    pub const fn custom(width: u32, height: u32) -> Resolution {
        Resolution::Custom { width, height }
    }

    /// How much one crosshair pixel grows when this resolution is stretched over a
    /// `screen_w`×`screen_h` screen. Each axis scales on its own, so a 4:3 crosshair
    /// on a 16:9 screen gets wider, exactly like a stretched game.
    pub fn scale(self, screen_w: u32, screen_h: u32) -> Scale {
        let Resolution::Custom { width, height } = self else {
            return Scale::ONE;
        };
        let ratio = |screen: u32, virt: u32| screen as f32 / virt.max(1) as f32;
        Scale {
            x: ratio(screen_w, width),
            y: ratio(screen_h, height),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid resolution {0:?}, expected native or WIDTHxHEIGHT like 1280x960")]
pub struct ParseResolutionError(String);

impl FromStr for Resolution {
    type Err = ParseResolutionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseResolutionError(s.to_owned());
        if s.trim().eq_ignore_ascii_case("native") {
            return Ok(Resolution::Native);
        }
        let (w, h) = s.split_once(['x', 'X', '×']).ok_or_else(err)?;
        let width: u32 = w.trim().parse().map_err(|_| err())?;
        let height: u32 = h.trim().parse().map_err(|_| err())?;
        if width == 0 || height == 0 {
            return Err(err());
        }
        Ok(Resolution::custom(width, height))
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resolution::Native => f.write_str("native"),
            Resolution::Custom { width, height } => write!(f, "{width}x{height}"),
        }
    }
}

impl TryFrom<String> for Resolution {
    type Error = ParseResolutionError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl From<Resolution> for String {
    fn from(r: Resolution) -> String {
        r.to_string()
    }
}

/// How scaled sizes become whole screen pixels. Everything but `AntiAlias` keeps edges crisp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Snap {
    #[default]
    Round,
    Floor,
    Ceil,
    AntiAlias,
}

impl Snap {
    /// Snaps a size. Anything visible stays at least one pixel, otherwise a 1px line
    /// at 0.75× scale would vanish with `Floor`.
    pub fn size(self, v: f32) -> f32 {
        if v <= 0.0 {
            return 0.0;
        }
        let snapped = match self {
            Snap::Round => v.round(),
            Snap::Floor => v.floor(),
            Snap::Ceil => v.ceil(),
            Snap::AntiAlias => return v,
        };
        snapped.max(1.0)
    }

    /// Snaps a distance, which may be zero or negative (e.g. a negative gap).
    pub fn offset(self, v: f32) -> f32 {
        match self {
            Snap::Round => v.round(),
            Snap::Floor => v.floor(),
            Snap::Ceil => v.ceil(),
            Snap::AntiAlias => v,
        }
    }

    /// The `(start, end)` of a span of `size` centered on 0. Odd pixel sizes can't be
    /// centered on a pixel boundary, so the extra pixel goes to the positive side.
    pub fn centered(self, size: f32) -> (f32, f32) {
        if self == Snap::AntiAlias {
            return (-size / 2.0, size / 2.0);
        }
        let before = (size / 2.0).floor();
        (-before, size - before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretched_1280x960_on_1440p() {
        let r = Resolution::custom(1280, 960);
        assert_eq!(r.scale(2560, 1440), Scale { x: 2.0, y: 1.5 });
    }

    #[test]
    fn same_aspect_is_uniform() {
        let r = Resolution::custom(1280, 720);
        assert_eq!(r.scale(2560, 1440), Scale { x: 2.0, y: 2.0 });
    }

    #[test]
    fn native_ignores_screen() {
        assert_eq!(Resolution::Native.scale(2560, 1440), Scale::ONE);
    }

    #[test]
    fn parses_and_prints() {
        for s in ["native", "1280x960", "1920x1080"] {
            assert_eq!(s.parse::<Resolution>().unwrap().to_string(), s);
        }
        assert_eq!(
            "1280 × 960".parse::<Resolution>().unwrap(),
            Resolution::custom(1280, 960)
        );
        assert!("0x960".parse::<Resolution>().is_err());
        assert!("big".parse::<Resolution>().is_err());
    }

    #[test]
    fn snapping_keeps_thin_lines_visible() {
        assert_eq!(Snap::Floor.size(0.75), 1.0);
        assert_eq!(Snap::Round.size(1.5), 2.0);
        assert_eq!(Snap::Ceil.size(1.1), 2.0);
        assert_eq!(Snap::AntiAlias.size(1.5), 1.5);
        assert_eq!(Snap::Round.size(0.0), 0.0);
    }

    #[test]
    fn centered_spans() {
        assert_eq!(Snap::Round.centered(4.0), (-2.0, 2.0));
        assert_eq!(Snap::Round.centered(3.0), (-1.0, 2.0));
        assert_eq!(Snap::AntiAlias.centered(3.0), (-1.5, 1.5));
    }
}
