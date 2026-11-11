use serde::{Deserialize, Serialize};

use crate::{Color, Snap};

/// A crosshair design, measured in game pixels: the game's resolution decides how big
/// it ends up on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crosshair {
    pub name: String,
    pub color: Color,
    #[serde(default = "full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub snap: Snap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arms: Option<Arms>,
    /// A second set of lines further out, like Valorant's outer lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer: Option<Arms>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dot: Option<Dot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle: Option<Circle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
}

/// Four lines around the center. `gap` is measured from the edge of the center square,
/// so arms stay symmetric whatever the thickness; a negative gap overlaps it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Arms {
    pub length: f32,
    pub thickness: f32,
    pub gap: f32,
    #[serde(default)]
    pub t_style: bool,
    #[serde(default = "full_opacity", skip_serializing_if = "is_full")]
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dot {
    pub size: f32,
    #[serde(default = "full_opacity", skip_serializing_if = "is_full")]
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Circle {
    pub radius: f32,
    pub thickness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outline {
    pub thickness: f32,
    pub color: Color,
}

fn full_opacity() -> f32 {
    1.0
}

fn is_full(opacity: &f32) -> bool {
    *opacity == 1.0
}

impl Arms {
    pub const DEFAULT: Arms = Arms {
        length: 5.0,
        thickness: 2.0,
        gap: 3.0,
        t_style: false,
        opacity: 1.0,
    };
}

impl Dot {
    pub const DEFAULT: Dot = Dot {
        size: 2.0,
        opacity: 1.0,
    };
}

impl Circle {
    pub const DEFAULT: Circle = Circle {
        radius: 8.0,
        thickness: 1.0,
    };
}

impl Outline {
    pub const DEFAULT: Outline = Outline {
        thickness: 1.0,
        color: Color::BLACK,
    };
}

impl Crosshair {
    /// An empty design: no parts, white, outlined parts once added.
    pub fn blank(name: impl Into<String>) -> Crosshair {
        Crosshair {
            name: name.into(),
            color: Color::WHITE,
            opacity: 1.0,
            snap: Snap::Round,
            arms: None,
            outer: None,
            dot: None,
            circle: None,
            outline: Some(Outline::DEFAULT),
        }
    }

    /// Furthest distance from the center any part reaches, in game pixels.
    pub fn reach(&self) -> f32 {
        let lines = |arms: Option<Arms>| {
            arms.map_or(0.0, |a| a.thickness / 2.0 + a.gap.max(0.0) + a.length)
        };
        let arms = lines(self.arms).max(lines(self.outer));
        let dot = self.dot.map_or(0.0, |d| d.size / 2.0);
        let circle = self.circle.map_or(0.0, |c| c.radius + c.thickness / 2.0);
        let outline = self.outline.map_or(0.0, |o| o.thickness);
        arms.max(dot).max(circle) + outline
    }
}
