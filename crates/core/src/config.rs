use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Chord, Crosshair, Resolution, presets};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    /// Used outside listed games: every other program with `ExceptListed`, every
    /// program with `Everywhere`.
    #[serde(default = "Game::other_programs")]
    pub other_programs: Game,
    #[serde(default, rename = "game", skip_serializing_if = "Vec::is_empty")]
    pub games: Vec<Game>,
    /// Custom crosshairs. Built-in ones come from `presets`.
    #[serde(default, rename = "crosshair", skip_serializing_if = "Vec::is_empty")]
    pub crosshairs: Vec<Crosshair>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub show: Show,
    /// The crosshair in use, unless a game picks its own.
    pub crosshair: String,
    /// Shows/hides crossz everywhere, in any program.
    pub master_toggle: Option<Chord>,
    /// Hides the overlay from screenshots, OBS and other capture.
    pub exclude_from_capture: bool,
    pub start_with_windows: bool,
    /// Start in the tray without opening the settings window.
    pub start_minimized: bool,
    pub check_for_updates: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            show: Show::default(),
            crosshair: presets::all()[0].name.clone(),
            master_toggle: "Ctrl+Alt+X".parse().ok(),
            exclude_from_capture: false,
            start_with_windows: true,
            start_minimized: true,
            check_for_updates: true,
        }
    }
}

/// Where the crosshair shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Show {
    /// Only while a listed game is in the foreground.
    #[default]
    InGames,
    /// Everywhere except while a listed game is in the foreground.
    ExceptListed,
    /// Everywhere, with `other_programs`' settings; the games list isn't used.
    Everywhere,
}

/// How crossz behaves in one program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Game {
    /// Executable file name, e.g. `RustClient.exe`, compared case-insensitively.
    /// Empty for `Config::other_programs`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exe: String,
    /// Display name, e.g. the window title when it was added.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Full path when known, to show the program's icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default = "enabled", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// A crosshair just for this game; `None` uses `general.crosshair`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crosshair: Option<String>,
    /// Makes the crosshair look like it would at this resolution, stretched over the
    /// screen. The game's real resolution doesn't matter.
    #[serde(default)]
    pub resolution: Resolution,
    /// Screen-pixel nudge from the center, `[x, y]`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: [i32; 2],
    #[serde(default, rename = "binding", skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<Binding>,
}

impl Game {
    pub fn new(exe: impl Into<String>) -> Game {
        Game {
            exe: exe.into(),
            name: String::new(),
            path: None,
            enabled: true,
            crosshair: None,
            resolution: Resolution::Native,
            offset: [0, 0],
            bindings: Vec::new(),
        }
    }

    pub fn other_programs() -> Game {
        Game::new("")
    }

    pub fn matches(&self, exe: &str) -> bool {
        self.enabled && self.exe.eq_ignore_ascii_case(exe)
    }

    pub fn display_name(&self) -> &str {
        if self.name.is_empty() {
            &self.exe
        } else {
            &self.name
        }
    }
}

fn enabled() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_zero(o: &[i32; 2]) -> bool {
    *o == [0, 0]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub trigger: Chord,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Toggle,
    HoldToHide,
    HoldToShow,
    SetCrosshair(String),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("{0}")]
    Invalid(String),
}

impl Default for Config {
    fn default() -> Self {
        let mut rust = Game::new("RustClient.exe");
        rust.name = "Rust".into();
        rust.bindings.push(Binding {
            trigger: Chord::key(crate::Key::MOUSE2),
            action: Action::HoldToHide,
        });
        let mut cs2 = Game::new("cs2.exe");
        cs2.name = "Counter-Strike 2".into();
        cs2.resolution = Resolution::custom(1280, 960);
        Config {
            general: General::default(),
            other_programs: Game::other_programs(),
            games: vec![rust, cs2],
            crosshairs: Vec::new(),
        }
    }
}

impl Config {
    pub fn parse(text: &str, path: &Path) -> Result<Config, Error> {
        let mut config: Config = toml::from_str(text).map_err(|e| Error::Parse {
            path: path.to_owned(),
            source: Box::new(e),
        })?;
        // Before `general.crosshair` existed, other programs had their own crosshair; that
        // one becomes the selected crosshair.
        let has_selected = (text.parse::<toml::Table>().ok())
            .is_some_and(|t| t.get("general").and_then(|g| g.get("crosshair")).is_some());
        if !has_selected && let Some(name) = config.other_programs.crosshair.take() {
            config.general.crosshair = name;
        }
        config.validate()?;
        Ok(config)
    }

