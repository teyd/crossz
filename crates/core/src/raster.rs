use tiny_skia::{Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::{Color, Crosshair, Scale, Snap};

/// A rendered crosshair. The crosshair's center is the pixel boundary at
/// `(width / 2, height / 2)`; both dimensions are even.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// Premultiplied BGRA, top-down rows. This is what `UpdateLayeredWindow` takes.
    pub bgra: Vec<u8>,
}

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.bgra[i..i + 4].try_into().unwrap()
    }
}

/// Edges relative to the center, in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Span {
    fn expand(self, x: f32, y: f32) -> Span {
        Span {
            left: self.left - x,
            top: self.top - y,
            right: self.right + x,
            bottom: self.bottom + y,
        }
    }
}

pub fn render(crosshair: &Crosshair, scale: Scale) -> Image {
    let snap = crosshair.snap;
    const PAD: u32 = 2;
    let even = |v: f32| (v.ceil() as u32).next_multiple_of(2);
    let reach = crosshair.reach();
    let width = even(reach * scale.x) * 2 + PAD * 2;
    let height = even(reach * scale.y) * 2 + PAD * 2;
    let (cx, cy) = ((width / 2) as f32, (height / 2) as f32);

    let mut pixmap = Pixmap::new(width, height).expect("crosshair canvas is never empty");
    let opacity = crosshair.opacity.clamp(0.0, 1.0);
    let anti_alias = snap == Snap::AntiAlias;
    let rects = rects(crosshair, scale, snap);

    let mut fill = |span: Span, color: Color| {
        let rect = Rect::from_ltrb(
            cx + span.left,
            cy + span.top,
            cx + span.right,
            cy + span.bottom,
        );
        if let Some(rect) = rect {
            pixmap.fill_rect(
                rect,
                &paint(color, opacity, anti_alias),
                Transform::identity(),
                None,
            );
        }
    };
    // All outlines go first so one part's outline never covers another part's fill.
    if let Some(outline) = crosshair.outline {
        let x = snap.size(outline.thickness * scale.x);
        let y = snap.size(outline.thickness * scale.y);
        for &span in &rects {
            fill(span.expand(x, y), outline.color);
        }
    }
    for &span in &rects {
        fill(span, crosshair.color);
    }

    if let Some(circle) = crosshair.circle
        && let Some(path) = PathBuilder::from_circle(0.0, 0.0, circle.radius)
    {
        // Stroking in virtual pixels under a non-uniform transform stretches the ring
        // into an ellipse with thicker sides, just like the stretched game would.
        let transform = Transform::from_row(scale.x, 0.0, 0.0, scale.y, cx, cy);
        let mut stroke = |width: f32, color: Color| {
            let stroke = Stroke {
                width,
                ..Stroke::default()
            };
            pixmap.stroke_path(
                &path,
                &paint(color, opacity, true),
                &stroke,
                transform,
                None,
            );
        };
        if let Some(outline) = crosshair.outline {
            stroke(circle.thickness + outline.thickness * 2.0, outline.color);
        }
        stroke(circle.thickness, crosshair.color);
    }

    let mut bgra = pixmap.take();
    for px in bgra.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Image {
        width,
        height,
        bgra,
    }
}

fn paint(color: Color, opacity: f32, anti_alias: bool) -> Paint<'static> {
    let alpha = (color.a as f32 * opacity).round() as u8;
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, alpha);
    paint.anti_alias = anti_alias;
    paint
}

