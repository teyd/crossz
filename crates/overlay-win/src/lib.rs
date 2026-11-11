//! The Win32 side of crossz: a click-through layered window that draws the crosshair,
//! tracking of the foreground program, and raw keyboard/mouse input for bindings.
//! Compiles to nothing on other platforms.
#![cfg(windows)]

mod foreground;
mod input;
mod runner;
mod window;

use std::sync::mpsc;
use std::thread::JoinHandle;

use crossz_core::Config;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub use foreground::visible_programs;

pub enum Command {
    SetConfig(Config),
    SetEnabled(bool),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The master toggle changed, from a hotkey or `Command::SetEnabled`.
    EnabledChanged(bool),
}

/// Handle to the overlay thread. Dropping it stops the overlay.
pub struct Overlay {
    commands: mpsc::Sender<Command>,
    hwnd: isize,
    thread: Option<JoinHandle<()>>,
}

impl Overlay {
    pub fn spawn(
        config: Config,
        on_event: impl Fn(Event) + Send + 'static,
    ) -> windows::core::Result<Overlay> {
        let (commands, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("crossz-overlay".into())
            .spawn(move || runner::run(config, receiver, Box::new(on_event), ready_tx))
            .expect("spawning the overlay thread");
        let hwnd = ready_rx.recv().expect("overlay thread reports startup")?;
        Ok(Overlay {
            commands,
            hwnd,
            thread: Some(thread),
        })
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            // SAFETY: posting to a window owned by another thread is allowed; if the
            // window is already gone the call just fails.
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(self.hwnd as _)),
                    runner::WM_COMMAND_READY,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Makes window and monitor sizes come back in physical pixels. Call before any window
/// is created; later calls fail harmlessly.
pub fn enable_dpi_awareness() {
    // SAFETY: no pointers involved; failure only means awareness was already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}
