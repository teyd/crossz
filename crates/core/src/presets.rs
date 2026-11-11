//! Built-in crosshairs. They're always available, never written to the config, and can
//! be copied into a custom crosshair to change them.

use std::sync::LazyLock;

use crate::{Arms, Circle, Crosshair, Dot, valorant};

/// Popular Valorant crosshairs from vcrdb.net, kept as their codes so they stay exact.
const VALORANT: &[(&str, &str)] = &[
    // https://www.vcrdb.net/crosshair/66
    ("Solid plus", "0;P;h;0;f;0;0l;4;0o;0;0a;1;0f;0;1b;0"),
    // https://www.vcrdb.net/crosshair/15978
    (
        "Santos",
        "0;s;1;P;o;0.2;m;1;0l;3;0o;2;0a;1;0f;0;1b;0;S;s;0.85;o;1",
    ),
    // https://www.vcrdb.net/crosshair/85
    ("Small dot", "0;P;d;1;f;0;0t;4;0l;1;0o;0;0a;1;0f;0;1b;0"),
    // https://www.vcrdb.net/crosshair/14203
    (
        "Block",
        "0;p;0;s;1;P;o;0.2;f;0;s;0;m;1;0t;6;0l;1;0o;0;0a;1;0f;0;1t;6;1l;1;1o;0;1a;1;1m;0;1f;0",
    ),
];

pub fn all() -> &'static [Crosshair] {
    static PRESETS: LazyLock<Vec<Crosshair>> = LazyLock::new(|| {
        let with = |name: &str, change: &dyn Fn(&mut Crosshair)| {
            let mut c = Crosshair::blank(name);
            change(&mut c);
            c
        };
        let mut presets = vec![
            // Arms meet the center square at gap 0, so the dot fills it in.
            with("Plus", &|c| {
                c.arms = Some(Arms {
                    gap: 0.0,
                    ..Arms::DEFAULT
                });
                c.dot = Some(Dot::DEFAULT);
            }),
            with("Plus with gap", &|c| c.arms = Some(Arms::DEFAULT)),
            with("Dot", &|c| {
                c.dot = Some(Dot {
                    size: 3.0,
                    ..Dot::DEFAULT
                });
            }),
            with("Plus with dot", &|c| {
                c.arms = Some(Arms::DEFAULT);
                c.dot = Some(Dot::DEFAULT);
            }),
            with("T", &|c| {
                c.arms = Some(Arms {
                    t_style: true,
                    ..Arms::DEFAULT
                });
            }),
            with("Circle with dot", &|c| {
                c.circle = Some(Circle::DEFAULT);
                c.dot = Some(Dot::DEFAULT);
            }),
            with("Small plus", &|c| {
                c.arms = Some(Arms {
                    length: 3.0,
                    thickness: 1.0,
                    gap: 2.0,
                    ..Arms::DEFAULT
                });
            }),
        ];
        presets.extend(VALORANT.iter().map(|(name, code)| {
            valorant::parse(code, *name).expect("built-in Valorant codes are valid")
        }));
        presets
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

    #[test]
    fn valorant_presets_load() {
        for (name, _) in VALORANT {
            assert!(get(name).is_some(), "{name}");
        }
    }
}
