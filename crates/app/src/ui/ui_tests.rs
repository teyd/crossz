//! UI integration tests: the real settings window, headless, driven by clicks and keys.

use std::path::PathBuf;

use crossz_core::{Config, Show, Status, Visibility};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
};

use super::SettingsView;
use crate::store::Store;
use crate::{Update, updater};

/// Opens the settings window on `config`, saved to a fresh file that's returned too.
fn open(cx: &mut TestAppContext, name: &str, config: Config) -> (AnyWindowHandle, PathBuf) {
    let dir = std::env::temp_dir().join(format!("crossz-ui-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::init(cx);
        cx.set_global(Store::new(config, path.clone()));
        cx.set_global(Update {
            state: updater::State::Idle,
            messages: async_channel::unbounded().0,
        });
    });
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            Default::default(),
            size(px(1180.), px(780.)),
        ))),
        ..Default::default()
    };
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| SettingsView::new(window, cx))
        })
        .expect("open the settings window")
    });
    (handle, path)
}

fn selected(cx: &mut TestAppContext) -> String {
    cx.update(|cx| Store::get(cx).config.general.crosshair.clone())
}

#[gpui_kit::test]
fn clicking_a_crosshair_selects_and_saves_it(cx: &mut TestAppContext) {
    let (handle, path) = open(cx, "select", Config::default());
    assert_eq!(selected(cx), "Plus");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("tile-Dot", cx);
    })
    .unwrap();

    assert_eq!(selected(cx), "Dot");
    let saved = Config::load_or_create(&path).unwrap().0;
    assert_eq!(saved.general.crosshair, "Dot");
}

#[gpui_kit::test]
fn a_new_crosshair_opens_the_editor_and_escape_closes_it(cx: &mut TestAppContext) {
    let (handle, _) = open(cx, "editor", Config::default());

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("crosshair-done").is_none());
        window.click("crosshair-new", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(selected(cx), "My crosshair");

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("crosshair-done").is_some());
        // The editor takes focus when it opens, so Escape works without a click first.
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();

    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("crosshair-done").is_none());
        assert!(window.try_find("crosshair-new").is_some());
    })
    .unwrap();
}

#[cfg(any(windows, target_os = "macos"))]
#[gpui_kit::test]
fn the_status_line_explains_why_nothing_shows(cx: &mut TestAppContext) {
    let mut config = Config::default();
    config.general.show = Show::InGames;
    let (handle, _) = open(cx, "status", config);

    cx.update(|cx| {
        Store::set_status(
            cx,
            Status {
                program: Some("Code".into()),
                game: None,
                visibility: Visibility::NotHere,
            },
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find("status").label(),
            Some("Not showing: Code isn't one of your games")
        );
    })
    .unwrap();

    // Turned off while crossz itself is in front: the line still says so.
    cx.update(|cx| {
        Store::set_status(
            cx,
            Status {
                program: None,
                game: None,
                visibility: Visibility::Off,
            },
        );
    });
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let label = window.find("status").label().map(str::to_owned);
        assert!(
            label.as_deref().is_some_and(|l| l.starts_with("Off.")),
            "{label:?}"
        );
    })
    .unwrap();
}
