//! Bridges the UI to Windows: the overlay, running programs, icons and autostart. On
//! other platforms these do nothing, which keeps the settings UI runnable for development.

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

#[cfg(windows)]
mod imp {
    use std::sync::Arc;

    use crossz_core::Config;
    use crossz_overlay_win::{Command, Event, Overlay};
    use gpui_kit::{App, Global, RenderImage};

    use super::RunningProgram;
    use crate::Message;
    use crate::store::Store;

    struct Handle(Overlay);

    impl Global for Handle {}

    pub fn start(config: Config, messages: async_channel::Sender<Message>, cx: &mut App) {
        let overlay = Overlay::spawn(config, move |Event::EnabledChanged(on)| {
            let _ = messages.send_blocking(Message::Enabled(on));
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
        crossz_overlay_win::running_programs()
            .into_iter()
            .map(|p| RunningProgram {
                exe: p.exe,
                path: p.path,
                title: p.title,
            })
            .collect()
    }

    pub fn load_icon(path: &str) -> Option<Arc<RenderImage>> {
        let icon = crossz_overlay_win::icon(path)?;
        crate::preview::icon(icon.width, icon.height, icon.bgra)
    }

    /// Development builds never register themselves to start with Windows.
    pub fn sync_autostart(cx: &mut App) {
        if cfg!(debug_assertions) {
            return;
        }
        let enabled = Store::get(cx).config.general.start_with_windows;
        if let Err(e) = crossz_overlay_win::autostart::set(enabled, "--autostart") {
            Store::report(cx, format!("couldn't change starting with Windows: {e}"));
        }
    }
}

#[cfg(not(windows))]
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

    pub fn load_icon(_: &str) -> Option<Arc<RenderImage>> {
        None
    }

    pub fn sync_autostart(_: &mut App) {}
}

pub use imp::{apply, running_programs, set_enabled, start, sync_autostart};
