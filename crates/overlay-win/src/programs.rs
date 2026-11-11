use std::collections::HashMap;
use std::path::Path;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GWL_EXSTYLE, GetIconInfo, GetWindowLongW, GetWindowTextW, HICON,
    ICONINFO, IsWindowVisible, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING};

use crate::foreground;

/// A program with a visible window, for picking games from a list.
#[derive(Debug, Clone)]
pub struct Program {
    pub exe: String,
    pub path: String,
    /// Title of its first visible window, usually the game's name.
    pub title: String,
    pub icon: Option<Icon>,
}

/// Straight-alpha BGRA pixels, top-down.
#[derive(Debug, Clone)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Programs with a visible, titled top-level window, one entry per executable, sorted
/// by title.
pub fn running_programs() -> Vec<Program> {
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: the callback only runs during EnumWindows, while `windows` is alive and
    // exclusively borrowed through the LPARAM.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&raw mut windows as isize)) };

    let mut programs: HashMap<String, Program> = HashMap::new();
    for hwnd in windows {
        let Some(path) = foreground::exe_path(hwnd) else {
            continue;
        };
        let Some(exe) = Path::new(&path).file_name() else {
            continue;
        };
        let exe = exe.to_string_lossy().into_owned();
        programs
            .entry(exe.to_ascii_lowercase())
            .or_insert_with(|| Program {
                icon: icon(&path),
                title: window_title(hwnd),
                exe,
                path,
            });
    }
    let mut programs: Vec<Program> = programs.into_values().collect();
    programs.sort_by_key(|p| p.title.to_lowercase());
    programs
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `running_programs`.
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    // SAFETY: querying properties of a window handle EnumWindows just gave us.
    let listed = unsafe {
        IsWindowVisible(hwnd).as_bool()
            && GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 == 0
    };
    if listed && !window_title(hwnd).is_empty() {
        windows.push(hwnd);
    }
    true.into()
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: `buf` is a valid writable buffer of the given length.
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..len.max(0) as usize])
}

/// The 32×32 shell icon of an executable.
pub fn icon(path: &str) -> Option<Icon> {
    let mut info = SHFILEINFOW::default();
    // SAFETY: `info` is a valid SHFILEINFOW of the size passed; the icon it returns is
    // ours to destroy.
    unsafe {
        let found = SHGetFileInfoW(
            &HSTRING::from(path),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if found == 0 || info.hIcon.is_invalid() {
            return None;
        }
        let icon = icon_pixels(info.hIcon);
        let _ = DestroyIcon(info.hIcon);
        icon
    }
}

fn icon_pixels(hicon: HICON) -> Option<Icon> {
    let mut info = ICONINFO::default();
    // SAFETY: GetIconInfo fills `info` with two bitmaps we must delete, done below.
    unsafe { GetIconInfo(hicon, &mut info).ok()? };
    let pixels = bitmap_bgra(info.hbmColor);
    // SAFETY: both bitmaps came from GetIconInfo and are deleted once.
    unsafe {
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    let (width, height, mut bgra) = pixels?;
    // Icons without an alpha channel mark transparency only in the mask; treat their
    // pixels as opaque rather than invisible.
    if bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
        bgra.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|px| px[3] = 255);
    }
    Some(Icon {
        width,
        height,
        bgra,
    })
}

fn bitmap_bgra(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    if bitmap.is_invalid() {
        return None;
    }
    let mut header = BITMAP::default();
    // SAFETY: `header` is a BITMAP of the size passed.
    let read = unsafe {
        GetObjectW(
            bitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&raw mut header).cast()),
        )
    };
    if read == 0 || header.bmWidth <= 0 || header.bmHeight <= 0 {
        return None;
    }
    let (width, height) = (header.bmWidth as u32, header.bmHeight as u32);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    // SAFETY: `bgra` holds exactly `height` rows of 32bpp pixels as `info` describes;
    // the screen DC is released before returning.
    let lines = unsafe {
        let dc = GetDC(None);
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    (lines == height as i32).then_some((width, height, bgra))
}
