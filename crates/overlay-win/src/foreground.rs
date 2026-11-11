use std::path::Path;

use crossz_core::Target;
use windows::Win32::Foundation::{CloseHandle, HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
    MONITORINFO, MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowThreadProcessId};
use windows::core::PWSTR;

pub fn process_id(hwnd: HWND) -> u32 {
    let mut pid = 0;
    // SAFETY: `pid` outlives the call; an invalid hwnd just leaves it 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

/// File name of the program that owns `hwnd`, e.g. `cs2.exe`.
pub fn exe_name(hwnd: HWND) -> Option<String> {
    let path = exe_path(hwnd)?;
    Path::new(&path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Full path of the executable that owns `hwnd`.
pub fn exe_path(hwnd: HWND) -> Option<String> {
    let pid = process_id(hwnd);
    if pid == 0 {
        return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` and `len` describe a valid writable buffer, and the process handle
    // is closed on every path. Limited-information access works even for elevated games.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        ok.ok()?;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

/// The screen rectangle the crosshair centers on, in physical pixels.
pub fn target_rect(hwnd: HWND, target: Target) -> RECT {
    if target == Target::Window
        && let Some(rect) = client_rect(hwnd)
    {
        return rect;
    }
    // SAFETY: plain value arguments; both calls always return a monitor.
    let monitor = unsafe {
        if hwnd.is_invalid() {
            MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY)
        } else {
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
        }
    };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is initialized with its size as the API requires.
    let _ = unsafe { GetMonitorInfoW(monitor, &mut info) };
    info.rcMonitor
}

fn client_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    let mut origin = POINT::default();
    // SAFETY: out-pointers are valid locals; failures are reported and handled.
    unsafe {
        GetClientRect(hwnd, &mut rect).ok()?;
        ClientToScreen(hwnd, &mut origin).ok().ok()?;
    }
    (rect.right > 0 && rect.bottom > 0).then_some(RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + rect.right,
        bottom: origin.y + rect.bottom,
    })
}
