use std::collections::{HashMap, HashSet};

use crate::{Action, Chord, Config, Crosshair, Game, Key, Show};

/// What the overlay centers on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The foreground window's client area: a listed game.
    Window,
    /// The monitor the foreground window is on: any other program.
    Monitor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown<'a> {
    pub crosshair: &'a Crosshair,
    /// The game being played, or `Config::other_programs`.
    pub game: &'a Game,
    pub target: Target,
}

/// Decides whether the crosshair is shown and which one, from the config, the
/// foreground program and key/button presses. Toggles and crosshair switches are
/// remembered per game.
#[derive(Debug)]
pub struct Engine {
    config: Config,
    foreground: Option<String>,
    enabled: bool,
    held: HashSet<Key>,
    /// Hold bindings currently held down, with the key that releases them.
    holds: Vec<(Key, Action)>,
    /// Games (by lowercase exe, "" for other programs) toggled off.
    toggled_off: HashSet<String>,
    /// Crosshair picked by a binding, per game.
    switched: HashMap<String, String>,
}

impl Engine {
    pub fn new(config: Config) -> Engine {
        Engine {
            config,
            foreground: None,
            enabled: true,
            held: HashSet::new(),
            holds: Vec::new(),
            toggled_off: HashSet::new(),
            switched: HashMap::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.holds.clear();
        self.switched
            .retain(|_, name| config.crosshair(name).is_some());
        self.config = config;
    }

    /// `exe` is the foreground program's file name, or `None` if it couldn't be determined.
    pub fn set_foreground(&mut self, exe: Option<&str>) {
        self.foreground = exe.map(str::to_ascii_lowercase);
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Feeds a key or mouse button transition. Auto-repeat downs are ignored.
    pub fn key(&mut self, key: Key, down: bool) {
        let key = key.normalized();
        if !down {
            self.held.remove(&key);
            self.holds.retain(|(k, _)| *k != key);
            return;
        }
        if !self.held.insert(key) {
            return;
        }
        let held = &self.held;
        let pressed = |chord: &Chord| chord.key == key && chord.mods_held(|k| held.contains(&k));

        if self
            .config
            .general
            .master_toggle
            .as_ref()
            .is_some_and(pressed)
        {
            self.enabled = !self.enabled;
        }
        if !self.enabled {
            return;
        }
        let Some((game, _)) = self.matched() else {
            return;
        };
        let state_key = game.exe.to_ascii_lowercase();
        let fired: Vec<Action> = (game.bindings.iter())
            .filter(|b| pressed(&b.trigger))
            .map(|b| b.action.clone())
            .collect();
        for action in fired {
            match action {
                Action::Toggle => {
                    if !self.toggled_off.remove(&state_key) {
                        self.toggled_off.insert(state_key.clone());
                    }
                }
                Action::HoldToHide | Action::HoldToShow => self.holds.push((key, action)),
                Action::SetCrosshair(name) => {
                    self.switched.insert(state_key.clone(), name);
                }
            }
        }
    }

    /// The crosshair to show right now, or `None` if it's hidden.
    pub fn shown(&self) -> Option<Shown<'_>> {
        if !self.enabled {
            return None;
        }
        let (game, target) = self.matched()?;
        let state_key = game.exe.to_ascii_lowercase();
        if self.toggled_off.contains(&state_key) {
            return None;
        }
        let holding = |wanted: Action| self.holds.iter().any(|(_, a)| *a == wanted);
        let has_hold_to_show = (game.bindings.iter()).any(|b| b.action == Action::HoldToShow);
        if holding(Action::HoldToHide) || (has_hold_to_show && !holding(Action::HoldToShow)) {
            return None;
        }
        let crosshair = (self.switched.get(&state_key))
            .and_then(|name| self.config.crosshair(name))
            .or_else(|| self.config.crosshair(&game.crosshair))?;
        Some(Shown {
            crosshair,
            game,
            target,
        })
    }

    /// The listed game in the foreground, or other programs, if the crosshair applies there.
    fn matched(&self) -> Option<(&Game, Target)> {
        let config = &self.config;
        let game = (self.foreground.as_deref())
            .and_then(|exe| config.games.iter().find(|g| g.matches(exe)));
        let other = (&config.other_programs, Target::Monitor);
        match (config.general.show, game) {
            (Show::InGames | Show::Everywhere, Some(game)) => Some((game, Target::Window)),
            (Show::InGames, None) | (Show::ExceptListed, Some(_)) => None,
            (Show::ExceptListed | Show::Everywhere, None) => Some(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Binding, Crosshair};

    fn bind(trigger: &str, action: Action) -> Binding {
        Binding {
            trigger: trigger.parse().unwrap(),
            action,
        }
    }

    fn game(exe: &str, crosshair: &str, bindings: Vec<Binding>) -> Game {
        Game {
            crosshair: crosshair.into(),
            bindings,
            ..Game::new(exe)
        }
    }

    fn config(show: Show, games: Vec<Game>) -> Config {
        let mut c = Config::default();
        c.general.show = show;
        c.games = games;
        c.other_programs.crosshair = "Plus".into();
        c.crosshairs = vec![Crosshair::blank("mine")];
        c.validate().unwrap();
        c
    }

    fn key(name: &str) -> Key {
        Key::from_name(name).unwrap()
    }

    fn shown(e: &Engine) -> Option<&str> {
        e.shown().map(|s| s.crosshair.name.as_str())
    }

    fn press(e: &mut Engine, name: &str) {
        e.key(key(name), true);
        e.key(key(name), false);
    }

    #[test]
    fn in_games_shows_only_listed_games() {
        let mut e = Engine::new(config(Show::InGames, vec![game("cs2.exe", "Dot", vec![])]));
        assert_eq!(shown(&e), None);
        e.set_foreground(Some("CS2.EXE"));
        assert_eq!(shown(&e), Some("Dot"));
        assert_eq!(e.shown().unwrap().target, Target::Window);
        e.set_foreground(Some("explorer.exe"));
        assert_eq!(shown(&e), None);
    }

    #[test]
    fn except_listed_hides_listed_games() {
        let mut e = Engine::new(config(
            Show::ExceptListed,
            vec![game("cs2.exe", "Dot", vec![])],
        ));
        e.set_foreground(Some("explorer.exe"));
        assert_eq!(shown(&e), Some("Plus"));
        assert_eq!(e.shown().unwrap().target, Target::Monitor);
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown(&e), None);
        e.set_foreground(None);
        assert_eq!(shown(&e), Some("Plus"));
    }

    #[test]
    fn everywhere_uses_each_games_crosshair() {
        let mut e = Engine::new(config(
            Show::Everywhere,
            vec![game("cs2.exe", "mine", vec![])],
        ));
        e.set_foreground(Some("notepad.exe"));
        assert_eq!(shown(&e), Some("Plus"));
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown(&e), Some("mine"));
    }

    #[test]
    fn disabled_game_is_ignored() {
        let mut g = game("cs2.exe", "Dot", vec![]);
        g.enabled = false;
        let mut e = Engine::new(config(Show::InGames, vec![g]));
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown(&e), None);
    }

