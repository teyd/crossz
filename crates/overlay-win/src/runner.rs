//! The overlay thread: owns the window, the event hooks and the `Engine`, and re-renders
//! or moves the crosshair whenever what should be shown changes.

use std::cell::RefCell;
use std::sync::mpsc::{Receiver, SyncSender};

use crossz_core::{Config, Crosshair, Engine, Image, Scale, Target, render};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DefWindowProcW, DestroyWindow, DispatchMessageW, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND, GetForegroundWindow, GetMessageW, MSG, OBJID_WINDOW, PostQuitMessage,
    TranslateMessage, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNTHREAD, WM_APP, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_INPUT,
};

use crate::{Command, Event, foreground, input, window};

pub const WM_COMMAND_READY: u32 = WM_APP + 1;

thread_local! {
    static RUNNER: RefCell<Option<Runner>> = const { RefCell::new(None) };
}

/// Runs `f` on this thread's runner. Win32 can re-enter the window procedure while we're
/// mid-update (e.g. from inside SetWindowPos); those nested calls are skipped.
fn with_runner(f: impl FnOnce(&mut Runner)) {
    RUNNER.with(|cell| {
        if let Ok(mut runner) = cell.try_borrow_mut()
            && let Some(runner) = runner.as_mut()
        {
            f(runner);
        }
    });
}

struct Rendered {
    crosshair: Crosshair,
    scale: Scale,
    image: Image,
}

struct Runner {
    hwnd: HWND,
    engine: Engine,
    commands: Receiver<Command>,
    on_event: Box<dyn Fn(Event) + Send>,
    foreground: HWND,
    location_hook: Option<(u32, HWINEVENTHOOK)>,
    rendered: Option<Rendered>,
    /// Top-left corner of what's on screen, `None` while hidden.
    placed: Option<(i32, i32)>,
}

pub fn run(
    config: Config,
    commands: Receiver<Command>,
    on_event: Box<dyn Fn(Event) + Send>,
    ready: SyncSender<windows::core::Result<isize>>,
) {
    // SAFETY: setting this thread's DPI awareness has no memory-safety preconditions.
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let hwnd = match window::create(Some(wndproc)).and_then(|hwnd| {
        input::register(hwnd)?;
        Ok(hwnd)
    }) {
        Ok(hwnd) => hwnd,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };

    // SAFETY: the callback is a plain function, and out-of-context hooks are delivered
    // through this thread's message loop below. Skipping our own thread avoids hearing
    // about our own window.
    let foreground_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(on_win_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNTHREAD,
        )
    };

    window::exclude_from_capture(hwnd, config.general.exclude_from_capture);
    let mut runner = Runner {
        hwnd,
        engine: Engine::new(config),
        commands,
        on_event,
        foreground: HWND::default(),
        location_hook: None,
        rendered: None,
        placed: None,
    };
    // SAFETY: no preconditions.
    runner.set_foreground(unsafe { GetForegroundWindow() });
    RUNNER.with(|cell| *cell.borrow_mut() = Some(runner));
    let _ = ready.send(Ok(hwnd.0 as isize));

    let mut msg = MSG::default();
    // SAFETY: standard message loop on the thread that owns the window and hooks.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWinEvent(foreground_hook);
    }
    RUNNER.with(|cell| {
        if let Some(mut runner) = cell.borrow_mut().take() {
            runner.watch_location(None);
        }
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => with_runner(|r| r.on_input(lparam)),
        WM_COMMAND_READY => with_runner(Runner::drain_commands),
        WM_DISPLAYCHANGE => with_runner(Runner::update),
        WM_DESTROY => {
            // SAFETY: no preconditions.
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    // SAFETY: forwarding the message we were given. WM_INPUT must reach DefWindowProc
    // too so Windows can free the input data.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    with_runner(|r| match event {
        EVENT_SYSTEM_FOREGROUND => r.set_foreground(hwnd),
        EVENT_OBJECT_LOCATIONCHANGE
            if hwnd == r.foreground && object == OBJID_WINDOW.0 && child == CHILDID_SELF as i32 =>
        {
            r.update();
        }
        _ => {}
    });
}

