use std::ffi::c_void;

use crossz_core::Image;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, HWND_TOPMOST, RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, ULW_ALPHA,
    UpdateLayeredWindow, WDA_EXCLUDEFROMCAPTURE, WDA_NONE, WNDCLASSEXW, WNDPROC, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, Result, w};

/// Creates the overlay: layered for per-pixel alpha, transparent to clicks, never
/// activated, topmost, and kept out of Alt-Tab. Starts hidden.
pub fn create(wndproc: WNDPROC) -> Result<HWND> {
    // SAFETY: the class name is a static wide string and `wndproc` lives for the
    // whole program; GetModuleHandleW(None) returns this executable's module.
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: wndproc,
            hInstance: instance,
            lpszClassName: w!("crossz-overlay"),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(Error::from_thread());
        }
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("crossz-overlay"),
            w!("crossz"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
}

/// Puts `image` on screen with its top-left corner at `(x, y)` and shows the window.
pub fn present(hwnd: HWND, image: &Image, x: i32, y: i32) -> Result<()> {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: image.width as i32,
        // Negative height makes the DIB top-down, matching `Image` rows.
        biHeight: -(image.height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    // SAFETY: every GDI object created here is released before returning. The DIB is
    // 32bpp top-down with exactly `width * height` pixels, so copying `image.bgra`
    // (4 bytes per pixel, same layout) stays in bounds.
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let mut bits: *mut c_void = std::ptr::null_mut();
        let result = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .and_then(|bitmap| {
                std::ptr::copy_nonoverlapping(
                    image.bgra.as_ptr(),
                    bits.cast::<u8>(),
                    image.bgra.len(),
                );
                let previous = SelectObject(memory, bitmap.into());
                let result = UpdateLayeredWindow(
                    hwnd,
                    Some(screen),
                    Some(&POINT { x, y }),
                    Some(&SIZE {
                        cx: image.width as i32,
                        cy: image.height as i32,
                    }),
                    Some(memory),
                    Some(&POINT::default()),
                    COLORREF(0),
                    Some(&blend),
                    ULW_ALPHA,
                );
                SelectObject(memory, previous);
                let _ = DeleteObject(bitmap.into());
                result
            });
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        result?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    raise(hwnd);
    Ok(())
}

pub fn hide(hwnd: HWND) {
    // SAFETY: `hwnd` is the overlay window owned by this thread.
    let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
}

/// Some games put themselves topmost too; re-assert our place above them.
pub fn raise(hwnd: HWND) {
    // SAFETY: `hwnd` is the overlay window owned by this thread.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

pub fn exclude_from_capture(hwnd: HWND, exclude: bool) {
    let affinity = if exclude {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    // SAFETY: `hwnd` is the overlay window owned by this thread.
    let _ = unsafe { SetWindowDisplayAffinity(hwnd, affinity) };
}
