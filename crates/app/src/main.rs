// Release builds are tray apps; a console window would pop up next to the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod capture;
mod overlay;
mod preview;
mod store;
mod tray;
mod ui;
mod watch;

use std::path::PathBuf;

use crossz_core::Config;
use gpui_kit::{App, QuitMode};

use store::Store;

pub enum Message {
    Reloaded(Config),
    FileError(String),
    Enabled(bool),
    Menu(tray_icon::menu::MenuId),
    Tray(tray::Command),
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crossz")
        .join("config.toml")
}

fn main() {
    #[cfg(windows)]
    crossz_overlay_win::enable_dpi_awareness();

    let path = config_path();
    // A broken file isn't overwritten at startup: defaults run until the user fixes it or
    // changes a setting in the UI.
    let (config, load_error) = match Config::load_or_create(&path) {
        Ok(config) => (config, None),
        Err(e) => (Config::default(), Some(e.to_string())),
    };
    let start_hidden = std::env::args().any(|arg| arg == "--hidden");
    let (messages, inbox) = async_channel::unbounded();
    watch::spawn(path.clone(), messages.clone());

    gpui_kit::application()
        .with_assets(assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_global(Store::new(config.clone(), path));
            if let Some(error) = load_error {
                Store::report(cx, error);
            }
            overlay::start(config, messages.clone(), cx);
            let has_tray = match tray::create(messages) {
                Ok(()) => true,
                Err(e) => {
                    Store::report(cx, format!("couldn't create the tray icon: {e}"));
                    false
                }
            };
            cx.spawn(async move |cx| {
                while let Ok(message) = inbox.recv().await {
                    cx.update(|cx| handle(message, cx));
                }
            })
            .detach();
            if !has_tray {
                // Nothing would be left to reopen the window or quit from.
                cx.on_window_closed(|cx, _| cx.quit()).detach();
            }
            if !start_hidden || !has_tray {
                ui::open(cx);
            }
        });
}

fn handle(message: Message, cx: &mut App) {
    match message {
        Message::Reloaded(config) => Store::reload(cx, config),
        Message::FileError(error) => Store::report(cx, error),
        Message::Enabled(enabled) => tray::set_enabled(enabled),
        Message::Menu(id) => {
            if let Some(command) = tray::command(&id) {
                run(command, cx);
            }
        }
        Message::Tray(command) => run(command, cx),
    }
}

fn run(command: tray::Command, cx: &mut App) {
    match command {
        tray::Command::OpenSettings => ui::open(cx),
        tray::Command::SetEnabled(enabled) => overlay::set_enabled(enabled, cx),
        tray::Command::Quit => cx.quit(),
    }
}
