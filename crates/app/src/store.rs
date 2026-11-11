use std::path::PathBuf;

use crossz_core::{Action, Config, Profile};
use gpui_kit::{App, Global};

/// The config being edited, shared by every part of the UI. The file on disk is the
/// source of truth: edits are saved immediately and the overlay picks them up from there.
pub struct Store {
    pub config: Config,
    pub path: PathBuf,
    /// Index of the profile shown in the crosshair editor.
    pub editing: usize,
    /// Why the last edit or file change wasn't applied.
    pub error: Option<String>,
    pub preview_resolution: (u32, u32),
}

impl Global for Store {}

impl Store {
    pub fn new(config: Config, path: PathBuf) -> Store {
        Store {
            config,
            path,
            editing: 0,
            error: None,
            preview_resolution: (2560, 1440),
        }
    }

    pub fn get(cx: &App) -> &Store {
        cx.global::<Store>()
    }

    pub fn profile(cx: &App) -> &Profile {
        let store = Store::get(cx);
        &store.config.profiles[store.editing.min(store.config.profiles.len() - 1)]
    }

    /// Applies `change` to the config, then validates and saves it. An invalid result is
    /// kept in the editor but not saved, with the reason in `error`.
    pub fn edit(cx: &mut App, change: impl FnOnce(&mut Config)) {
        let mut config = Store::get(cx).config.clone();
        change(&mut config);
        Store::set(cx, config);
    }

    pub fn edit_profile(cx: &mut App, change: impl FnOnce(&mut Profile)) {
        let editing = Store::get(cx).editing;
        Store::edit(cx, |config| change(&mut config.profiles[editing]));
    }

    fn set(cx: &mut App, config: Config) {
        if config == Store::get(cx).config {
            return;
        }
        let result = config
            .validate()
            .and_then(|()| config.save(&Store::get(cx).path));
        let store = cx.global_mut::<Store>();
        store.error = result.err().map(|e| e.to_string());
        store.config = config;
        crate::overlay::apply(cx);
        cx.refresh_windows();
    }

    /// Takes a config that changed on disk, e.g. edited by hand.
    pub fn reload(cx: &mut App, config: Config) {
        let store = cx.global_mut::<Store>();
        store.error = None;
        if store.config != config {
            store.editing = store.editing.min(config.profiles.len().saturating_sub(1));
            store.config = config;
            crate::overlay::apply(cx);
        }
        cx.refresh_windows();
    }

    pub fn report(cx: &mut App, error: String) {
        cx.global_mut::<Store>().error = Some(error);
        cx.refresh_windows();
    }
}

/// Renames a profile and every reference to it.
pub fn rename_profile(config: &mut Config, from: &str, to: &str) {
    let rename = |name: &mut String| {
        if name == from {
            *name = to.to_owned();
        }
    };
    for profile in &mut config.profiles {
        rename(&mut profile.name);
    }
    rename(&mut config.programs.default_profile);
    for rule in &mut config.programs.rules {
        if let Some(name) = &mut rule.profile {
            rename(name);
        }
    }
    for binding in &mut config.bindings {
        if let Action::SetProfile(name) = &mut binding.action {
            rename(name);
        }
    }
}

/// Removes a profile; references to it fall back to the default profile.
pub fn remove_profile(config: &mut Config, index: usize) {
    let name = config.profiles.remove(index).name;
    if config.programs.default_profile == name {
        config.programs.default_profile.clear();
    }
    for rule in &mut config.programs.rules {
        if rule.profile.as_deref() == Some(&name) {
            rule.profile = None;
        }
    }
    config
        .bindings
        .retain(|b| b.action != Action::SetProfile(name.clone()));
}

/// `base`, or `base 2`, `base 3`… whichever isn't a profile name yet.
pub fn unique_name(config: &Config, base: &str) -> String {
    (1..)
        .map(|n| match n {
            1 => base.to_owned(),
            n => format!("{base} {n}"),
        })
        .find(|name| config.profile(name).is_none())
        .expect("some suffix is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossz_core::{Binding, ProgramRule};

    fn config() -> Config {
        let mut c = Config::default();
        c.programs.default_profile = "Native".into();
        c.programs.rules.push(ProgramRule {
            exe: "game.exe".into(),
            profile: Some("Native".into()),
            enabled: true,
        });
        c.bindings.push(Binding {
            trigger: "F6".parse().unwrap(),
            action: Action::SetProfile("Native".into()),
        });
        c
    }

    #[test]
    fn rename_updates_references() {
        let mut c = config();
        rename_profile(&mut c, "Native", "Plain");
        c.validate().unwrap();
        assert_eq!(c.programs.default_profile, "Plain");
        assert_eq!(c.programs.rules[1].profile.as_deref(), Some("Plain"));
        assert_eq!(c.bindings[1].action, Action::SetProfile("Plain".into()));
    }

    #[test]
    fn remove_drops_references() {
        let mut c = config();
        remove_profile(&mut c, 0);
        c.validate().unwrap();
        assert_eq!(c.programs.rules[1].profile, None);
        assert_eq!(c.bindings.len(), 1);
    }

    #[test]
    fn unique_names() {
        let c = config();
        assert_eq!(unique_name(&c, "Native"), "Native 2");
        assert_eq!(unique_name(&c, "New"), "New");
    }
}
