use std::collections::{HashMap, HashSet};

use crate::{Action, Config, Key, Profile, ProgramMode};

/// What the overlay centers on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The foreground window's client area: a program matched by a rule.
    Window,
    /// The monitor the foreground window is on: shown without a specific rule.
    Monitor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown<'a> {
    pub profile: &'a Profile,
    pub target: Target,
}

/// Decides whether the crosshair is shown and with which profile, from the config,
/// the foreground program and key/button presses.
#[derive(Debug)]
pub struct Engine {
    config: Config,
    foreground: Option<String>,
    enabled: bool,
    toggled_off: bool,
    held: HashSet<Key>,
    /// Indices into `config.bindings` of hold bindings whose key is down.
    holds: HashSet<usize>,
    /// Profile picked by a binding, per executable.
    overrides: HashMap<String, String>,
}

impl Engine {
    pub fn new(config: Config) -> Engine {
        Engine {
            config,
            foreground: None,
            enabled: true,
            toggled_off: false,
            held: HashSet::new(),
            holds: HashSet::new(),
            overrides: HashMap::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn set_config(&mut self, config: Config) {
        self.holds.clear();
        self.overrides
            .retain(|_, profile| config.profile(profile).is_some());
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
            let bindings = &self.config.bindings;
            self.holds.retain(|&i| bindings[i].trigger.key != key);
            return;
        }
        if !self.held.insert(key) {
            return;
        }
        let held = &self.held;
        let pressed =
            |chord: &crate::Chord| chord.key == key && chord.mods_held(|k| held.contains(&k));

        if self
            .config
            .general
            .master_toggle
            .as_ref()
            .is_some_and(pressed)
        {
            self.enabled = !self.enabled;
        }
        if !self.enabled || self.matched().is_none() {
            return;
        }
        let fired: Vec<(usize, Action)> = (self.config.bindings.iter().enumerate())
            .filter(|(_, b)| pressed(&b.trigger))
            .map(|(i, b)| (i, b.action.clone()))
            .collect();
        for (i, action) in fired {
            match action {
                Action::Toggle => self.toggled_off = !self.toggled_off,
                Action::HoldToHide | Action::HoldToShow => {
                    self.holds.insert(i);
                }
                Action::CycleProfile => {
                    let profiles = &self.config.profiles;
                    if let Some(current) = self.profile()
                        && let Some(pos) = profiles.iter().position(|p| p.name == current.name)
                    {
                        let next = profiles[(pos + 1) % profiles.len()].name.clone();
                        self.set_override(next);
                    }
                }
                Action::SetProfile(name) => self.set_override(name),
            }
        }
    }

    /// The crosshair to show right now, or `None` if it's hidden.
    pub fn shown(&self) -> Option<Shown<'_>> {
        if !self.enabled || self.toggled_off {
            return None;
        }
        let (_, target) = self.matched()?;
        let holding = |wanted: &Action| {
            (self.holds.iter()).any(|&i| self.config.bindings[i].action == *wanted)
        };
        let has_hold_to_show =
            (self.config.bindings.iter()).any(|b| b.action == Action::HoldToShow);
        if holding(&Action::HoldToHide) || (has_hold_to_show && !holding(&Action::HoldToShow)) {
            return None;
        }
        Some(Shown {
            profile: self.profile()?,
            target,
        })
    }

    /// The active profile for the foreground program, whether or not it's shown.
    pub fn profile(&self) -> Option<&Profile> {
        let (rule_profile, _) = self.matched()?;
        let overridden = (self.foreground.as_ref())
            .and_then(|exe| self.overrides.get(exe))
            .and_then(|name| self.config.profile(name));
        overridden
            .or_else(|| rule_profile.and_then(|name| self.config.profile(name)))
            .or_else(|| self.config.default_profile())
    }

    fn set_override(&mut self, profile: String) {
        if let Some(exe) = &self.foreground {
            self.overrides.insert(exe.clone(), profile);
        }
    }

    /// Whether the foreground program gets a crosshair, and the profile its rule names.
    fn matched(&self) -> Option<(Option<&str>, Target)> {
        let programs = &self.config.programs;
        let rule = (self.foreground.as_deref())
            .and_then(|exe| programs.rules.iter().find(|r| r.matches(exe)));
        let by_rule = rule.map(|r| (r.profile.as_deref(), Target::Window));
        match programs.mode {
            ProgramMode::Whitelist => by_rule,
            ProgramMode::Blacklist if rule.is_some() => None,
            ProgramMode::Blacklist => Some((None, Target::Monitor)),
            ProgramMode::Always => by_rule.or(Some((None, Target::Monitor))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Binding, Chord, Crosshair, ProgramRule, ScaleMode, Snap};

    fn profile(name: &str) -> Profile {
        Profile {
            name: name.into(),
            scale: ScaleMode::Native,
            snap: Snap::Round,
            offset: [0, 0],
            crosshair: Crosshair::default(),
        }
    }

    fn rule(exe: &str, profile: Option<&str>) -> ProgramRule {
        ProgramRule {
            exe: exe.into(),
            profile: profile.map(Into::into),
            enabled: true,
        }
    }

    fn bind(trigger: &str, action: Action) -> Binding {
        Binding {
            trigger: trigger.parse().unwrap(),
            action,
        }
    }

    fn config(mode: ProgramMode, bindings: Vec<Binding>) -> Config {
        let mut c = Config::default();
        c.programs.mode = mode;
        c.programs.default_profile = "native".into();
        c.programs.rules = vec![rule("cs2.exe", Some("stretched")), rule("game.exe", None)];
        c.profiles = vec![profile("native"), profile("stretched"), profile("third")];
        c.bindings = bindings;
        c.validate().unwrap();
        c
    }

    fn key(name: &str) -> Key {
        Key::from_name(name).unwrap()
    }

    fn shown_name(e: &Engine) -> Option<&str> {
        e.shown().map(|s| s.profile.name.as_str())
    }

    fn press(e: &mut Engine, name: &str) {
        e.key(key(name), true);
        e.key(key(name), false);
    }

    #[test]
    fn whitelist_shows_only_listed_programs() {
        let mut e = Engine::new(config(ProgramMode::Whitelist, vec![]));
        assert_eq!(shown_name(&e), None);
        e.set_foreground(Some("CS2.EXE"));
        assert_eq!(shown_name(&e), Some("stretched"));
        assert_eq!(e.shown().unwrap().target, Target::Window);
        e.set_foreground(Some("game.exe"));
        assert_eq!(shown_name(&e), Some("native"));
        e.set_foreground(Some("explorer.exe"));
        assert_eq!(shown_name(&e), None);
    }

    #[test]
    fn blacklist_hides_listed_programs() {
        let mut e = Engine::new(config(ProgramMode::Blacklist, vec![]));
        e.set_foreground(Some("explorer.exe"));
        assert_eq!(e.shown().unwrap().target, Target::Monitor);
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), None);
        e.set_foreground(None);
        assert_eq!(shown_name(&e), Some("native"));
    }

