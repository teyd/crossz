use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, OpenProcess,
    PROCESS_SYNCHRONIZE, SetEvent, WaitForSingleObject,
};
use windows::core::w;

/// Held for the life of the process so a second crossz can tell one is running.
pub struct SingleInstance(HANDLE);

// SAFETY: a mutex handle is a process-wide kernel handle, usable from any thread.
unsafe impl Send for SingleInstance {}

impl SingleInstance {
    /// `None` if another crossz in this session already holds it.
    pub fn acquire() -> Option<SingleInstance> {
        // SAFETY: static name; the handle is closed in Drop.
        unsafe {
            let handle = CreateMutexW(None, false, w!("Local\\crossz")).ok()?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return None;
            }
            Some(SingleInstance(handle))
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Waits up to 30 seconds for process `pid` to exit; returns at once if it's gone.
pub fn wait_for_exit(pid: u32) {
    // SAFETY: the handle is only used to wait on and is closed afterwards.
    unsafe {
        if let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            WaitForSingleObject(process, 30_000);
            let _ = CloseHandle(process);
        }
    }
}

/// Asks the running crossz to show its settings window.
pub fn signal_show() {
    // SAFETY: static name; the handle is closed after signalling.
    unsafe {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, SHOW_EVENT) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}

/// Calls `on_show` on a background thread whenever another launch calls `signal_show`.
pub fn listen_for_show(on_show: impl Fn() + Send + 'static) {
    // SAFETY: auto-reset event owned by the listener thread for the life of the process.
    let Ok(event) = (unsafe { CreateEventW(None, false, false, SHOW_EVENT) }) else {
        return;
    };
    let event = event.0 as isize;
    std::thread::spawn(move || {
        loop {
            // SAFETY: the event handle stays open for the life of the process.
            let result = unsafe { WaitForSingleObject(HANDLE(event as _), INFINITE) };
            if result != WAIT_OBJECT_0 {
                return;
            }
            on_show();
        }
    });
}

const SHOW_EVENT: windows::core::PCWSTR = w!("Local\\crossz-show");
