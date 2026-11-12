//! Pixel-drawn crosshairs. In the config a drawing is a palette and text rows, one
//! character per pixel, so it stays readable and diffs well:
//!
//! ```toml
//! [crosshair.pixels]
//! palette = ["#ffffff", "#000000"]
//! rows = [
//!   ".111.",
//!   "10001",
//! ]
//! ```

use serde::{Deserialize, Serialize};

use crate::{Color, Image};

/// Characters for palette entries, in order; `.` is an empty pixel.
const SYMBOLS: &str = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const MAX_SIZE: u32 = 64;

/// A grid of pixels whose center is the crosshair's center: between pixels for even
/// sizes, in the middle of one for odd sizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Text", into = "Text")]
pub struct Pixels {
    width: u32,
    height: u32,
    cells: Vec<Option<Color>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    palette: Vec<Color>,
    rows: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("a pixel drawing needs between 1 and {MAX_SIZE} rows and columns")]
    Size,
    #[error("pixel rows have different lengths")]
    Ragged,
    #[error("pixel {0:?} isn't in the palette")]
    Unknown(char),
    #[error("a pixel drawing can use at most {} colors", SYMBOLS.len())]
    TooManyColors,
}

impl Pixels {
    pub fn new(width: u32, height: u32) -> Pixels {
        let (width, height) = (width.clamp(1, MAX_SIZE), height.clamp(1, MAX_SIZE));
        Pixels {
            width,
            height,
            cells: vec![None; (width * height) as usize],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn get(&self, x: u32, y: u32) -> Option<Color> {
        (x < self.width && y < self.height)
            .then(|| self.cells[(y * self.width + x) as usize])
            .flatten()
    }

    pub fn set(&mut self, x: u32, y: u32, color: Option<Color>) {
        if x < self.width && y < self.height {
            self.cells[(y * self.width + x) as usize] = color;
        }
    }

    /// Sets a pixel and its mirror images across both center lines.
    pub fn set_mirrored(&mut self, x: u32, y: u32, color: Option<Color>) {
        let (mx, my) = (
            self.width - 1 - x.min(self.width - 1),
            self.height - 1 - y.min(self.height - 1),
        );
        for (px, py) in [(x, y), (mx, y), (x, my), (mx, my)] {
            self.set(px, py, color);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(Option::is_none)
    }

    /// The same drawing on a canvas of a new size, staying centered.
    pub fn resized(&self, width: u32, height: u32) -> Pixels {
        let mut out = Pixels::new(width, height);
        let dx = (out.width as i64 - self.width as i64) / 2;
        let dy = (out.height as i64 - self.height as i64) / 2;
        for y in 0..self.height {
            for x in 0..self.width {
                let (tx, ty) = (x as i64 + dx, y as i64 + dy);
                if tx >= 0 && ty >= 0 {
                    out.set(tx as u32, ty as u32, self.get(x, y));
                }
            }
        }
        out
    }

    /// A rendered crosshair as pixels, e.g. to paint over a built-in one.
    pub fn from_image(image: &Image) -> Pixels {
        let mut out = Pixels::new(image.width, image.height);
        for y in 0..image.height.min(MAX_SIZE) {
            for x in 0..image.width.min(MAX_SIZE) {
                let [b, g, r, a] = image.pixel(x, y);
                if a == 0 {
                    continue;
                }
                let straight = |c: u8| (c as u32 * 255 / a as u32).min(255) as u8;
                out.set(
                    x,
                    y,
                    Some(Color {
                        r: straight(r),
                        g: straight(g),
                        b: straight(b),
                        a,
                    }),
                );
            }
        }
        out
    }
}

impl TryFrom<Text> for Pixels {
    type Error = Error;

    fn try_from(text: Text) -> Result<Self, Self::Error> {
        let height = text.rows.len() as u32;
        let width = text.rows.first().map_or(0, |r| r.chars().count()) as u32;
        if !(1..=MAX_SIZE).contains(&width) || !(1..=MAX_SIZE).contains(&height) {
            return Err(Error::Size);
        }
        if text.palette.len() > SYMBOLS.len() {
            return Err(Error::TooManyColors);
        }
        let mut pixels = Pixels::new(width, height);
        for (y, row) in text.rows.iter().enumerate() {
            if row.chars().count() as u32 != width {
                return Err(Error::Ragged);
            }
            for (x, symbol) in row.chars().enumerate() {
                if symbol == '.' {
                    continue;
                }
                let color = (SYMBOLS.find(symbol))
                    .and_then(|i| text.palette.get(i))
                    .ok_or(Error::Unknown(symbol))?;
                pixels.set(x as u32, y as u32, Some(*color));
            }
        }
        Ok(pixels)
    }
}

impl From<Pixels> for Text {
    fn from(pixels: Pixels) -> Text {
        let mut palette: Vec<Color> = Vec::new();
        let symbols: Vec<char> = SYMBOLS.chars().collect();
        let rows = (0..pixels.height)
            .map(|y| {
                (0..pixels.width)
                    .map(|x| match pixels.get(x, y) {
                        None => '.',
                        Some(color) => {
                            let i = palette.iter().position(|c| *c == color).unwrap_or_else(|| {
                                palette.push(color);
                                palette.len() - 1
                            });
                            // Past the last symbol, colors share it; drawings never get that colorful.
                            symbols[i.min(symbols.len() - 1)]
                        }
                    })
                    .collect()
            })
            .collect();
        Text { palette, rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plus() -> Pixels {
        let mut p = Pixels::new(5, 5);
        for i in 0..5 {
            p.set(2, i, Some(Color::WHITE));
            p.set(i, 2, Some(Color::WHITE));
        }
        p.set(2, 2, Some(Color::BLACK));
        p
    }

    #[test]
    fn round_trips_through_text() {
        let text = Text::from(plus());
        assert_eq!(text.rows, ["..0..", "..0..", "00100", "..0..", "..0.."]);
        assert_eq!(Pixels::try_from(text).unwrap(), plus());
    }

    #[test]
    fn rejects_bad_text() {
        let text = |rows: &[&str]| Text {
            palette: vec![Color::WHITE],
            rows: rows.iter().map(|r| r.to_string()).collect(),
        };
        assert!(matches!(
            Pixels::try_from(text(&["0.", "0"])),
            Err(Error::Ragged)
        ));
        assert!(matches!(
            Pixels::try_from(text(&["01"])),
            Err(Error::Unknown('1'))
        ));
        assert!(matches!(Pixels::try_from(text(&[])), Err(Error::Size)));
    }

    #[test]
    fn mirrors_into_all_four_quarters() {
        let mut p = Pixels::new(4, 3);
        p.set_mirrored(0, 0, Some(Color::WHITE));
        for (x, y) in [(0, 0), (3, 0), (0, 2), (3, 2)] {
            assert_eq!(p.get(x, y), Some(Color::WHITE));
        }
        assert_eq!(p.get(1, 1), None);
    }

    #[test]
    fn resizing_keeps_the_drawing_centered() {
        let p = plus().resized(9, 9);
        assert_eq!(p.get(4, 4), Some(Color::BLACK));
        assert_eq!(p.get(4, 2), Some(Color::WHITE));
        assert_eq!(plus().resized(3, 3).get(1, 1), Some(Color::BLACK));
    }
}
