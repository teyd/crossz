use tiny_skia::{Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform};

use crate::{Color, Crosshair, Pixels, Scale, Shape, Snap};

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

const PAD: u32 = 2;

/// Draws a crosshair at `scale` (the game's stretch), times its own zoom.
pub fn render(crosshair: &Crosshair, scale: Scale) -> Image {
    let zoom = crosshair.zoom.max(0.05);
    let scale = Scale {
        x: scale.x * zoom,
        y: scale.y * zoom,
    };
    let opacity = crosshair.opacity.clamp(0.0, 1.0);
    match (&crosshair.pixels, &crosshair.shape) {
        (Some(pixels), _) => render_pixels(pixels, scale, opacity),
        (None, Some(shape)) => render_shape(shape, scale, opacity),
        (None, None) => finish(Pixmap::new(PAD * 2, PAD * 2).expect("non-empty")),
    }
}

/// Canvas size for something reaching `reach` screen pixels from the center: even,
/// so the center falls on a pixel boundary, with a little padding.
fn canvas(reach_x: f32, reach_y: f32) -> (Pixmap, (f32, f32)) {
    let even = |v: f32| (v.max(0.0).ceil() as u32).next_multiple_of(2);
    let width = even(reach_x) * 2 + PAD * 2;
    let height = even(reach_y) * 2 + PAD * 2;
    let pixmap = Pixmap::new(width, height).expect("crosshair canvas is never empty");
    (pixmap, ((width / 2) as f32, (height / 2) as f32))
}

fn finish(pixmap: Pixmap) -> Image {
    let (width, height) = (pixmap.width(), pixmap.height());
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

/// Every drawn pixel becomes a rectangle whose edges are rounded to whole screen
/// pixels, so a stretched drawing stays crisp, like a stretched game's HUD.
fn render_pixels(pixels: &Pixels, scale: Scale, opacity: f32) -> Image {
    let (w, h) = (pixels.width() as f32, pixels.height() as f32);
    let (mut pixmap, center) = canvas(w / 2.0 * scale.x + 1.0, h / 2.0 * scale.y + 1.0);
    // Halves round up, so an odd drawing's center pixel stays one pixel wide at 1x.
    let edge = |i: u32, size: f32, s: f32| ((i as f32 - size / 2.0) * s + 0.5).floor();
    for y in 0..pixels.height() {
        let (top, bottom) = (edge(y, h, scale.y), edge(y + 1, h, scale.y));
        let mut x = 0;
        while x < pixels.width() {
            let Some(color) = pixels.get(x, y) else {
                x += 1;
                continue;
            };
            // One rectangle per run of equal pixels in the row.
            let start = x;
            while x < pixels.width() && pixels.get(x, y) == Some(color) {
                x += 1;
            }
            let span = Span {
                left: edge(start, w, scale.x),
                top,
                right: edge(x, w, scale.x),
                bottom,
            };
            fill_rect(&mut pixmap, span, center, &paint(color, opacity, false));
        }
    }
    finish(pixmap)
}

fn render_shape(shape: &Shape, scale: Scale, opacity: f32) -> Image {
    let snap = shape.snap;
    let reach = shape.reach();
    let (mut pixmap, center) = canvas(reach * scale.x, reach * scale.y);
    let (width, height) = (pixmap.width(), pixmap.height());
    let anti_alias = snap == Snap::AntiAlias;
    let rects = rects(shape, scale, snap);
    let ring = shape.circle.and_then(|circle| {
        // Stroking in game pixels under a non-uniform transform stretches the ring
        // into an ellipse with thicker sides, just like the stretched game would.
        let path = PathBuilder::from_circle(0.0, 0.0, circle.radius)?;
        let transform = Transform::from_row(scale.x, 0.0, 0.0, scale.y, center.0, center.1);
        Some((path, circle.thickness, transform))
    });

    // Outlines go on their own layer, drawn solid and blended once, so a translucent
    // outline doesn't get darker where two parts' outlines overlap. They go under every
    // fill, so one part's outline never covers another part.
    if let Some(outline) = shape.outline {
        let mut layer = Pixmap::new(width, height).expect("same size as the canvas");
        let solid = Color {
            a: 255,
            ..outline.color
        };
        let x = snap.size(outline.thickness * scale.x);
        let y = snap.size(outline.thickness * scale.y);
        for &span in &rects {
            fill_rect(
                &mut layer,
                span.expand(x, y),
                center,
                &paint(solid, 1.0, anti_alias),
            );
        }
        if let Some((path, thickness, transform)) = &ring {
            let width = thickness + outline.thickness * 2.0;
            stroke(
                &mut layer,
                path,
                width,
                *transform,
                &paint(solid, 1.0, true),
            );
        }
        let blend = PixmapPaint {
            opacity: outline.color.a as f32 / 255.0 * opacity,
            ..PixmapPaint::default()
        };
        pixmap.draw_pixmap(0, 0, layer.as_ref(), &blend, Transform::identity(), None);
    }
    for &span in &rects {
        fill_rect(
            &mut pixmap,
            span,
            center,
            &paint(shape.color, opacity, anti_alias),
        );
    }
    if let Some((path, thickness, transform)) = &ring {
        let paint = paint(shape.color, opacity, true);
        stroke(&mut pixmap, path, *thickness, *transform, &paint);
    }
    finish(pixmap)
}

fn fill_rect(pixmap: &mut Pixmap, span: Span, (cx, cy): (f32, f32), paint: &Paint) {
    let rect = Rect::from_ltrb(
        cx + span.left,
        cy + span.top,
        cx + span.right,
        cy + span.bottom,
    );
    if let Some(rect) = rect {
        pixmap.fill_rect(rect, paint, Transform::identity(), None);
    }
}

fn stroke(pixmap: &mut Pixmap, path: &Path, width: f32, transform: Transform, paint: &Paint) {
    let stroke = Stroke {
        width,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, paint, &stroke, transform, None);
}

fn paint(color: Color, opacity: f32, anti_alias: bool) -> Paint<'static> {
    let alpha = (color.a as f32 * opacity).round() as u8;
    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, alpha);
    paint.anti_alias = anti_alias;
    paint
}