fn rects(crosshair: &Crosshair, scale: Scale, snap: Snap) -> Vec<Span> {
    let mut rects = Vec::with_capacity(5);
    if let Some(arms) = crosshair.arms {
        // Vertical arms take their width from the X scale, horizontal arms their height from Y.
        let (v_left, v_right) = snap.centered(snap.size(arms.thickness * scale.x));
        let (h_top, h_bottom) = snap.centered(snap.size(arms.thickness * scale.y));
        let gap_x = snap.offset(arms.gap * scale.x);
        let gap_y = snap.offset(arms.gap * scale.y);
        let len_x = snap.size(arms.length * scale.x);
        let len_y = snap.size(arms.length * scale.y);

        let horizontal = |left, right| Span {
            left,
            top: h_top,
            right,
            bottom: h_bottom,
        };
        let vertical = |top, bottom| Span {
            left: v_left,
            top,
            right: v_right,
            bottom,
        };
        rects.push(horizontal(v_left - gap_x - len_x, v_left - gap_x));
        rects.push(horizontal(v_right + gap_x, v_right + gap_x + len_x));
        rects.push(vertical(h_bottom + gap_y, h_bottom + gap_y + len_y));
        if !arms.t_style {
            rects.push(vertical(h_top - gap_y - len_y, h_top - gap_y));
        }
    }
    if let Some(dot) = crosshair.dot {
        let (left, right) = snap.centered(snap.size(dot.size * scale.x));
        let (top, bottom) = snap.centered(snap.size(dot.size * scale.y));
        rects.push(Span {
            left,
            top,
            right,
            bottom,
        });
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arms, Dot, Outline, Resolution};

    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn arms() -> Crosshair {
        Crosshair {
            color: Color::GREEN,
            arms: Some(Arms {
                length: 6.0,
                thickness: 2.0,
                gap: 2.0,
                t_style: false,
            }),
            outline: None,
            ..Crosshair::blank("test")
        }
    }

    impl Crosshair {
        fn without_arms(mut self) -> Crosshair {
            self.arms = None;
            self
        }
    }

    fn stretched() -> Scale {
        Resolution::custom(1280, 960).scale(2560, 1440)
    }

    /// Asserts every pixel: `color` inside one of `spans` (center-relative), clear elsewhere.
    fn assert_exact(img: &Image, spans: &[(i32, i32, i32, i32)], color: [u8; 4]) {
        let (cx, cy) = (img.width as i32 / 2, img.height as i32 / 2);
        for y in 0..img.height {
            for x in 0..img.width {
                let (rx, ry) = (x as i32 - cx, y as i32 - cy);
                let inside = spans
                    .iter()
                    .any(|&(l, t, r, b)| rx >= l && rx < r && ry >= t && ry < b);
                let expected = if inside { color } else { CLEAR };
                assert_eq!(img.pixel(x, y), expected, "pixel ({rx}, {ry}) from center");
            }
        }
    }

    #[test]
    fn native_arms() {
        let img = render(&arms(), Scale::ONE);
        assert_exact(
            &img,
            &[
                (-9, -1, -3, 1),
                (3, -1, 9, 1),
                (-1, 3, 1, 9),
                (-1, -9, 1, -3),
            ],
            GREEN,
        );
    }

    #[test]
    fn stretched_1280x960_on_1440p_matches_stretched_game() {
        // Horizontal arms: 6x2 -> 12x3. Vertical arms: 2x6 -> 4x9. Gaps: 2 -> 4 (x), 3 (y).
        let img = render(&arms(), stretched());
        assert_exact(
            &img,
            &[
                (-18, -1, -6, 2),
                (6, -1, 18, 2),
                (-2, 5, 2, 14),
                (-2, -13, 2, -4),
            ],
            GREEN,
        );
    }

    #[test]
    fn t_style_drops_top_arm() {
        let mut c = arms();
        c.arms.as_mut().unwrap().t_style = true;
        let img = render(&c, Scale::ONE);
        assert_exact(
            &img,
            &[(-9, -1, -3, 1), (3, -1, 9, 1), (-1, 3, 1, 9)],
            GREEN,
        );
    }

    #[test]
    fn stretched_dot_is_wide() {
        let c = Crosshair {
            dot: Some(Dot { size: 2.0 }),
            outline: None,
            ..arms()
        }
        .without_arms();
        let img = render(&c, stretched());
        assert_exact(&img, &[(-2, -1, 2, 2)], GREEN);
    }

    #[test]
    fn outline_surrounds_fill() {
        let c = Crosshair {
            dot: Some(Dot { size: 2.0 }),
            outline: Some(Outline {
                thickness: 1.0,
                color: Color::BLACK,
            }),
            ..arms()
        }
        .without_arms();
        let img = render(&c, Scale::ONE);
        let (cx, cy) = (img.width / 2, img.height / 2);
        assert_eq!(img.pixel(cx, cy), GREEN);
        assert_eq!(img.pixel(cx - 1, cy - 1), GREEN);
        assert_eq!(img.pixel(cx - 2, cy - 2), BLACK);
        assert_eq!(img.pixel(cx + 1, cy + 1), BLACK);
        assert_eq!(img.pixel(cx + 2, cy + 2), CLEAR);
    }

    #[test]
    fn opacity_premultiplies() {
        let c = Crosshair {
            opacity: 0.5,
            ..arms()
        };
        let img = render(&c, Scale::ONE);
        let [b, g, r, a] = img.pixel(img.width / 2 + 3, img.height / 2);
        assert_eq!((b, r), (0, 0));
        assert!((127..=128).contains(&a), "alpha {a}");
        assert_eq!(g, a);
    }

    #[test]
    fn canvas_fits_everything_and_is_even() {
        let c = Crosshair {
            circle: Some(crate::Circle {
                radius: 10.0,
                thickness: 1.0,
            }),
            outline: Some(Outline::DEFAULT),
            ..arms()
        };
        let img = render(&c, stretched());
        assert_eq!(img.width % 2, 0);
        assert_eq!(img.height % 2, 0);
        let edge_clear = (0..img.width)
            .all(|x| img.pixel(x, 0)[3] == 0 && img.pixel(x, img.height - 1)[3] == 0)
            && (0..img.height)
                .all(|y| img.pixel(0, y)[3] == 0 && img.pixel(img.width - 1, y)[3] == 0);
        assert!(edge_clear, "crosshair touches the canvas edge");
    }
}
