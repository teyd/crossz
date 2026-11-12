use serde::{Deserialize, Serialize};

use crate::{Color, Pixels, Snap};

/// A crosshair: either a shape built from lines, a dot and a circle (the built-in
/// ones), or a pixel drawing. Sizes are in game pixels: the game's "stretch like"
/// resolution and `zoom` decide how big it ends up on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crosshair {
    pub name: String,
    /// Extra size multiplier on top of the game's stretch.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub zoom: f32,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// Takes precedence over `shape` when both are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixels: Option<Pixels>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub color: Color,
    #[serde(default)]
    pub snap: Snap,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arms: Option<Arms>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dot {
    pub size: f32,
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

fn one() -> f32 {
    1.0
}

fn is_one(v: &f32) -> bool {
    *v == 1.0
}

impl Arms {
    pub const DEFAULT: Arms = Arms {
        length: 5.0,
        thickness: 2.0,
        gap: 3.0,
        t_style: false,
    };
}

impl Dot {
    pub const DEFAULT: Dot = Dot { size: 2.0 };
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

impl Shape {
    /// No parts yet: white, with a black outline once parts are added.
    pub fn blank() -> Shape {
        Shape {
            color: Color::WHITE,
            snap: Snap::Round,
            arms: None,
            dot: None,
            circle: None,
            outline: Some(Outline::DEFAULT),
        }
    }

    /// Furthest distance from the center any part reaches, in game pixels.
    pub fn reach(&self) -> f32 {
        let arms = (self.arms).map_or(0.0, |a| a.thickness / 2.0 + a.gap.max(0.0) + a.length);
        let dot = self.dot.map_or(0.0, |d| d.size / 2.0);
        let circle = self.circle.map_or(0.0, |c| c.radius + c.thickness / 2.0);
        let outline = self.outline.map_or(0.0, |o| o.thickness);
        arms.max(dot).max(circle) + outline
    }
}

impl Crosshair {
    pub fn shape(name: impl Into<String>, shape: Shape) -> Crosshair {
        Crosshair {
            name: name.into(),
            zoom: 1.0,
            opacity: 1.0,
            shape: Some(shape),
            pixels: None,
        }
    }

    pub fn pixels(name: impl Into<String>, pixels: Pixels) -> Crosshair {
        Crosshair {
            name: name.into(),
            zoom: 1.0,
            opacity: 1.0,
            shape: None,
            pixels: Some(pixels),
        }
    }
}
