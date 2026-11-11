//! Turns key presses and clicks in the settings window into bindable chords.

use crossz_core::{Chord, Key, Mods};
use gpui_kit::{Keystroke, Modifiers, MouseButton, NavigationDirection};

pub fn from_keystroke(keystroke: &Keystroke) -> Option<Chord> {
    // gpui names keys like "f8", "pageup" or "a"; ours match case-insensitively.
    let key = Key::from_name(&keystroke.key)?;
    Some(Chord {
        mods: mods(&keystroke.modifiers),
        key,
    })
}

pub fn from_mouse(button: MouseButton, modifiers: &Modifiers) -> Chord {
    let key = match button {
        MouseButton::Left => Key::MOUSE1,
        MouseButton::Right => Key::MOUSE2,
        MouseButton::Middle => Key::MOUSE3,
        MouseButton::Navigate(NavigationDirection::Back) => Key::MOUSE4,
        MouseButton::Navigate(NavigationDirection::Forward) => Key::MOUSE5,
    };
    Chord {
        mods: mods(modifiers),
        key,
    }
}

fn mods(m: &Modifiers) -> Mods {
    Mods {
        ctrl: m.control,
        shift: m.shift,
        alt: m.alt,
        win: m.platform,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_gpui_key_names() {
        for (gpui, ours) in [
            ("f8", "F8"),
            ("pageup", "PageUp"),
            ("a", "A"),
            ("space", "Space"),
            ("[", "["),
        ] {
            let keystroke = Keystroke {
                key: gpui.into(),
                ..Default::default()
            };
            assert_eq!(from_keystroke(&keystroke).unwrap().to_string(), ours);
        }
    }

    #[test]
    fn keeps_modifiers() {
        let keystroke = Keystroke {
            key: "x".into(),
            modifiers: Modifiers {
                control: true,
                alt: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            from_keystroke(&keystroke).unwrap().to_string(),
            "Ctrl+Alt+X"
        );
    }

    #[test]
    fn side_buttons_are_mouse4_and_5() {
        let none = Modifiers::default();
        let back = from_mouse(MouseButton::Navigate(NavigationDirection::Back), &none);
        assert_eq!(back.to_string(), "Mouse4");
        assert_eq!(from_mouse(MouseButton::Right, &none).to_string(), "Mouse2");
    }
}