    #[test]
    fn rust_hides_while_aiming_down_sights() {
        let rust = game(
            "RustClient.exe",
            "Dot",
            vec![bind("Mouse2", Action::HoldToHide)],
        );
        let mut e = Engine::new(config(Show::InGames, vec![rust]));
        e.set_foreground(Some("RustClient.exe"));
        e.key(Key(0xA0), true); // left shift, sprinting
        e.key(Key::MOUSE2, true);
        assert_eq!(shown(&e), None);
        e.key(Key(0xA0), false);
        assert_eq!(shown(&e), None);
        e.key(Key::MOUSE2, false);
        assert_eq!(shown(&e), Some("Dot"));
    }

    #[test]
    fn bindings_belong_to_their_game() {
        let rust = game(
            "RustClient.exe",
            "Dot",
            vec![bind("Mouse2", Action::HoldToHide)],
        );
        let cs2 = game("cs2.exe", "Dot", vec![]);
        let mut e = Engine::new(config(Show::InGames, vec![rust, cs2]));
        e.set_foreground(Some("cs2.exe"));
        e.key(Key::MOUSE2, true);
        assert_eq!(shown(&e), Some("Dot"));
    }

    #[test]
    fn toggle_ignores_auto_repeat_and_is_per_game() {
        let toggle = || vec![bind("F8", Action::Toggle)];
        let a = game("a.exe", "Dot", toggle());
        let b = game("b.exe", "Dot", toggle());
        let mut e = Engine::new(config(Show::InGames, vec![a, b]));
        e.set_foreground(Some("a.exe"));
        e.key(key("F8"), true);
        e.key(key("F8"), true);
        e.key(key("F8"), true);
        e.key(key("F8"), false);
        assert_eq!(shown(&e), None);
        e.set_foreground(Some("b.exe"));
        assert_eq!(shown(&e), Some("Dot"));
        e.set_foreground(Some("a.exe"));
        press(&mut e, "F8");
        assert_eq!(shown(&e), Some("Dot"));
    }

    #[test]
    fn hold_to_show_hides_until_held() {
        let g = game("a.exe", "Dot", vec![bind("Mouse5", Action::HoldToShow)]);
        let mut e = Engine::new(config(Show::InGames, vec![g]));
        e.set_foreground(Some("a.exe"));
        assert_eq!(shown(&e), None);
        e.key(Key::MOUSE5, true);
        assert_eq!(shown(&e), Some("Dot"));
        e.key(Key::MOUSE5, false);
        assert_eq!(shown(&e), None);
    }

    #[test]
    fn master_toggle_needs_modifiers_and_works_everywhere() {
        let mut e = Engine::new(config(Show::InGames, vec![game("cs2.exe", "Dot", vec![])]));
        e.set_foreground(Some("explorer.exe"));
        press(&mut e, "X");
        assert!(e.enabled());
        e.key(Key(0xA2), true); // left ctrl
        e.key(Key(0xA5), true); // right alt
        press(&mut e, "X");
        assert!(!e.enabled());
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown(&e), None);
    }

    #[test]
    fn set_crosshair_is_per_game_and_survives_valid_config_changes() {
        let g = game(
            "a.exe",
            "Dot",
            vec![bind("F6", Action::SetCrosshair("mine".into()))],
        );
        let mut e = Engine::new(config(Show::Everywhere, vec![g.clone()]));
        e.set_foreground(Some("a.exe"));
        press(&mut e, "F6");
        assert_eq!(shown(&e), Some("mine"));
        e.set_foreground(Some("notepad.exe"));
        assert_eq!(shown(&e), Some("Plus"));
        e.set_config(config(Show::Everywhere, vec![g.clone()]));
        e.set_foreground(Some("a.exe"));
        assert_eq!(shown(&e), Some("mine"));

        let mut without_mine = config(Show::Everywhere, vec![]);
        without_mine.crosshairs.clear();
        without_mine.games = vec![game("a.exe", "Dot", vec![])];
        e.set_config(without_mine);
        assert_eq!(shown(&e), Some("Dot"));
    }
}