impl Runner {
    fn set_foreground(&mut self, hwnd: HWND) {
        self.foreground = hwnd;
        self.engine
            .set_foreground(foreground::exe_name(hwnd).as_deref());
        self.update();
    }

    fn on_input(&mut self, lparam: LPARAM) {
        let before = self.visible_state();
        let was_enabled = self.engine.enabled();
        let mut any = false;
        input::read(lparam, |key, down| {
            any = true;
            self.engine.key(key, down);
        });
        if !any {
            return;
        }
        if self.engine.enabled() != was_enabled {
            (self.on_event)(Event::EnabledChanged(self.engine.enabled()));
        }
        if self.visible_state() != before {
            self.update();
        }
    }

    /// Summary of the engine's decision, to skip redundant updates on key events.
    fn visible_state(&self) -> Option<(String, String, Target)> {
        (self.engine.shown()).map(|s| (s.crosshair.name.clone(), s.game.exe.clone(), s.target))
    }

    fn drain_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::SetConfig(config) => {
                    window::exclude_from_capture(self.hwnd, config.general.exclude_from_capture);
                    self.engine.set_config(*config);
                }
                Command::SetEnabled(enabled) => {
                    self.engine.set_enabled(enabled);
                    (self.on_event)(Event::EnabledChanged(enabled));
                }
                Command::Quit => {
                    // SAFETY: destroying our own window from its thread; WM_DESTROY
                    // then ends the message loop.
                    let _ = unsafe { DestroyWindow(self.hwnd) };
                    return;
                }
            }
        }
        self.update();
    }

    fn update(&mut self) {
        let Some(shown) = self.engine.shown() else {
            self.watch_location(None);
            if self.placed.take().is_some() {
                window::hide(self.hwnd);
            }
            return;
        };
        let target = shown.target;
        let rect = foreground::target_rect(self.foreground, target);
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        let game = shown.game;
        let scale = game.resolution.scale(width as u32, height as u32);
        let [dx, dy] = game.offset;

        let stale = (self.rendered.as_ref())
            .is_none_or(|r| r.crosshair != *shown.crosshair || r.scale != scale);
        if stale {
            self.rendered = Some(Rendered {
                crosshair: shown.crosshair.clone(),
                scale,
                image: render(shown.crosshair, scale),
            });
        }
        let rendered = self.rendered.as_ref().expect("rendered above");
        let position = (
            rect.left + width / 2 - rendered.image.width as i32 / 2 + dx,
            rect.top + height / 2 - rendered.image.height as i32 / 2 + dy,
        );
        if stale || self.placed != Some(position) {
            if window::present(self.hwnd, &rendered.image, position.0, position.1).is_ok() {
                self.placed = Some(position);
            }
        } else {
            window::raise(self.hwnd);
        }
        let watched = (target == Target::Window).then_some(self.foreground);
        self.watch_location(watched);
    }

    /// Follows moves and resizes of the window we're centered on. The hook is scoped to
    /// its process because location events fire for every window, including the cursor.
    fn watch_location(&mut self, hwnd: Option<HWND>) {
        let pid = hwnd.map(foreground::process_id).filter(|&pid| pid != 0);
        if self.location_hook.map(|(p, _)| p) == pid {
            return;
        }
        if let Some((_, hook)) = self.location_hook.take() {
            // SAFETY: `hook` came from SetWinEventHook and is unhooked once.
            let _ = unsafe { UnhookWinEvent(hook) };
        }
        if let Some(pid) = pid {
            // SAFETY: same as the foreground hook in `run`.
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_OBJECT_LOCATIONCHANGE,
                    EVENT_OBJECT_LOCATIONCHANGE,
                    None,
                    Some(on_win_event),
                    pid,
                    0,
                    WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNTHREAD,
                )
            };
            if !hook.is_invalid() {
                self.location_hook = Some((pid, hook));
            }
        }
    }
}
