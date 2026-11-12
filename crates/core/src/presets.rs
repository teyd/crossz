//! Built-in crosshairs. They're always available, never written to the config, and can
//! be copied into a pixel drawing to change them.

use std::sync::LazyLock;

use crate::{Arms, Circle, Crosshair, Dot, Shape};

pub fn all() -> &'static [Crosshair] {
    static PRESETS: LazyLock<Vec<Crosshair>> = LazyLock::new(|| {
        let with = |name: &str, change: &dyn Fn(&mut Shape)| {
            let mut shape = Shape::blank();
            change(&mut shape);
            Crosshair::shape(name, shape)
        };
        vec![
            // Arms meet the center square at gap 0, so the dot fills it in.
            with("Plus", &|s| {
                s.arms = Some(Arms {
                    gap: 0.0,
                    ..Arms::DEFAULT
                });
                s.dot = Some(Dot::DEFAULT);
            }),
            with("Plus with gap", &|s| s.arms = Some(Arms::DEFAULT)),
            with("Dot", &|s| s.dot = Some(Dot { size: 3.0 })),
            with("Plus with dot", &|s| {
                s.arms = Some(Arms::DEFAULT);
                s.dot = Some(Dot::DEFAULT);
            }),
            with("T", &|s| {
                s.arms = Some(Arms {
                    t_style: true,
                    ..Arms::DEFAULT
                });
            }),
            with("Circle with dot", &|s| {
                s.circle = Some(Circle::DEFAULT);
                s.dot = Some(Dot::DEFAULT);
            }),
            with("Small plus", &|s| {
                s.arms = Some(Arms {
                    length: 3.0,
                    thickness: 1.0,
                    gap: 2.0,
                    t_style: false,
                });
            }),
        ]
    });
    &PRESETS
}

pub fn get(name: &str) -> Option<&'static Crosshair> {
    all().iter().find(|c| c.name == name)
}

pub fn is_preset(name: &str) -> bool {
    get(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Scale, render};

    #[test]
    fn names_are_unique() {
        let mut names: Vec<_> = all().iter().map(|c| c.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), all().len());
    }

    #[test]
    fn plus_has_a_filled_center() {
        let img = render(get("Plus").unwrap(), Scale::ONE);
        let (cx, cy) = (img.width / 2, img.height / 2);
        assert_eq!(img.pixel(cx, cy), [255, 255, 255, 255]);
        assert_eq!(img.pixel(cx + 3, cy), [255, 255, 255, 255]);
    }

    #[test]
    fn plus_with_gap_has_an_empty_center() {
        let img = render(get("Plus with gap").unwrap(), Scale::ONE);
        assert_eq!(img.pixel(img.width / 2, img.height / 2)[3], 0);
    }
}
