use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    switch::Switch,
};
use gpui_kit::{AnyElement, App, IntoElement as _, ParentElement as _, Styled as _, WeakEntity};

use super::layout::{Section, row};
use super::{Recording, SettingsView, chord_button};
use crate::Update;
use crate::store::Store;
use crate::updater::{self, State as UpdateState};

pub(super) fn page(
    view: &SettingsView,
    this: WeakEntity<SettingsView>,
    cx: &App,
) -> Vec<AnyElement> {
    let general = &Store::get(cx).config.general;
    let master = general.master_toggle;
    let recording = view.recording == Some(Recording::MasterToggle);
    let path = Store::get(cx).path.clone();
    let update_state = cx.global::<Update>().state.clone();
    let switch = |id: &'static str, on: bool, set: fn(&mut crossz_core::General, bool)| {
        Switch::new(id)
            .checked(on)
            .on_change(move |on: &bool, _, cx| {
                let on = *on;
                Store::edit(cx, |c| set(&mut c.general, on));
            })
    };

    let startup = Section::new()
        .title("Startup")
        .child(row(
            if cfg!(windows) {
                "Start with Windows"
            } else {
                "Start at login"
            },
            Some("Starts in the tray when you log in.".into()),
            switch("start-at-login", general.start_with_windows, |g, on| {
                g.start_with_windows = on
            }),
            cx,
        ))
        .child(row(
            "Start minimized",
            Some("When starting at login, open to the tray instead of this window.".into()),
            switch("start-minimized", general.start_minimized, |g, on| {
                g.start_minimized = on
            }),
            cx,
        ));

    let updates = Section::new()
        .title("Updates")
        .child(row(
            format!("crossz {}", updater::VERSION),
            Some(update_description(&update_state).into()),
            update_button(&update_state),
            cx,
        ))
        .child(row(
            "Check for updates automatically",
            Some(
                "Daily. Updates download in the background and install when you restart crossz."
                    .into(),
            ),
            switch("check-for-updates", general.check_for_updates, |g, on| {
                g.check_for_updates = on
            }),
            cx,
        ));

    let controls = Section::new()
        .title("Controls")
        .child(row(
            "Master toggle",
            Some("Turns crossz on or off everywhere.".into()),
            h_flex()
                .gap_2()
                .child(chord_button(
                    "master-toggle".into(),
                    master,
                    recording,
                    view.capture_focus.clone(),
                    Recording::MasterToggle,
                    this,
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
                ),
            cx,
        ))
        .child(row(
            "Hide from screen capture",
            Some("Keeps the crosshair out of screenshots, OBS and Discord streams.".into()),
            switch(
                "exclude-from-capture",
                general.exclude_from_capture,
                |g, on| g.exclude_from_capture = on,
            ),
            cx,
        ))
        .child(row(
            "Settings file",
            Some(path.display().to_string().into()),
            Button::new("open-config")
                .outline()
                .icon(IconName::FolderOpen)
                .label("Show in folder")
                .on_click(move |_, _, cx| cx.reveal_path(&path)),
            cx,
        ));

    vec![
        startup.into_any_element(),
        updates.into_any_element(),
        controls.into_any_element(),
    ]
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
