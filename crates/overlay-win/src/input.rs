use crossz_core::Key;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_INPUTSINK, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    RI_KEY_BREAK, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, RI_MOUSE_BUTTON_5_DOWN,
    RI_MOUSE_BUTTON_5_UP, RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP,
    RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP, RI_MOUSE_RIGHT_BUTTON_DOWN,
    RI_MOUSE_RIGHT_BUTTON_UP,
};

const GENERIC_DESKTOP: u16 = 0x01;
const MOUSE: u16 = 0x02;
const KEYBOARD: u16 = 0x06;

/// Receives keyboard and mouse input as `WM_INPUT` even while a game has focus. Unlike
/// low-level hooks, raw input can't delay the game's own input if we're slow.
pub fn register(hwnd: HWND) -> windows::core::Result<()> {
    let device = |usage| RAWINPUTDEVICE {
        usUsagePage: GENERIC_DESKTOP,
        usUsage: usage,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    // SAFETY: the slice and element size describe valid RAWINPUTDEVICE structs.
    unsafe {
        RegisterRawInputDevices(
            &[device(MOUSE), device(KEYBOARD)],
            size_of::<RAWINPUTDEVICE>() as u32,
        )
    }
}

const MOUSE_BUTTONS: [(u32, Key, bool); 10] = [
    (RI_MOUSE_LEFT_BUTTON_DOWN, Key::MOUSE1, true),
    (RI_MOUSE_LEFT_BUTTON_UP, Key::MOUSE1, false),
    (RI_MOUSE_RIGHT_BUTTON_DOWN, Key::MOUSE2, true),
    (RI_MOUSE_RIGHT_BUTTON_UP, Key::MOUSE2, false),
    (RI_MOUSE_MIDDLE_BUTTON_DOWN, Key::MOUSE3, true),
    (RI_MOUSE_MIDDLE_BUTTON_UP, Key::MOUSE3, false),
    (RI_MOUSE_BUTTON_4_DOWN, Key::MOUSE4, true),
    (RI_MOUSE_BUTTON_4_UP, Key::MOUSE4, false),
    (RI_MOUSE_BUTTON_5_DOWN, Key::MOUSE5, true),
    (RI_MOUSE_BUTTON_5_UP, Key::MOUSE5, false),
];

/// Key and button transitions in a `WM_INPUT` message. Mouse movement, which is most
/// of the traffic, yields nothing.
pub fn read(lparam: LPARAM, mut on_key: impl FnMut(Key, bool)) {
    let mut raw = RAWINPUT::default();
    let mut size = size_of::<RAWINPUT>() as u32;
    // SAFETY: `raw` is a RAWINPUT-sized buffer and `size` says so. Keyboard and mouse
    // packets always fit; anything larger (HID) fails and is ignored.
    let read = unsafe {
        GetRawInputData(
            HRAWINPUT(lparam.0 as _),
            RID_INPUT,
            Some((&raw mut raw).cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if read == u32::MAX || read == 0 {
        return;
    }
    if raw.header.dwType == RIM_TYPEMOUSE.0 {
        // SAFETY: the header says this packet holds mouse data.
        let flags = unsafe { raw.data.mouse.Anonymous.Anonymous.usButtonFlags } as u32;
        if flags == 0 {
            return;
        }
        for (flag, key, down) in MOUSE_BUTTONS {
            if flags & flag != 0 {
                on_key(key, down);
            }
        }
    } else if raw.header.dwType == RIM_TYPEKEYBOARD.0 {
        // SAFETY: the header says this packet holds keyboard data.
        let keyboard = unsafe { raw.data.keyboard };
        // 0xFF marks fake keys sent as part of escaped sequences (e.g. Pause).
        if keyboard.VKey != 0xFF {
            on_key(
                Key(keyboard.VKey),
                keyboard.Flags as u32 & RI_KEY_BREAK == 0,
            );
        }
    }
}
