//! Keys and mouse buttons from a listen-only event tap. Listening, never filtering, means
//! the tap can't delay or eat a game's input; it needs Input Monitoring permission.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crossz_core::{Key, Target};
use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType, CGPreflightListenEventAccess,
    CGRequestListenEventAccess,
};

use crate::{Event, Shared, keys, refresh};

thread_local! {
    // The tap, so the callback can switch it back on after macOS disables it.
    static TAP: RefCell<Option<CFRetained<CFMachPort>>> = const { RefCell::new(None) };
}

const LISTENED: [CGEventType; 9] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::FlagsChanged,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::RightMouseDown,
    CGEventType::RightMouseUp,
    CGEventType::OtherMouseDown,
    CGEventType::OtherMouseUp,
];

pub fn spawn(shared: Arc<Shared>) {
    std::thread::Builder::new()
        .name("crossz-input".into())
        .spawn(move || run(shared))
        .expect("spawning the input thread");
}

fn run(shared: Arc<Shared>) {
    if !CGPreflightListenEventAccess() {
        CGRequestListenEventAccess();
    }
    let mask = LISTENED.iter().fold(0u64, |mask, t| mask | 1 << t.0);
    let mut reported = false;
    // Permission can be granted while we run, so keep trying until the tap is allowed.
    let tap = loop {
        if shared.quit.load(Ordering::Relaxed) {
            return;
        }
        // SAFETY: `shared` outlives the tap: this thread holds an Arc to it for as long
        // as the run loop below runs, which is the tap's whole life.
        let tap = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                mask,
                Some(callback),
                Arc::as_ptr(&shared) as *mut c_void,
            )
        };
        match tap {
            Some(tap) => break tap,
            None => {
                if !reported {
                    (shared.on_event)(Event::InputUnavailable);
                    reported = true;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    };
    let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
        return;
    };
    let Some(run_loop) = CFRunLoop::current() else {
        return;
    };
    // SAFETY: reading an immutable CoreFoundation constant.
    run_loop.add_source(Some(&source), unsafe { kCFRunLoopCommonModes });
    TAP.with(|t| *t.borrow_mut() = Some(tap));
    CFRunLoop::run();
}

unsafe extern "C-unwind" fn callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: `user_info` is the Shared that `run` keeps alive while the tap exists.
    let shared = unsafe { &*(user_info as *const Shared) };
    // SAFETY: CoreGraphics passes a valid event for the duration of the callback.
    let event_ref = unsafe { event.as_ref() };
    if kind == CGEventType::TapDisabledByTimeout || kind == CGEventType::TapDisabledByUserInput {
        TAP.with(|t| {
            if let Some(tap) = t.borrow().as_ref() {
                CGEvent::tap_enable(tap, true);
            }
        });
    } else if let Some((key, down)) = transition(kind, event_ref) {
        on_key(shared, key, down);
    }
    event.as_ptr()
}

/// The key or button an event pressed or released.
fn transition(kind: CGEventType, event: &CGEvent) -> Option<(Key, bool)> {
    let field = |f: CGEventField| CGEvent::integer_value_field(Some(event), f);
    match kind {
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let key = keys::from_mac(field(CGEventField::KeyboardEventKeycode) as u16)?;
            Some((key, kind == CGEventType::KeyDown))
        }
        // Modifiers only report their new state: a modifier is down if its flag is set.
        CGEventType::FlagsChanged => {
            let key = keys::from_mac(field(CGEventField::KeyboardEventKeycode) as u16)?;
            let flags = CGEvent::flags(Some(event));
            let flag = match key {
                Key::SHIFT => CGEventFlags::MaskShift,
                Key::CTRL => CGEventFlags::MaskControl,
                Key::ALT => CGEventFlags::MaskAlternate,
                Key::WIN => CGEventFlags::MaskCommand,
                _ => return None,
            };
            Some((key, flags.contains(flag)))
        }
        CGEventType::LeftMouseDown | CGEventType::LeftMouseUp => {
            Some((Key::MOUSE1, kind == CGEventType::LeftMouseDown))
        }
        CGEventType::RightMouseDown | CGEventType::RightMouseUp => {
            Some((Key::MOUSE2, kind == CGEventType::RightMouseDown))
        }
        CGEventType::OtherMouseDown | CGEventType::OtherMouseUp => {
            let key = keys::from_mouse_button(field(CGEventField::MouseEventButtonNumber))?;
            Some((key, kind == CGEventType::OtherMouseDown))
        }
        _ => None,
    }
}

fn on_key(shared: &Shared, key: Key, down: bool) {
    let summary = |engine: &crossz_core::Engine| {
        engine.shown().map(|s| {
            (
                s.crosshair.name.clone(),
                s.game.exe.clone(),
                s.target == Target::Window,
            )
        })
    };
    let (enabled_changed, visible_changed) = {
        let mut engine = shared.engine.lock().unwrap();
        let (was_enabled, before) = (engine.enabled(), summary(&engine));
        engine.key(key, down);
        let enabled_changed = (engine.enabled() != was_enabled).then_some(engine.enabled());
        (enabled_changed, summary(&engine) != before)
    };
    if let Some(enabled) = enabled_changed {
        (shared.on_event)(Event::EnabledChanged(enabled));
    }
    if visible_changed {
        refresh(shared);
    }
}
