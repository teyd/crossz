//! Imports Valorant crosshair codes like `0;P;h;0;0l;4;0o;0;0a;1;0f;0;1b;0`.
//!
//! A code is a version, then `key;value` pairs grouped by section markers: `P` primary,
//! `A` aim down sights, `S` sniper. Only the primary crosshair is used; keys that are
//! missing take Valorant's defaults.

use std::collections::HashMap;

use crate::{Arms, Color, Crosshair, Dot, Outline};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a Valorant crosshair code")]
    NotACode,
    #[error("{key} has an invalid value {value:?}")]
    BadValue { key: String, value: String },
}

/// Valorant's preset colors, by the `c` index. 8 means a custom `u` color.
const COLORS: [Color; 8] = [
    Color::rgb(255, 255, 255),
    Color::rgb(0, 255, 0),
    Color::rgb(127, 255, 0),
    Color::rgb(223, 255, 0),
    Color::rgb(255, 255, 0),
    Color::rgb(0, 255, 255),
    Color::rgb(255, 0, 255),
    Color::rgb(255, 0, 0),
];

pub fn parse(code: &str, name: impl Into<String>) -> Result<Crosshair, Error> {
    let tokens: Vec<&str> = code.trim().split(';').map(str::trim).collect();
    if tokens.first().is_none_or(|v| v.parse::<u32>().is_err()) || tokens.len() < 2 {
        return Err(Error::NotACode);
    }
    let mut primary = HashMap::new();
    let mut section = "";
    let mut rest = tokens[1..].iter();
    while let Some(&token) = rest.next() {
        if matches!(token, "P" | "A" | "S") {
            section = token;
            continue;
        }
        let Some(&value) = rest.next() else {
            break;
        };
        if section == "P" {
            primary.insert(token, value);
        }
    }

    let number = |key: &str, default: f32| -> Result<f32, Error> {
        primary.get(key).map_or(Ok(default), |v| {
            v.parse().map_err(|_| Error::BadValue {
                key: key.to_owned(),
                value: (*v).to_owned(),
            })
        })
    };
    let on = |key: &str, default: bool| -> Result<bool, Error> {
        Ok(number(key, if default { 1.0 } else { 0.0 })? != 0.0)
    };
    // Valorant measures a line's offset from the center; ours starts at the edge of
    // the center square, half a thickness out.
    let lines = |prefix: &str, defaults: (f32, f32, f32, f32)| -> Result<Arms, Error> {
        let (thickness, length, offset, opacity) = defaults;
        let thickness = number(&format!("{prefix}t"), thickness)?;
        Ok(Arms {
            length: number(&format!("{prefix}l"), length)?,
            thickness,
            gap: number(&format!("{prefix}o"), offset)? - thickness / 2.0,
            t_style: false,
            opacity: number(&format!("{prefix}a"), opacity)?,
        })
    };

    let color = match number("c", 0.0)? as usize {
        8 => primary
            .get("u")
            .and_then(|hex| format!("#{hex}").parse().ok())
            .unwrap_or(COLORS[0]),
        i => *COLORS.get(i).unwrap_or(&COLORS[0]),
    };
    let mut crosshair = Crosshair::blank(name);
    crosshair.color = color;
    crosshair.outline = on("h", true)?
        .then(|| -> Result<Outline, Error> {
            let alpha = (number("o", 0.5)?.clamp(0.0, 1.0) * 255.0).round() as u8;
            Ok(Outline {
                thickness: number("t", 1.0)?,
                color: Color {
                    a: alpha,
                    ..Color::BLACK
                },
            })
        })
        .transpose()?;
    crosshair.dot = on("d", false)?
        .then(|| -> Result<Dot, Error> {
            Ok(Dot {
                size: number("z", 2.0)?,
                opacity: number("a", 1.0)?,
            })
        })
        .transpose()?;
    crosshair.arms = on("0b", true)?
        .then(|| lines("0", (2.0, 6.0, 3.0, 0.8)))
        .transpose()?;
    crosshair.outer = on("1b", true)?
        .then(|| lines("1", (2.0, 2.0, 10.0, 0.35)))
        .transpose()?;
    Ok(crosshair)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scale, render};

    #[test]
    fn missing_keys_take_valorant_defaults() {
        let c = parse("0;P", "default").unwrap();
        assert_eq!(c.color, COLORS[0]);
        assert_eq!(c.outline.unwrap().color.a, 128);
        assert!(c.dot.is_none());
        let inner = c.arms.unwrap();
        assert_eq!(
            (inner.thickness, inner.length, inner.gap, inner.opacity),
            (2.0, 6.0, 2.0, 0.8)
        );
        assert_eq!(c.outer.unwrap().length, 2.0);
    }

    #[test]
    fn reads_only_the_primary_crosshair() {
        let c = parse("0;p;0;s;1;P;h;0;0l;4;1b;0;A;0l;9;S;s;1", "x").unwrap();
        assert!(c.outline.is_none());
        assert_eq!(c.arms.unwrap().length, 4.0);
        assert!(c.outer.is_none());
    }

    #[test]
    fn custom_color() {
        let c = parse("0;P;c;8;u;00FF7FFF", "x").unwrap();
        assert_eq!(c.color, Color::rgb(0, 255, 127));
        let c = parse("0;P;c;7", "x").unwrap();
        assert_eq!(c.color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn offset_zero_lines_meet_in_a_solid_plus() {
        let c = parse("0;P;h;0;f;0;0l;4;0o;0;0a;1;0f;0;1b;0", "x").unwrap();
        let img = render(&c, Scale::ONE);
        let (cx, cy) = (img.width / 2, img.height / 2);
        for (x, y) in [
            (cx, cy),
            (cx - 1, cy - 1),
            (cx + 3, cy),
            (cx - 4, cy),
            (cx, cy + 3),
        ] {
            assert_eq!(img.pixel(x, y), [255, 255, 255, 255], "({x}, {y})");
        }
        assert_eq!(img.pixel(cx + 4, cy)[3], 0);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("hello", "x").is_err());
        assert!(parse("", "x").is_err());
        assert!(parse("0;P;0l;long", "x").is_err());
    }
}
