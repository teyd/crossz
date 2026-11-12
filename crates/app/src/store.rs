use std::path::PathBuf;

use crossz_core::{Action, Config, Crosshair, Engine, Game, Status, Visibility, presets};
use gpui_kit::{App, Global};

/// The config being edited, shared by every part of the UI. The file on disk is the
/// source of truth: edits are saved immediately and the overlay picks them up from there.
pub struct Store {
    pub config: Config,
    pub path: PathBuf,
    /// Why the last edit or file change wasn't applied.
    pub error: Option<String>,
    /// The game the preview shows: an index into `config.games`, or `None` for the
    /// setup outside listed games. Only listed games when showing in games.
    pub selected_game: Option<usize>,
    /// The main display's resolution, which the preview draws for.
    pub screen: (u32, u32),
    /// Binds can't work until the user allows input access (macOS Input Monitoring).
    pub input_unavailable: bool,
    /// What the overlay did in the last program used other than crossz itself.
    status: Option<Status>,
    /// The master toggle.
    enabled: bool,
}

impl Global for Store {}

impl Store {
    pub fn new(config: Config, path: PathBuf) -> Store {
        let mut store = Store {
            config,
            path,
            error: None,
            selected_game: None,
            screen: crate::overlay::main_display_size(),
            input_unavailable: false,
            status: None,
            enabled: true,
        };
        store.clamp_selection();
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

    /// The crosshair the previewed game uses, falling back to the first built-in one.
    pub fn crosshair(&self) -> &Crosshair {
        (self.config.crosshair_for(self.game())).unwrap_or(&presets::all()[0])
    }

    /// The crosshair in use, unless a game picks its own.
    pub fn active(&self) -> &Crosshair {
        (self.config.crosshair(&self.config.general.crosshair)).unwrap_or(&presets::all()[0])
    }

    pub fn active_is_custom(&self) -> bool {
        (self.config.crosshairs.iter()).any(|c| c.name == self.config.general.crosshair)
    }

    /// Listed games that use their own crosshair instead of the selected one.
    pub fn games_with_own_crosshair(&self) -> Vec<&Game> {
        (self.config.games.iter())
            .filter(|g| g.enabled && g.crosshair.is_some())
            .collect()
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
        store.clamp_selection();
        cx.refresh_windows();
    }

    /// Makes `name` the crosshair in use. If the previewed game has its own crosshair,
    /// previews one that follows the selection instead, so the change is visible.
    pub fn use_crosshair(cx: &mut App, name: String) {
        Store::edit(cx, |c| c.general.crosshair = name);
        let store = cx.global_mut::<Store>();
        if store.game().crosshair.is_some()
            && let Some(i) = (store.config.games.iter()).position(|g| g.crosshair.is_none())
        {
            store.selected_game = Some(i);
            store.clamp_selection();
        }
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

    /// Edits the crosshair in use if it's a custom one; built-in ones can't change.
    pub fn edit_crosshair(cx: &mut App, change: impl FnOnce(&mut Crosshair)) {
        let name = Store::get(cx).config.general.crosshair.clone();
        Store::edit(cx, |config| {
            if let Some(c) = config.crosshairs.iter_mut().find(|c| c.name == name) {
                change(c);
            }
        });
    }

    /// Keeps the overlay's status, except while crossz itself is in front: the settings
    /// window should describe the game you came from.
    pub fn set_status(cx: &mut App, status: Status) {
        let own = std::env::current_exe().ok();
        let own = own
            .as_deref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        let store = cx.global_mut::<Store>();
        store.enabled = status.visibility != Visibility::Off;
        if own.is_none() || status.program.as_deref() != own {
            store.status = Some(status);
        }
        cx.refresh_windows();
    }

    /// What the overlay does in the last program used, with the config as it is now:
    /// it only reports changes while that program is in front.
    pub fn status(&self) -> Option<Status> {
        if !cfg!(any(windows, target_os = "macos")) {
            return None;
        }
        let reported = self.status.as_ref();
        if let Some(reported) = reported
            && reported.visibility == Visibility::Hidden
        {
            return Some(reported.clone());
        }
        let program = reported.and_then(|r| r.program.as_deref());
        let mut engine = Engine::new(self.config.clone());
        engine.set_foreground(program);
        engine.set_enabled(self.enabled);
        Some(engine.status())
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

    /// Keeps the previewed game meaningful for the current mode: a listed game when
    /// showing in games, otherwise the setup outside them.
    fn clamp_selection(&mut self) {
        let in_games = self.config.general.show == crossz_core::Show::InGames;
        let games = self.config.games.len();
        self.selected_game = match self.selected_game {
            _ if !in_games => None,
            Some(i) if i < games => Some(i),
            // Prefer a game that shows the selected crosshair.
            _ => (self.config.games.iter())
                .position(|g| g.crosshair.is_none())
                .or(games.checked_sub(1).map(|_| 0)),
        };
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

/// Removes a custom crosshair. Games that picked it use the one in use instead; if it
/// was the one in use, the first built-in one takes over.
pub fn remove_crosshair(config: &mut Config, name: &str) {
    config.crosshairs.retain(|c| c.name != name);
    let fallback = presets::all()[0].name.clone();
    if config.general.crosshair == name {
        config.general.crosshair = fallback.clone();
    }
    for game in config.games.iter_mut().chain([&mut config.other_programs]) {
        if game.crosshair.as_deref() == Some(name) {
            game.crosshair = None;
        }
        for binding in &mut game.bindings {
            if let Action::SetCrosshair(target) = &mut binding.action
                && target == name
            {
                *target = fallback.clone();
            }
        }
    }
}

fn replace_references(config: &mut Config, from: &str, to: &str) {
    if config.general.crosshair == from {
        config.general.crosshair = to.to_owned();
    }
    for game in config.games.iter_mut().chain([&mut config.other_programs]) {
        if game.crosshair.as_deref() == Some(from) {
            game.crosshair = Some(to.to_owned());
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
        c.games[0].crosshair = Some("mine".into());
        c.games[1].crosshair = Some("Plus".into());
        c.general.crosshair = "mine".into();
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
        assert_eq!(c.games[0].crosshair.as_deref(), Some("ours"));
        assert_eq!(c.general.crosshair, "ours");
        assert_eq!(
            c.other_programs.bindings[0].action,
            Action::SetCrosshair("ours".into())
        );
    }

    #[test]
    fn remove_falls_back_to_the_active_crosshair() {
        let mut c = config();
        remove_crosshair(&mut c, "mine");
        c.validate().unwrap();
        assert_eq!(c.games[0].crosshair, None);
        assert_eq!(c.games[1].crosshair.as_deref(), Some("Plus"));
        assert_eq!(c.general.crosshair, presets::all()[0].name);
    }

    #[test]
    fn unique_names_skip_presets_and_customs() {
        let c = config();
        assert_eq!(unique_name(&c, "Dot"), "Dot 2");
        assert_eq!(unique_name(&c, "mine"), "mine 2");
        assert_eq!(unique_name(&c, "New"), "New");
    }
}