    /// Loads `path`, writing the default config there first if it doesn't exist.
    /// The flag says whether the file was just created.
    pub fn load_or_create(path: &Path) -> Result<(Config, bool), Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok((Config::parse(&text, path)?, false)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Config::default();
                config.save(path)?;
                Ok((config, true))
            }
            Err(source) => Err(Error::Io {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// Writes via a temp file and rename, so a crash or a watcher never sees half a file.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let io_err = |source| Error::Io {
            path: path.to_owned(),
            source,
        };
        let text = toml::to_string_pretty(self).expect("config always serializes");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_err)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(io_err)?;
        std::fs::rename(&tmp, path).map_err(io_err)
    }

    pub fn validate(&self) -> Result<(), Error> {
        let invalid = |message: String| Err(Error::Invalid(message));
        let mut names = HashSet::new();
        for c in &self.crosshairs {
            if c.name.trim().is_empty() {
                return invalid("a crosshair has no name".into());
            }
            if presets::is_preset(&c.name) {
                return invalid(format!("{:?} is the name of a built-in crosshair", c.name));
            }
            if !names.insert(c.name.as_str()) {
                return invalid(format!("two crosshairs are named {:?}", c.name));
            }
        }
        let mut exes = HashSet::new();
        for game in &self.games {
            if game.exe.trim().is_empty() {
                return invalid("a [[game]] has no exe".into());
            }
            if !exes.insert(game.exe.to_ascii_lowercase()) {
                return invalid(format!("{} is listed twice", game.exe));
            }
        }
        let known = |name: &str| self.crosshair(name).is_some();
        if !known(&self.general.crosshair) {
            return invalid(format!(
                "general.crosshair is the unknown crosshair {:?}",
                self.general.crosshair
            ));
        }
        for game in self.games.iter().chain([&self.other_programs]) {
            let place = if game.exe.is_empty() {
                "other_programs".to_owned()
            } else {
                game.exe.clone()
            };
            if let Some(name) = &game.crosshair
                && !known(name)
            {
                return invalid(format!("{place} uses unknown crosshair {name:?}"));
            }
            for b in &game.bindings {
                if let Action::SetCrosshair(name) = &b.action
                    && !known(name)
                {
                    return invalid(format!(
                        "binding {} in {place} uses unknown crosshair {name:?}",
                        b.trigger
                    ));
                }
            }
        }
        Ok(())
    }

    /// The crosshair a game uses: its own, or the one in use everywhere.
    pub fn crosshair_for(&self, game: &Game) -> Option<&Crosshair> {
        (game.crosshair.as_deref())
            .and_then(|name| self.crosshair(name))
            .or_else(|| self.crosshair(&self.general.crosshair))
    }

    /// A custom crosshair, or a built-in one.
    pub fn crosshair(&self, name: &str) -> Option<&Crosshair> {
        (self.crosshairs.iter())
            .find(|c| c.name == name)
            .or_else(|| presets::get(name))
    }

    /// Every crosshair a game can use: built-in ones first, then custom ones.
    pub fn all_crosshairs(&self) -> impl Iterator<Item = &Crosshair> {
        presets::all().iter().chain(&self.crosshairs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config, Error> {
        Config::parse(text, Path::new("test.toml"))
    }

    #[test]
    fn default_round_trips() {
        let config = Config::default();
        let text = toml::to_string_pretty(&config).unwrap();
        assert_eq!(parse(&text).unwrap(), config);
    }

    #[test]
    fn empty_file_is_valid() {
        let config = parse("").unwrap();
        assert!(config.games.is_empty());
        assert_eq!(config.general.crosshair, "Plus");
        assert_eq!(config.other_programs.crosshair, None);
    }

    #[test]
    fn reads_handwritten_config() {
        let config = parse(
            r##"
            [general]
            show = "everywhere"
            master_toggle = "Ctrl+Shift+F1"

            [other_programs]
            crosshair = "Dot"

            [[game]]
            exe = "RustClient.exe"
            crosshair = "mine"
            resolution = "1280x960"

            [[game.binding]]
            trigger = "Mouse2"
            action = "hold_to_hide"

            [[game.binding]]
            trigger = "F6"
            action = { set_crosshair = "Plus" }

            [[crosshair]]
            name = "mine"
            zoom = 1.5

            [crosshair.pixels]
            palette = ["#ff00ff"]
            rows = [".0.", "000", ".0."]
            "##,
        )
        .unwrap();
        assert_eq!(config.general.show, Show::Everywhere);
        let rust = &config.games[0];
        assert!(rust.enabled);
        assert_eq!(rust.resolution, Resolution::custom(1280, 960));
        assert_eq!(rust.bindings[1].action, Action::SetCrosshair("Plus".into()));
        let mine = config.crosshair("mine").unwrap();
        assert_eq!((mine.opacity, mine.zoom), (1.0, 1.5));
        assert_eq!(mine.pixels.as_ref().unwrap().width(), 3);
        assert_eq!(config.crosshair("Dot").unwrap().name, "Dot");
    }

    #[test]
    fn rejects_unknown_crosshair() {
        let err = parse("[[game]]\nexe = \"a.exe\"\ncrosshair = \"nope\"").unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn rejects_custom_crosshair_named_like_a_preset() {
        let err = parse("[[crosshair]]\nname = \"Dot\"").unwrap_err();
        assert!(err.to_string().contains("built-in"), "{err}");
    }

    #[test]
    fn rejects_duplicate_games() {
        let game = "[[game]]\nexe = \"a.exe\"\ncrosshair = \"Dot\"\n";
        assert!(parse(&format!("{game}{}", game.replace("a.exe", "A.EXE"))).is_err());
    }

    #[test]
    fn rejects_typos() {
        assert!(parse("[general]\nmaster_togle = \"F1\"").is_err());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("crossz-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let (created, new) = Config::load_or_create(&path).unwrap();
        assert!(new);
        assert_eq!(created, Config::default());
        assert_eq!(Config::load_or_create(&path).unwrap(), (created, false));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_other_programs_crosshair_becomes_the_selected_one() {
        let old = "[other_programs]\ncrosshair = \"Dot\"\n";
        let config = Config::parse(old, Path::new("old.toml")).unwrap();
        assert_eq!(config.general.crosshair, "Dot");
        assert_eq!(config.other_programs.crosshair, None);

        let new = "[general]\ncrosshair = \"T\"\n\n[other_programs]\ncrosshair = \"Dot\"\n";
        let config = Config::parse(new, Path::new("new.toml")).unwrap();
        assert_eq!(config.general.crosshair, "T");
        assert_eq!(config.other_programs.crosshair.as_deref(), Some("Dot"));
    }
}
