use std::cell::RefCell;

use crossz_core::{Arms, Color, Crosshair, Image, Outline, Scale, render};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::Message;

pub enum Command {
    OpenSettings,
    SetEnabled(bool),
    RestartToUpdate,
    Quit,
}

struct Tray {
    _icon: TrayIcon,
    settings: MenuId,
    /// Shows the version; becomes "Restart to update" once an update is ready.
    update: MenuItem,
    enabled: CheckMenuItem,
    quit: MenuId,
}

thread_local! {
    // Tray handles aren't Send; they live on the UI thread that created them.
    static TRAY: RefCell<Option<Tray>> = const { RefCell::new(None) };
}

pub fn create(messages: async_channel::Sender<Message>) -> tray_icon::Result<()> {
    let settings = MenuItem::new("Settings…", true, None);
    let enabled = CheckMenuItem::new("Enabled", true, true, None);
    let update = MenuItem::new(format!("crossz {}", crate::updater::VERSION), false, None);
    let quit = MenuItem::new("Quit crossz", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &settings,
        &enabled,
        &PredefinedMenuItem::separator(),
        &update,
        &quit,
    ])
    .expect("appending tray menu items");

    let icon = TrayIconBuilder::new()
        .with_tooltip("crossz")
        .with_icon(icon())
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()?;

    let menu_messages = messages.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_messages.send_blocking(Message::Menu(event.id));
    }));
    TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            let _ = messages.send_blocking(Message::Tray(Command::OpenSettings));
        }
    }));

    TRAY.with(|tray| {
        *tray.borrow_mut() = Some(Tray {
            _icon: icon,
            settings: settings.id().clone(),
            update,
            enabled,
            quit: quit.id().clone(),
        });
    });
    Ok(())
}

/// Translates a menu click. The check item flips itself when clicked, so its new state
/// is the requested one.
pub fn command(id: &MenuId) -> Option<Command> {
    TRAY.with(|tray| {
        let tray = tray.borrow();
        let tray = tray.as_ref()?;
        if *id == tray.settings {
            Some(Command::OpenSettings)
        } else if id == tray.enabled.id() {
            Some(Command::SetEnabled(tray.enabled.is_checked()))
        } else if id == tray.update.id() {
            Some(Command::RestartToUpdate)
        } else if *id == tray.quit {
            Some(Command::Quit)
        } else {
            None
        }
    })
}

pub fn set_enabled(enabled: bool) {
    TRAY.with(|tray| {
        if let Some(tray) = tray.borrow().as_ref() {
            tray.enabled.set_checked(enabled);
        }
    });
}

pub fn set_update(state: &crate::updater::State) {
    TRAY.with(|tray| {
        if let Some(tray) = tray.borrow().as_ref() {
            match state {
                crate::updater::State::Ready { version, .. } => {
                    tray.update
                        .set_text(format!("Restart to update to {version}"));
                    tray.update.set_enabled(true);
                }
                _ => {
                    tray.update
                        .set_text(format!("crossz {}", crate::updater::VERSION));
                    tray.update.set_enabled(false);
                }
            }
        }
    });
}

fn icon() -> Icon {
    const SIZE: u32 = 32;
    let crosshair = Crosshair {
        color: Color::rgb(0x4a, 0xde, 0x80),
        arms: Some(Arms {
            length: 8.0,
            thickness: 4.0,
            gap: 3.0,
            t_style: false,
            opacity: 1.0,
        }),
        outline: Some(Outline {
            thickness: 2.0,
            color: Color::BLACK,
        }),
        ..Crosshair::blank("tray")
    };
    let image = render(&crosshair, Scale::ONE);
    let rgba = straight_rgba(&image, SIZE);
    Icon::from_rgba(rgba, SIZE, SIZE).expect("tray icon buffer matches its size")
}

/// Centers `image` on a `size`×`size` canvas as straight-alpha RGBA.
pub fn straight_rgba(image: &Image, size: u32) -> Vec<u8> {
    let mut out = vec![0; (size * size * 4) as usize];
    let dx = size as i64 / 2 - image.width as i64 / 2;
    let dy = size as i64 / 2 - image.height as i64 / 2;
    for y in 0..image.height {
        for x in 0..image.width {
            let (tx, ty) = (x as i64 + dx, y as i64 + dy);
            if !(0..size as i64).contains(&tx) || !(0..size as i64).contains(&ty) {
                continue;
            }
            let [b, g, r, a] = image.pixel(x, y);
            let unpremultiply = |c: u8| match a {
                0 => 0,
                a => (c as u32 * 255 / a as u32) as u8,
            };
            let i = ((ty as u32 * size + tx as u32) * 4) as usize;
            out[i..i + 4].copy_from_slice(&[
                unpremultiply(r),
                unpremultiply(g),
                unpremultiply(b),
                a,
            ]);
        }
    }
    out
}