fn rects(shape: &Shape, scale: Scale, snap: Snap) -> Vec<Span> {
    let mut rects = Vec::with_capacity(5);
    if let Some(arms) = shape.arms {
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
        let mut push = |span| rects.push(span);
        push(horizontal(v_left - gap_x - len_x, v_left - gap_x));
        push(horizontal(v_right + gap_x, v_right + gap_x + len_x));
        push(vertical(h_bottom + gap_y, h_bottom + gap_y + len_y));
        if !arms.t_style {
            push(vertical(h_top - gap_y - len_y, h_top - gap_y));
        }
    }
    if let Some(dot) = shape.dot {
        let (left, right) = snap.centered(snap.size(dot.size * scale.x));
        let (top, bottom) = snap.centered(snap.size(dot.size * scale.y));
        let span = Span {
            left,
            top,
            right,
            bottom,
        };
        rects.push(span);
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Arms, Dot, Outline, Resolution};

    fn draw(shape: &Shape, scale: Scale) -> Image {
        render(&Crosshair::shape("test", shape.clone()), scale)
    }

    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];

    fn arms() -> Shape {
        Shape {
            color: Color::GREEN,
            arms: Some(Arms {
                length: 6.0,
                thickness: 2.0,
                gap: 2.0,
                t_style: false,
            }),
            outline: None,
            ..Shape::blank()
        }
    }

    impl Shape {
        fn without_arms(mut self) -> Shape {
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
        let img = draw(&arms(), Scale::ONE);
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
        let img = draw(&arms(), stretched());
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
        let img = draw(&c, Scale::ONE);
        assert_exact(
            &img,
            &[(-9, -1, -3, 1), (3, -1, 9, 1), (-1, 3, 1, 9)],
            GREEN,
        );
    }

    #[test]
    fn stretched_dot_is_wide() {
        let c = Shape {
            dot: Some(Dot::DEFAULT),
            outline: None,
            ..arms()
        }
        .without_arms();
        let img = draw(&c, stretched());
        assert_exact(&img, &[(-2, -1, 2, 2)], GREEN);
    }

    #[test]
    fn outline_surrounds_fill() {
        let c = Shape {
            dot: Some(Dot::DEFAULT),
            outline: Some(Outline {
                thickness: 1.0,
                color: Color::BLACK,
            }),
            ..arms()
        }
        .without_arms();
        let img = draw(&c, Scale::ONE);
        let (cx, cy) = (img.width / 2, img.height / 2);
        assert_eq!(img.pixel(cx, cy), GREEN);
        assert_eq!(img.pixel(cx - 1, cy - 1), GREEN);
        assert_eq!(img.pixel(cx - 2, cy - 2), BLACK);
        assert_eq!(img.pixel(cx + 1, cy + 1), BLACK);
        assert_eq!(img.pixel(cx + 2, cy + 2), CLEAR);
    }

    #[test]
    fn translucent_outlines_dont_stack_where_parts_overlap() {
        let c = Shape {
            dot: Some(Dot::DEFAULT),
            outline: Some(Outline {
                thickness: 1.0,
                color: Color {
                    a: 128,
                    ..Color::BLACK
                },
            }),
            ..arms()
        };
        let c = Shape {
            arms: Some(Arms {
                gap: 0.0,
                ..c.arms.unwrap()
            }),
            ..c
        };
        let img = draw(&c, Scale::ONE);
        let (cx, cy) = (img.width / 2, img.height / 2);
        // Next to where the right arm's and the bottom arm's outlines meet.
        let corner = img.pixel(cx + 1, cy + 1);
        let edge = img.pixel(cx + 4, cy + 1);
        assert_eq!(corner, edge);
        assert!((127..=128).contains(&edge[3]), "{edge:?}");
    }

    #[test]
    fn opacity_premultiplies() {
        let c = Crosshair {
            opacity: 0.5,
            ..Crosshair::shape("test", arms())
        };
        let img = render(&c, Scale::ONE);
        let [b, g, r, a] = img.pixel(img.width / 2 + 3, img.height / 2);
        assert_eq!((b, r), (0, 0));
        assert!((127..=128).contains(&a), "alpha {a}");
        assert_eq!(g, a);
    }

    #[test]
    fn canvas_fits_everything_and_is_even() {
        let c = Shape {
            circle: Some(crate::Circle {
                radius: 10.0,
                thickness: 1.0,
            }),
            outline: Some(Outline::DEFAULT),
            ..arms()
        };
        let img = draw(&c, stretched());
        assert_eq!(img.width % 2, 0);
        assert_eq!(img.height % 2, 0);
        let edge_clear = (0..img.width)
            .all(|x| img.pixel(x, 0)[3] == 0 && img.pixel(x, img.height - 1)[3] == 0)
            && (0..img.height)
                .all(|y| img.pixel(0, y)[3] == 0 && img.pixel(img.width - 1, y)[3] == 0);
        assert!(edge_clear, "crosshair touches the canvas edge");
    }

    fn dot_pixels(size: u32) -> Pixels {
        let mut p = Pixels::new(size, size);
        p.set(size / 2, size / 2, Some(Color::GREEN));
        p
    }

    #[test]
    fn odd_pixel_drawing_keeps_a_one_pixel_center() {
        let img = render(&Crosshair::pixels("p", dot_pixels(3)), Scale::ONE);
        assert_exact(&img, &[(0, 0, 1, 1)], GREEN);
    }

    #[test]
    fn pixel_drawing_stretches_like_a_game_hud() {
        let mut p = Pixels::new(4, 4);
        p.set(0, 1, Some(Color::GREEN));
        p.set(1, 1, Some(Color::GREEN));
        let img = render(&Crosshair::pixels("p", p), stretched());
        // Columns 0-1 of 4 span x -4..0 at 2x; row 1 spans y -1.5..0 at 1.5x, rounded.
        assert_exact(&img, &[(-4, -1, 0, 0)], GREEN);
    }

    #[test]
    fn zoom_scales_on_top_of_the_stretch() {
        let c = Crosshair {
            zoom: 2.0,
            ..Crosshair::pixels("p", dot_pixels(2))
        };
        let img = render(&c, Scale::ONE);
        assert_exact(&img, &[(0, 0, 2, 2)], GREEN);
    }
}
