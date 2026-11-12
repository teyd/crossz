//! Bridges the UI to the platform: the overlay, running programs, icons and autostart.
//! Windows and macOS crates share one API; elsewhere these do nothing, which keeps the
//! settings UI runnable.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{App, Global, RenderImage};

pub struct RunningProgram {
    pub exe: String,
    pub path: String,
    pub title: String,
}

/// Program icons by executable path, loaded once each.
#[derive(Default)]
struct Icons(HashMap<String, Option<Arc<RenderImage>>>);

impl Global for Icons {}

pub fn icon(path: &str, cx: &mut App) -> Option<Arc<RenderImage>> {
    let icons = cx.default_global::<Icons>();
    (icons.0.entry(path.to_owned()))
        .or_insert_with(|| imp::load_icon(path))
        .clone()
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use std::sync::Arc;

    use crossz_core::Config;
    #[cfg(target_os = "macos")]
    use crossz_overlay_mac as platform;
    #[cfg(windows)]
    use crossz_overlay_win as platform;
    use gpui_kit::{App, Global, RenderImage};
    use platform::{Command, Event, Overlay};

    use super::RunningProgram;
    use crate::Message;
    use crate::store::Store;

    struct Handle(Overlay);

    impl Global for Handle {}

    /// On macOS this must run on the main thread, which it does inside the app's run loop.
    pub fn start(config: Config, messages: async_channel::Sender<Message>, cx: &mut App) {
        let overlay = Overlay::spawn(config, move |event| {
            let message = match event {
                Event::EnabledChanged(on) => Message::Enabled(on),
                Event::InputUnavailable => Message::InputUnavailable,
                Event::Status(status) => Message::Status(status),
            };
            let _ = messages.send_blocking(message);
        });
        match overlay {
            Ok(overlay) => cx.set_global(Handle(overlay)),
            Err(e) => Store::report(cx, format!("couldn't start the overlay: {e}")),
        }
    }

    pub fn apply(cx: &mut App) {
        let store = Store::get(cx);
        if store.error.is_none()
            && let Some(Handle(overlay)) = cx.try_global::<Handle>()
        {
            overlay.send(Command::SetConfig(Box::new(store.config.clone())));
        }
        sync_autostart(cx);
    }

    pub fn set_enabled(enabled: bool, cx: &mut App) {
        if let Some(Handle(overlay)) = cx.try_global::<Handle>() {
            overlay.send(Command::SetEnabled(enabled));
        }
    }

    pub fn running_programs() -> Vec<RunningProgram> {
        platform::running_programs()
            .into_iter()
            .map(|p| RunningProgram {
                exe: p.exe,
                path: p.path,
                title: p.title,
            })
            .collect()
    }

    pub fn main_display_size() -> (u32, u32) {
        platform::main_display_size()
    }

    pub fn load_icon(path: &str) -> Option<Arc<RenderImage>> {
        let icon = platform::icon(path)?;
        crate::preview::icon(icon.width, icon.height, icon.bgra)
    }

    /// Development builds never register themselves to start at login.
    pub fn sync_autostart(cx: &mut App) {
        if cfg!(debug_assertions) {
            return;
        }
        let enabled = Store::get(cx).config.general.start_with_windows;
        if let Err(e) = platform::autostart::set(enabled, "--autostart") {
            Store::report(cx, format!("couldn't change starting at login: {e}"));
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::sync::Arc;

    use crossz_core::Config;
    use gpui_kit::{App, RenderImage};

    use super::RunningProgram;
    use crate::Message;

    pub fn start(_: Config, _: async_channel::Sender<Message>, _: &mut App) {}

    pub fn apply(_: &mut App) {}

    pub fn set_enabled(enabled: bool, _: &mut App) {
        // Without an overlay nothing reports the change back, so echo it for the tray.
        crate::tray::set_enabled(enabled);
    }

    pub fn running_programs() -> Vec<RunningProgram> {
        Vec::new()
    }

    pub fn main_display_size() -> (u32, u32) {
        (1920, 1080)
    }

    pub fn load_icon(_: &str) -> Option<Arc<RenderImage>> {
        None
    }

    pub fn sync_autostart(_: &mut App) {}
}

pub use imp::{apply, main_display_size, running_programs, set_enabled, start, sync_autostart};