    #[test]
    fn always_mode_still_applies_rule_profiles() {
        let mut e = Engine::new(config(ProgramMode::Always, vec![]));
        e.set_foreground(Some("notepad.exe"));
        assert_eq!(shown_name(&e), Some("native"));
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), Some("stretched"));
    }

    #[test]
    fn disabled_rule_is_ignored() {
        let mut c = config(ProgramMode::Whitelist, vec![]);
        c.programs.rules[0].enabled = false;
        let mut e = Engine::new(c);
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), None);
    }

    #[test]
    fn toggle_ignores_auto_repeat() {
        let mut e = Engine::new(config(
            ProgramMode::Always,
            vec![bind("F8", Action::Toggle)],
        ));
        e.key(key("F8"), true);
        e.key(key("F8"), true);
        e.key(key("F8"), true);
        assert_eq!(shown_name(&e), None);
        e.key(key("F8"), false);
        press(&mut e, "F8");
        assert_eq!(shown_name(&e), Some("native"));
    }

    #[test]
    fn hold_to_hide_works_with_extra_modifiers() {
        let mut e = Engine::new(config(
            ProgramMode::Always,
            vec![bind("Mouse2", Action::HoldToHide)],
        ));
        e.key(Key(0xA0), true); // left shift, e.g. walking
        e.key(Key::MOUSE2, true);
        assert_eq!(shown_name(&e), None);
        e.key(Key(0xA0), false);
        assert_eq!(shown_name(&e), None);
        e.key(Key::MOUSE2, false);
        assert_eq!(shown_name(&e), Some("native"));
    }

    #[test]
    fn hold_to_show_hides_until_held() {
        let mut e = Engine::new(config(
            ProgramMode::Always,
            vec![bind("Mouse5", Action::HoldToShow)],
        ));
        assert_eq!(shown_name(&e), None);
        e.key(Key::MOUSE5, true);
        assert_eq!(shown_name(&e), Some("native"));
        e.key(Key::MOUSE5, false);
        assert_eq!(shown_name(&e), None);
    }

    #[test]
    fn bindings_only_fire_in_matched_programs() {
        let mut e = Engine::new(config(
            ProgramMode::Whitelist,
            vec![bind("F8", Action::Toggle)],
        ));
        e.set_foreground(Some("explorer.exe"));
        press(&mut e, "F8");
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), Some("stretched"));
    }

    #[test]
    fn master_toggle_needs_modifiers_and_works_everywhere() {
        let mut e = Engine::new(config(ProgramMode::Whitelist, vec![]));
        e.set_foreground(Some("explorer.exe"));
        press(&mut e, "X");
        assert!(e.enabled());
        e.key(Key(0xA2), true); // left ctrl
        e.key(Key(0xA5), true); // right alt
        press(&mut e, "X");
        assert!(!e.enabled());
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), None);
    }

    #[test]
    fn cycle_and_set_profile_are_per_program() {
        let mut e = Engine::new(config(
            ProgramMode::Always,
            vec![
                bind("F6", Action::CycleProfile),
                bind("F7", Action::SetProfile("native".into())),
            ],
        ));
        e.set_foreground(Some("cs2.exe"));
        press(&mut e, "F6");
        assert_eq!(shown_name(&e), Some("third"));
        press(&mut e, "F6");
        assert_eq!(shown_name(&e), Some("native"));
        e.set_foreground(Some("notepad.exe"));
        assert_eq!(shown_name(&e), Some("native"));
        e.set_foreground(Some("cs2.exe"));
        assert_eq!(shown_name(&e), Some("native"));
        press(&mut e, "F7");
        assert_eq!(shown_name(&e), Some("native"));
    }

    #[test]
    fn new_config_drops_stale_overrides_and_holds() {
        let mut e = Engine::new(config(
            ProgramMode::Always,
            vec![
                bind("F7", Action::SetProfile("third".into())),
                bind("Mouse2", Action::HoldToHide),
            ],
        ));
        e.set_foreground(Some("cs2.exe"));
        press(&mut e, "F7");
        e.key(Key::MOUSE2, true);
        let mut c = config(ProgramMode::Always, vec![]);
        c.profiles.pop();
        e.set_config(c);
        assert_eq!(shown_name(&e), Some("stretched"));
    }

    #[test]
    fn chord_display_round_trips_through_bindings() {
        let c: Chord = "Ctrl+F1".parse().unwrap();
        assert_eq!(bind("Ctrl+F1", Action::Toggle).trigger, c);
    }
}
