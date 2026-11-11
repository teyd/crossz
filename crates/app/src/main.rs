// Release builds are tray apps; a console window would pop up next to the game.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod assets;
mod capture;
mod overlay;
mod preview;
mod store;
mod tray;
mod ui;
mod updater;
mod watch;

use std::path::PathBuf;

use crossz_core::Config;
use gpui_kit::{App, Global, QuitMode};

use store::Store;

pub enum Message {
    Reloaded(Config),
    FileError(String),
    Enabled(bool),
    Menu(tray_icon::menu::MenuId),
    Tray(tray::Command),
    Update(updater::State),
}

/// Where the updater is, for the settings window and the tray.
pub struct Update {
    pub state: updater::State,
    messages: async_channel::Sender<Message>,
}

impl Global for Update {}

impl Update {
    /// Checks now, off the UI thread, unless a check or download is already running.
    pub fn check(cx: &mut App) {
        let update = cx.global_mut::<Update>();
        if matches!(
            update.state,
            updater::State::Checking
                | updater::State::Downloading(_)
                | updater::State::Ready { .. }
        ) {
            return;
        }
        update.state = updater::State::Checking;
        let messages = update.messages.clone();
        std::thread::spawn(move || {
            let progress = messages.clone();
            let state = updater::check_and_download(|state| {
                let _ = progress.send_blocking(Message::Update(state));
            });
            let _ = messages.send_blocking(Message::Update(state));
        });
        cx.refresh_windows();
    }

    pub fn restart(cx: &mut App) {
        let hidden = !ui::is_open(cx);
        match updater::install_and_restart(hidden) {
            Ok(()) => cx.quit(),
            Err(e) => {
                cx.global_mut::<Update>().state =
                    updater::State::Failed(format!("couldn't install the update: {e}"));
                cx.refresh_windows();
            }
        }
    }
}

struct Args {
    hidden: bool,
    #[cfg_attr(not(windows), allow(dead_code))]
    after_update: Option<u32>,
}

fn args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let after_update = (args.iter().position(|a| a == "--after-update"))
        .and_then(|i| args.get(i + 1))
        .and_then(|pid| pid.parse().ok());
    Args {
        hidden: args.iter().any(|a| a == "--hidden" || a == "--autostart"),
        after_update,
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crossz")
        .join("config.toml")
}

fn main() {
    let args = args();
    #[cfg(windows)]
    let _instance = {
        if let Some(pid) = args.after_update {
            crossz_overlay_win::instance::wait_for_exit(pid);
        }
        match crossz_overlay_win::instance::SingleInstance::acquire() {
            Some(instance) => instance,
            None => {
                crossz_overlay_win::instance::signal_show();
                return;
            }
        }
    };
    #[cfg(windows)]
    crossz_overlay_win::enable_dpi_awareness();
    updater::clean_up();

    let path = config_path();
    // A broken file isn't overwritten at startup: defaults run until the user fixes it or
    // changes a setting in the UI.
    let (config, first_run, load_error) = match Config::load_or_create(&path) {
        Ok((config, created)) => (config, created, None),
        Err(e) => (Config::default(), false, Some(e.to_string())),
    };
    let show_window = first_run || !(args.hidden || config.general.start_minimized);
    let (messages, inbox) = async_channel::unbounded();
    watch::spawn(path.clone(), messages.clone());
    #[cfg(windows)]
    {
        let messages = messages.clone();
        crossz_overlay_win::instance::listen_for_show(move || {
            let _ = messages.send_blocking(Message::Tray(tray::Command::OpenSettings));
        });
    }

    gpui_kit::application()
        .with_assets(assets::Assets)
        .with_quit_mode(QuitMode::Explicit)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.set_global(Store::new(config.clone(), path));
            cx.set_global(Update {
                state: updater::State::Idle,
                messages: messages.clone(),
            });
            if let Some(error) = load_error {
                Store::report(cx, error);
            }
            overlay::start(config.clone(), messages.clone(), cx);
            overlay::sync_autostart(cx);
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
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(std::time::Duration::from_secs(10))
                        .await;
                    cx.update(|cx| {
                        if Store::get(cx).config.general.check_for_updates {
                            Update::check(cx);
                        }
                    });
                    cx.background_executor().timer(updater::CHECK_EVERY).await;
                }
            })
            .detach();
            if !has_tray {
                // Nothing would be left to reopen the window or quit from.
                cx.on_window_closed(|cx, _| cx.quit()).detach();
            }
            if show_window || !has_tray {
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
        Message::Update(state) => {
            tray::set_update(&state);
            cx.global_mut::<Update>().state = state;
            cx.refresh_windows();
        }
    }
}

fn run(command: tray::Command, cx: &mut App) {
    match command {
        tray::Command::OpenSettings => ui::open(cx),
        tray::Command::SetEnabled(enabled) => overlay::set_enabled(enabled, cx),
        tray::Command::RestartToUpdate => Update::restart(cx),
        tray::Command::Quit => cx.quit(),
    }
}
