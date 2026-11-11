//! macOS virtual key codes (`kVK_*`) to the Windows virtual-key codes crossz stores
//! bindings in, so a config works on both systems.

use crossz_core::Key;

pub fn from_mac(code: u16) -> Option<Key> {
    let vk = match code {
        0x00 => b'A' as u16,
        0x01 => b'S' as u16,
        0x02 => b'D' as u16,
        0x03 => b'F' as u16,
        0x04 => b'H' as u16,
        0x05 => b'G' as u16,
        0x06 => b'Z' as u16,
        0x07 => b'X' as u16,
        0x08 => b'C' as u16,
        0x09 => b'V' as u16,
        0x0B => b'B' as u16,
        0x0C => b'Q' as u16,
        0x0D => b'W' as u16,
        0x0E => b'E' as u16,
        0x0F => b'R' as u16,
        0x10 => b'Y' as u16,
        0x11 => b'T' as u16,
        0x12 => b'1' as u16,
        0x13 => b'2' as u16,
        0x14 => b'3' as u16,
        0x15 => b'4' as u16,
        0x16 => b'6' as u16,
        0x17 => b'5' as u16,
        0x18 => 0xBB, // =
        0x19 => b'9' as u16,
        0x1A => b'7' as u16,
        0x1B => 0xBD, // -
        0x1C => b'8' as u16,
        0x1D => b'0' as u16,
        0x1E => 0xDD, // ]
        0x1F => b'O' as u16,
        0x20 => b'U' as u16,
        0x21 => 0xDB, // [
        0x22 => b'I' as u16,
        0x23 => b'P' as u16,
        0x24 => 0x0D, // return
        0x25 => b'L' as u16,
        0x26 => b'J' as u16,
        0x27 => 0xDE, // '
        0x28 => b'K' as u16,
        0x29 => 0xBA, // ;
        0x2A => 0xDC, // \
        0x2B => 0xBC, // ,
        0x2C => 0xBF, // /
        0x2D => b'N' as u16,
        0x2E => b'M' as u16,
        0x2F => 0xBE,                        // .
        0x30 => 0x09,                        // tab
        0x31 => 0x20,                        // space
        0x32 => 0xC0,                        // `
        0x33 => 0x08,                        // delete (backspace)
        0x35 => 0x1B,                        // escape
        0x36 | 0x37 => 0x5B,                 // command, as the Windows key
        0x38 | 0x3C => 0x10,                 // shift
        0x39 => 0x14,                        // caps lock
        0x3A | 0x3D => 0x12,                 // option, as Alt
        0x3B | 0x3E => 0x11,                 // control
        0x41 => 0x6E,                        // keypad .
        0x43 => 0x6A,                        // keypad *
        0x45 => 0x6B,                        // keypad +
        0x4B => 0x6F,                        // keypad /
        0x4C => 0x0D,                        // keypad enter
        0x4E => 0x6D,                        // keypad -
        0x52..=0x59 => 0x60 + (code - 0x52), // keypad 0-7
        0x5B => 0x68,                        // keypad 8
        0x5C => 0x69,                        // keypad 9
        0x7A => 0x70,                        // F1
        0x78 => 0x71,
        0x63 => 0x72,
        0x76 => 0x73,
        0x60 => 0x74,
        0x61 => 0x75,
        0x62 => 0x76,
        0x64 => 0x77,
        0x65 => 0x78,
        0x6D => 0x79,
        0x67 => 0x7A,
        0x6F => 0x7B, // F12
        0x69 => 0x7C,
        0x6B => 0x7D,
        0x71 => 0x7E,
        0x6A => 0x7F,
        0x40 => 0x80,
        0x4F => 0x81,
        0x50 => 0x82,
        0x5A => 0x83, // F20
        0x72 => 0x2D, // help, where Insert sits
        0x73 => 0x24, // home
        0x74 => 0x21, // page up
        0x75 => 0x2E, // forward delete
        0x77 => 0x23, // end
        0x79 => 0x22, // page down
        0x7B => 0x25, // left
        0x7C => 0x27, // right
        0x7D => 0x28, // down
        0x7E => 0x26, // up
        _ => return None,
    };
    Some(Key(vk))
}

/// Mouse buttons by CoreGraphics button number: 0 left, 1 right, 2 middle, 3 and 4 side.
pub fn from_mouse_button(number: i64) -> Option<Key> {
    Some(match number {
        0 => Key::MOUSE1,
        1 => Key::MOUSE2,
        2 => Key::MOUSE3,
        3 => Key::MOUSE4,
        4 => Key::MOUSE5,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(code: u16) -> String {
        from_mac(code).unwrap().name().unwrap()
    }

    #[test]
    fn letters_digits_and_function_keys() {
        assert_eq!(name(0x00), "A");
        assert_eq!(name(0x0D), "W");
        assert_eq!(name(0x1D), "0");
        assert_eq!(name(0x7A), "F1");
        assert_eq!(name(0x64), "F8");
        assert_eq!(name(0x6F), "F12");
    }

    #[test]
    fn modifiers_fold_to_windows_equivalents() {
        assert_eq!(name(0x3C), "Shift");
        assert_eq!(name(0x3E), "Ctrl");
        assert_eq!(name(0x3A), "Alt");
        assert_eq!(name(0x37), "Win");
    }

    #[test]
    fn side_buttons() {
        assert_eq!(from_mouse_button(3), Some(Key::MOUSE4));
        assert_eq!(from_mouse_button(9), None);
    }
}
