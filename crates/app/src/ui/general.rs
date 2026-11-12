use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
};
use gpui_kit::{App, ParentElement as _, Styled as _, WeakEntity};

use super::{Recording, SettingsView, chord_button};
use crate::Update;
use crate::store::Store;
use crate::updater::{self, State as UpdateState};

pub(super) fn page(view: &SettingsView, this: WeakEntity<SettingsView>, cx: &App) -> SettingPage {
    let store = Store::get(cx);
    let master = store.config.general.master_toggle;
    let recording = view.recording == Some(Recording::MasterToggle);
    let focus = view.capture_focus.clone();
    let path = store.path.clone();
    let path_label = path.display().to_string();
    let update_state = cx.global::<Update>().state.clone();

    let startup = SettingGroup::new().title("Startup").items([
        SettingItem::new(
            if cfg!(windows) {
                "Start with Windows"
            } else {
                "Start at login"
            },
            SettingField::switch(
                |cx| Store::get(cx).config.general.start_with_windows,
                |on, cx| Store::edit(cx, |c| c.general.start_with_windows = on),
            ),
        )
        .description("Starts in the tray when you log in."),
        SettingItem::new(
            "Start minimized",
            SettingField::switch(
                |cx| Store::get(cx).config.general.start_minimized,
                |on, cx| Store::edit(cx, |c| c.general.start_minimized = on),
            ),
        )
        .description("When starting at login, open to the tray instead of this window."),
    ]);

    let updates = SettingGroup::new().title("Updates").items([
        SettingItem::new(
            format!("crossz {}", updater::VERSION),
            SettingField::render(move |_, _, _| update_button(&update_state)),
        )
        .description(update_description(&cx.global::<Update>().state)),
        SettingItem::new(
            "Check for updates automatically",
            SettingField::switch(
                |cx| Store::get(cx).config.general.check_for_updates,
                |on, cx| Store::edit(cx, |c| c.general.check_for_updates = on),
            ),
        )
        .description(
            "Daily. Updates download in the background and install when you restart crossz.",
        ),
    ]);

    let controls = SettingGroup::new().title("Controls").items([
        SettingItem::new(
            "Master toggle",
            SettingField::render(move |_, _, cx| {
                h_flex()
                    .gap_2()
                    .child(chord_button(
                        "master-toggle".into(),
                        master,
                        recording,
                        focus.clone(),
                        Recording::MasterToggle,
                        this.clone(),
                        cx,
                    ))
                    .child(
                        Button::new("master-toggle-clear")
                            .ghost()
                            .icon(IconName::X)
                            .tooltip("Clear")
                            .disabled(master.is_none())
                            .on_click(|_, _, cx| {
                                Store::edit(cx, |c| c.general.master_toggle = None);
                            }),
                    )
            }),
        )
        .description("Turns crossz on or off everywhere."),
        SettingItem::new(
            "Hide from screen capture",
            SettingField::switch(
                |cx| Store::get(cx).config.general.exclude_from_capture,
                |on, cx| Store::edit(cx, |c| c.general.exclude_from_capture = on),
            ),
        )
        .description("Keeps the crosshair out of screenshots, OBS and Discord streams."),
        SettingItem::new(
            "Settings file",
            SettingField::render(move |_, _, _| {
                let path = path.clone();
                Button::new("open-config")
                    .outline()
                    .icon(IconName::FolderOpen)
                    .label("Show in folder")
                    .on_click(move |_, _, cx| cx.reveal_path(&path))
            }),
        )
        .description(path_label),
    ]);

    SettingPage::new("General")
        .icon(IconName::Settings)
        .groups([startup, updates, controls])
}

fn update_button(state: &UpdateState) -> Button {
    let button = Button::new("update");
    match state {
        UpdateState::Ready { version, .. } => button
            .primary()
            .label(format!("Restart to update to {version}"))
            .on_click(|_, _, cx| Update::restart(cx)),
        UpdateState::Checking | UpdateState::Downloading(_) => {
            button.outline().label("Checking…").disabled(true)
        }
        UpdateState::Unavailable(_) => button.outline().label("Check now").disabled(true),
        _ => button
            .outline()
            .label("Check now")
            .on_click(|_, _, cx| Update::check(cx)),
    }
}

fn update_description(state: &UpdateState) -> String {
    match state {
        UpdateState::Idle => "Not checked yet.".into(),
        UpdateState::Checking => "Checking for updates…".into(),
        UpdateState::UpToDate => "You have the latest version.".into(),
        UpdateState::Downloading(v) => format!("Downloading {v}…"),
        UpdateState::Ready { version, .. } => {
            format!("{version} is downloaded and verified. Restart crossz to use it.")
        }
        UpdateState::Unavailable(reason) => reason.clone(),
        UpdateState::Failed(error) => error.clone(),
    }
}
