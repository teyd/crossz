use serde::{Deserialize, Serialize};

use crate::Color;

/// A crosshair, measured in the profile's virtual-resolution pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Crosshair {
    pub color: Color,
    #[serde(default = "full_opacity")]
    pub opacity: f32,
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
/// so arms stay symmetric whatever the thickness.
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

fn full_opacity() -> f32 {
    1.0
}

impl Default for Crosshair {
    fn default() -> Self {
        Crosshair {
            color: Color::GREEN,
            opacity: 1.0,
            arms: Some(Arms {
                length: 6.0,
                thickness: 2.0,
                gap: 2.0,
                t_style: false,
            }),
            dot: None,
            circle: None,
            outline: Some(Outline {
                thickness: 1.0,
                color: Color::BLACK,
            }),
        }
    }
}

impl Crosshair {
    /// Furthest distance from the center any part reaches, in virtual pixels.
    pub fn reach(&self) -> f32 {
        let arms = self
            .arms
            .map_or(0.0, |a| a.thickness / 2.0 + a.gap.max(0.0) + a.length);
        let dot = self.dot.map_or(0.0, |d| d.size / 2.0);
        let circle = self.circle.map_or(0.0, |c| c.radius + c.thickness / 2.0);
        let outline = self.outline.map_or(0.0, |o| o.thickness);
        arms.max(dot).max(circle) + outline
    }
}
