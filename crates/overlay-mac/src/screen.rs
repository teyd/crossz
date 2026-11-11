//! What's in the foreground and where, from CoreGraphics' window list. Bounds come
//! without Screen Recording permission; only window titles would need it.

use std::ffi::c_void;
use std::path::Path;

use objc2::rc::Retained;
use objc2_core_foundation::{CFArray, CGPoint, CGRect};
use objc2_core_graphics::{
    CGDirectDisplayID, CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode,
    CGGetDisplaysWithPoint, CGMainDisplayID, CGWindowListCopyWindowInfo, CGWindowListOption,
    kCGNullWindowID,
};
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString};

/// A rectangle in points, in global coordinates with the origin at the top left of the
/// main display, which is what CoreGraphics uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn from_cg(r: CGRect) -> Rect {
        Rect {
            x: r.origin.x,
            y: r.origin.y,
            width: r.size.width,
            height: r.size.height,
        }
    }

    fn center(&self) -> CGPoint {
        CGPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// A display: its bounds in points and how many pixels a point is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Display {
    pub bounds: Rect,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Foreground {
    pub pid: i32,
    pub exe: Option<String>,
    /// The app's frontmost normal window, if it has one on screen.
    pub window: Option<Rect>,
    pub display: Display,
}

/// The frontmost app's frontmost window: the first normal-layer window in the on-screen
/// list, which runs front to back. Our overlay sits on a higher layer, so it's skipped.
pub fn foreground() -> Foreground {
    let front = windows().into_iter().find(|w| w.layer == 0);
    let window = front.as_ref().map(|w| w.bounds);
    let display = display_at(window.map(|w| w.center()));
    Foreground {
        pid: front.as_ref().map_or(0, |w| w.pid),
        exe: front.and_then(|w| exe_name(w.pid)),
        window,
        display,
    }
}

struct WindowInfo {
    pid: i32,
    layer: i64,
    bounds: Rect,
}

fn windows() -> Vec<WindowInfo> {
    let options =
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements;
    let Some(list) = CGWindowListCopyWindowInfo(options, kCGNullWindowID) else {
        return Vec::new();
    };
    // SAFETY: CFArray and NSArray are toll-free bridged, and the window list is an array
    // of dictionaries with string keys.
    let list: &NSArray<NSDictionary<NSString, objc2::runtime::AnyObject>> =
        unsafe { &*(&*list as *const CFArray).cast() };
    let number = |dict: &NSDictionary<NSString, objc2::runtime::AnyObject>, key: &str| {
        let value = dict.objectForKey(&NSString::from_str(key))?;
        value.downcast::<NSNumber>().ok().map(|n| n.as_f64())
    };
    list.iter()
        .filter_map(|dict| {
            let bounds_value = dict.objectForKey(&NSString::from_str("kCGWindowBounds"))?;
            let bounds: Retained<NSDictionary> = bounds_value.downcast().ok()?;
            // SAFETY: kCGWindowBounds is a dictionary with string keys.
            let bounds: Retained<NSDictionary<NSString, objc2::runtime::AnyObject>> =
                unsafe { Retained::cast_unchecked(bounds) };
            Some(WindowInfo {
                pid: number(&dict, "kCGWindowOwnerPID")? as i32,
                layer: number(&dict, "kCGWindowLayer")? as i64,
                bounds: Rect {
                    x: number(&bounds, "X")?,
                    y: number(&bounds, "Y")?,
                    width: number(&bounds, "Width")?,
                    height: number(&bounds, "Height")?,
                },
            })
        })
        .collect()
}

/// The display containing `point`, or the main display.
pub fn display_at(point: Option<CGPoint>) -> Display {
    let mut id: CGDirectDisplayID = CGMainDisplayID();
    if let Some(point) = point {
        let mut found: CGDirectDisplayID = 0;
        let mut count = 0u32;
        // SAFETY: both out-pointers are valid locals with room for one display.
        unsafe { CGGetDisplaysWithPoint(point, 1, &mut found, &mut count) };
        if count == 1 {
            id = found;
        }
    }
    let bounds = Rect::from_cg(CGDisplayBounds(id));
    let mode = CGDisplayCopyDisplayMode(id);
    let pixels = CGDisplayMode::pixel_width(mode.as_deref()) as f64;
    let scale = if bounds.width > 0.0 && pixels > 0.0 {
        pixels / bounds.width
    } else {
        1.0
    };
    Display { bounds, scale }
}

/// Height of the main display in points, to flip CoreGraphics' top-left coordinates
/// into AppKit's bottom-left ones.
pub fn main_display_height() -> f64 {
    CGDisplayBounds(CGMainDisplayID()).size.height
}

/// File name of a process's executable, e.g. `Safari`.
pub fn exe_name(pid: i32) -> Option<String> {
    exe_path(pid).and_then(|path| {
        Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
    })
}

pub fn exe_path(pid: i32) -> Option<String> {
    let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `buf` is writable for the length passed.
    let len =
        unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast::<c_void>(), buf.len() as u32) };
    (len > 0).then(|| String::from_utf8_lossy(&buf[..len as usize]).into_owned())
}
