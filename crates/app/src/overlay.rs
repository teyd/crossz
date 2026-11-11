//! Bridges the UI to the Win32 overlay. On other platforms there's no overlay and these
//! do nothing, which keeps the settings UI runnable for development.

#[cfg(windows)]
mod imp {
    use crossz_core::Config;
    use crossz_overlay_win::{Command, Event, Overlay};
    use gpui_kit::{App, Global};

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
            overlay.send(Command::SetConfig(store.config.clone()));
        }
    }

    pub fn set_enabled(enabled: bool, cx: &mut App) {
        if let Some(Handle(overlay)) = cx.try_global::<Handle>() {
            overlay.send(Command::SetEnabled(enabled));
        }
    }

    pub fn running_programs() -> Vec<String> {
        crossz_overlay_win::visible_programs()
    }
}

#[cfg(not(windows))]
mod imp {
    use crossz_core::Config;
    use gpui_kit::App;

    use crate::Message;

    pub fn start(_: Config, _: async_channel::Sender<Message>, _: &mut App) {}

    pub fn apply(_: &mut App) {}

    pub fn set_enabled(enabled: bool, _: &mut App) {
        // Without an overlay nothing reports the change back, so echo it for the tray.
        crate::tray::set_enabled(enabled);
    }

    pub fn running_programs() -> Vec<String> {
        Vec::new()
    }
}

pub use imp::{apply, running_programs, set_enabled, start};
