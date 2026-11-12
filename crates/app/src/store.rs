use std::path::PathBuf;

use crossz_core::{Action, Config, Crosshair, Game, presets};
use gpui_kit::{App, Global};

/// The config being edited, shared by every part of the UI. The file on disk is the
/// source of truth: edits are saved immediately and the overlay picks them up from there.
pub struct Store {
    pub config: Config,
    pub path: PathBuf,
    /// Why the last edit or file change wasn't applied.
    pub error: Option<String>,
    /// Game being edited: an index into `config.games`, or `None` for other programs.
    pub selected_game: Option<usize>,
    /// Crosshair shown in the preview and the crosshair editor.
    pub selected_crosshair: String,
    /// The main display's resolution, which the preview draws for.
    pub screen: (u32, u32),
    /// Binds can't work until the user allows input access (macOS Input Monitoring).
    pub input_unavailable: bool,
}

impl Global for Store {}

impl Store {
    pub fn new(config: Config, path: PathBuf) -> Store {
        let selected_game = (!config.games.is_empty()).then_some(0);
        let mut store = Store {
            config,
            path,
            error: None,
            selected_game,
            selected_crosshair: String::new(),
            screen: crate::overlay::main_display_size(),
            input_unavailable: false,
        };
        store.selected_crosshair = store.game().crosshair.clone();
        store
    }

    pub fn get(cx: &App) -> &Store {
        cx.global::<Store>()
    }

    pub fn game(&self) -> &Game {
        (self.selected_game)
            .and_then(|i| self.config.games.get(i))
            .unwrap_or(&self.config.other_programs)
    }

    /// The selected crosshair, falling back to the first built-in one.
    pub fn crosshair(&self) -> &Crosshair {
        (self.config.crosshair(&self.selected_crosshair)).unwrap_or(&presets::all()[0])
    }

    pub fn crosshair_is_custom(&self) -> bool {
        (self.config.crosshairs.iter()).any(|c| c.name == self.selected_crosshair)
    }

    /// Where crossz shows a crosshair while this game is in front, as a short label.
    pub fn game_label(&self) -> String {
        match self.selected_game {
            Some(_) => self.game().display_name().to_owned(),
            None if self.config.general.show == crossz_core::Show::Everywhere => {
                "all programs".to_owned()
            }
            None => "other programs".to_owned(),
        }
    }

    pub fn select_game(cx: &mut App, game: Option<usize>) {
        let store = cx.global_mut::<Store>();
        store.selected_game = game;
        store.selected_crosshair = store.game().crosshair.clone();
        cx.refresh_windows();
    }

    pub fn select_crosshair(cx: &mut App, name: String) {
        cx.global_mut::<Store>().selected_crosshair = name;
        cx.refresh_windows();
    }

    /// Applies `change` to the config, then validates and saves it. An invalid result is
    /// kept in the editor but not saved, with the reason in `error`.
    pub fn edit(cx: &mut App, change: impl FnOnce(&mut Config)) {
        let mut config = Store::get(cx).config.clone();
        change(&mut config);
        if config == Store::get(cx).config {
            return;
        }
        let result = config
            .validate()
            .and_then(|()| config.save(&Store::get(cx).path));
        let store = cx.global_mut::<Store>();
        store.error = result.err().map(|e| e.to_string());
        store.config = config;
        store.clamp_selection();
        crate::overlay::apply(cx);
        cx.refresh_windows();
    }

    pub fn edit_game(cx: &mut App, change: impl FnOnce(&mut Game)) {
        let selected = Store::get(cx).selected_game;
        Store::edit(cx, |config| {
            match selected.and_then(|i| config.games.get_mut(i)) {
                Some(game) => change(game),
                None => change(&mut config.other_programs),
            }
        });
    }

    /// Edits the selected crosshair if it's a custom one; built-in ones can't change.
    pub fn edit_crosshair(cx: &mut App, change: impl FnOnce(&mut Crosshair)) {
        let name = Store::get(cx).selected_crosshair.clone();
        Store::edit(cx, |config| {
            if let Some(c) = config.crosshairs.iter_mut().find(|c| c.name == name) {
                change(c);
            }
        });
    }

    /// Takes a config that changed on disk, e.g. edited by hand.
    pub fn reload(cx: &mut App, config: Config) {
        let store = cx.global_mut::<Store>();
        store.error = None;
        if store.config != config {
            store.config = config;
            store.clamp_selection();
            crate::overlay::apply(cx);
        }
        cx.refresh_windows();
    }

    pub fn report(cx: &mut App, error: String) {
        cx.global_mut::<Store>().error = Some(error);
        cx.refresh_windows();
    }

    fn clamp_selection(&mut self) {
        if self
            .selected_game
            .is_some_and(|i| i >= self.config.games.len())
        {
            self.selected_game = self.config.games.len().checked_sub(1);
        }
        if self.config.crosshair(&self.selected_crosshair).is_none() {
            self.selected_crosshair = self.game().crosshair.clone();
        }
    }
}

/// Renames a custom crosshair and every reference to it.
pub fn rename_crosshair(config: &mut Config, from: &str, to: &str) {
    for c in &mut config.crosshairs {
        if c.name == from {
            c.name = to.to_owned();
        }
    }
    replace_references(config, from, to);
}

/// Removes a custom crosshair; games using it fall back to the first built-in one.
pub fn remove_crosshair(config: &mut Config, name: &str) {
    config.crosshairs.retain(|c| c.name != name);
    let fallback = presets::all()[0].name.clone();
    replace_references(config, name, &fallback);
}

fn replace_references(config: &mut Config, from: &str, to: &str) {
    for game in config.games.iter_mut().chain([&mut config.other_programs]) {
        if game.crosshair == from {
            game.crosshair = to.to_owned();
        }
        for binding in &mut game.bindings {
            if let Action::SetCrosshair(name) = &mut binding.action
                && name == from
            {
                *name = to.to_owned();
            }
        }
    }
}

/// `base`, or `base 2`, `base 3`… whichever no crosshair is called yet.
pub fn unique_name(config: &Config, base: &str) -> String {
    (1..)
        .map(|n| match n {
            1 => base.to_owned(),
            n => format!("{base} {n}"),
        })
        .find(|name| config.crosshair(name).is_none())
        .expect("some suffix is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossz_core::Binding;

    fn config() -> Config {
        let mut c = Config::default();
        c.crosshairs
            .push(Crosshair::pixels("mine", crossz_core::Pixels::new(3, 3)));
        c.games[0].crosshair = "mine".into();
        c.other_programs.bindings.push(Binding {
            trigger: "F6".parse().unwrap(),
            action: Action::SetCrosshair("mine".into()),
        });
        c.validate().unwrap();
        c
    }

    #[test]
    fn rename_updates_references() {
        let mut c = config();
        rename_crosshair(&mut c, "mine", "ours");
        c.validate().unwrap();
        assert_eq!(c.games[0].crosshair, "ours");
        assert_eq!(
            c.other_programs.bindings[0].action,
            Action::SetCrosshair("ours".into())
        );
    }

    #[test]
    fn remove_falls_back_to_a_preset() {
        let mut c = config();
        remove_crosshair(&mut c, "mine");
        c.validate().unwrap();
        assert_eq!(c.games[0].crosshair, presets::all()[0].name);
    }

    #[test]
    fn unique_names_skip_presets_and_customs() {
        let c = config();
        assert_eq!(unique_name(&c, "Dot"), "Dot 2");
        assert_eq!(unique_name(&c, "mine"), "mine 2");
        assert_eq!(unique_name(&c, "New"), "New");
    }
}
