use serde::{Deserialize, Serialize};

/// How a profile maps its virtual resolution onto the game's real resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScaleMode {
    /// Crosshair pixels are screen pixels.
    #[default]
    Native,
    /// Independent X/Y scale, like a 4:3 game stretched over a 16:9 screen.
    Stretched { width: u32, height: u32 },
    /// One scale taken from the height, like a 4:3 game with black bars.
    Uniform { height: u32 },
}

pub const PRESETS: &[(u32, u32)] = &[
    (1280, 960),
    (1440, 1080),
    (1024, 768),
    (1280, 1024),
    (1600, 1200),
    (1920, 1440),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub x: f32,
    pub y: f32,
}

impl Scale {
    pub const ONE: Scale = Scale { x: 1.0, y: 1.0 };
}

impl ScaleMode {
    pub fn scale(self, screen_w: u32, screen_h: u32) -> Scale {
        let ratio = |screen: u32, virt: u32| screen as f32 / virt.max(1) as f32;
        match self {
            ScaleMode::Native => Scale::ONE,
            ScaleMode::Stretched { width, height } => Scale {
                x: ratio(screen_w, width),
                y: ratio(screen_h, height),
            },
            ScaleMode::Uniform { height } => {
                let s = ratio(screen_h, height);
                Scale { x: s, y: s }
            }
        }
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
        let mode = ScaleMode::Stretched {
            width: 1280,
            height: 960,
        };
        assert_eq!(mode.scale(2560, 1440), Scale { x: 2.0, y: 1.5 });
    }

    #[test]
    fn uniform_uses_height() {
        let mode = ScaleMode::Uniform { height: 960 };
        assert_eq!(mode.scale(2560, 1440), Scale { x: 1.5, y: 1.5 });
    }

    #[test]
    fn native_ignores_screen() {
        assert_eq!(ScaleMode::Native.scale(2560, 1440), Scale::ONE);
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
