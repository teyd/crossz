use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Chord, Crosshair, Key, ScaleMode, Snap};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default)]
    pub programs: Programs,
    #[serde(default, rename = "profile")]
    pub profiles: Vec<Profile>,
    #[serde(default, rename = "binding")]
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    /// Shows/hides crossz everywhere, even outside matched programs.
    pub master_toggle: Option<Chord>,
    /// Hides the overlay from screenshots, OBS and other capture.
    pub exclude_from_capture: bool,
}

impl Default for General {
    fn default() -> Self {
        General {
            master_toggle: "Ctrl+Alt+X".parse().ok(),
            exclude_from_capture: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Programs {
    pub mode: ProgramMode,
    /// Profile used when no rule names one. Empty means the first profile.
    pub default_profile: String,
    #[serde(rename = "rule")]
    pub rules: Vec<ProgramRule>,
}

impl Default for Programs {
    fn default() -> Self {
        Programs {
            mode: ProgramMode::Whitelist,
            default_profile: String::new(),
            rules: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramMode {
    /// Show only while a listed program is in the foreground.
    #[default]
    Whitelist,
    /// Show everywhere except while a listed program is in the foreground.
    Blacklist,
    /// Show everywhere; rules only pick profiles.
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRule {
    /// Executable file name, e.g. `cs2.exe`. Compared case-insensitively.
    pub exe: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default = "enabled", skip_serializing_if = "is_enabled")]
    pub enabled: bool,
}

impl ProgramRule {
    pub fn matches(&self, exe: &str) -> bool {
        self.enabled && self.exe.eq_ignore_ascii_case(exe)
    }
}

fn enabled() -> bool {
    true
}

fn is_enabled(b: &bool) -> bool {
    *b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub scale: ScaleMode,
    #[serde(default)]
    pub snap: Snap,
    /// Screen-pixel nudge from the center, `[x, y]`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: [i32; 2],
    pub crosshair: Crosshair,
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
    CycleProfile,
    SetProfile(String),
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
        Config {
            general: General::default(),
            programs: Programs {
                mode: ProgramMode::Whitelist,
                default_profile: "Native".into(),
                rules: vec![ProgramRule {
                    exe: "cs2.exe".into(),
                    profile: Some("Stretched 1280x960".into()),
                    enabled: true,
                }],
            },
            profiles: vec![
                Profile {
                    name: "Native".into(),
                    scale: ScaleMode::Native,
                    snap: Snap::Round,
                    offset: [0, 0],
                    crosshair: Crosshair::default(),
                },
                Profile {
                    name: "Stretched 1280x960".into(),
                    scale: ScaleMode::Stretched {
                        width: 1280,
                        height: 960,
                    },
                    snap: Snap::Round,
                    offset: [0, 0],
                    crosshair: Crosshair::default(),
                },
            ],
            bindings: vec![Binding {
                trigger: Chord::key(Key::from_name("F8").expect("F8 is a key")),
                action: Action::Toggle,
            }],
        }
    }
}

impl Config {
    pub fn parse(text: &str, path: &Path) -> Result<Config, Error> {
        let config: Config = toml::from_str(text).map_err(|e| Error::Parse {
            path: path.to_owned(),
            source: Box::new(e),
        })?;
        config.validate()?;
        Ok(config)
    }

    /// Loads `path`, writing the default config there first if it doesn't exist.
    pub fn load_or_create(path: &Path) -> Result<Config, Error> {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text, path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let config = Config::default();
                config.save(path)?;
                Ok(config)
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
        if self.profiles.is_empty() {
            return Err(Error::Invalid("add at least one [[profile]]".into()));
        }
        let mut names = HashSet::new();
        for p in &self.profiles {
            if !names.insert(p.name.as_str()) {
                return Err(Error::Invalid(format!(
                    "two profiles are named {:?}",
                    p.name
                )));
            }
        }
        let known = |name: &str, what: &str| {
            if names.contains(name) {
                Ok(())
            } else {
                Err(Error::Invalid(format!(
                    "{what} uses unknown profile {name:?}"
                )))
            }
        };
        if !self.programs.default_profile.is_empty() {
            known(&self.programs.default_profile, "default_profile")?;
        }
        for rule in &self.programs.rules {
            if let Some(p) = &rule.profile {
                known(p, &format!("rule for {}", rule.exe))?;
            }
        }
        for b in &self.bindings {
            if let Action::SetProfile(p) = &b.action {
                known(p, &format!("binding {}", b.trigger))?;
            }
        }
        Ok(())
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn default_profile(&self) -> Option<&Profile> {
        self.profile(&self.programs.default_profile)
            .or(self.profiles.first())
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
    fn reads_handwritten_config() {
        let config = parse(
            r##"
            [general]
            master_toggle = "Ctrl+Shift+F1"

            [programs]
            mode = "blacklist"

            [[programs.rule]]
            exe = "explorer.exe"

            [[profile]]
            name = "cs"
            scale = { mode = "stretched", width = 1280, height = 960 }
            offset = [1, 0]

            [profile.crosshair]
            color = "#ff00ff"
            dot = { size = 2 }

            [[binding]]
            trigger = "Mouse2"
            action = "hold_to_hide"

            [[binding]]
            trigger = "F6"
            action = { set_profile = "cs" }
            "##,
        )
        .unwrap();
        assert_eq!(config.programs.mode, ProgramMode::Blacklist);
        assert!(config.programs.rules[0].enabled);
        assert_eq!(config.profiles[0].offset, [1, 0]);
        assert_eq!(config.profiles[0].crosshair.opacity, 1.0);
        assert_eq!(config.bindings[1].action, Action::SetProfile("cs".into()));
        assert_eq!(config.default_profile().unwrap().name, "cs");
    }

    #[test]
    fn rejects_unknown_profile_reference() {
        let err = parse(
            r##"
            [[programs.rule]]
            exe = "cs2.exe"
            profile = "nope"

            [[profile]]
            name = "a"
            crosshair = { color = "#ffffff" }
            "##,
        )
        .unwrap_err();
        assert!(err.to_string().contains("nope"), "{err}");
    }

    #[test]
    fn rejects_duplicate_profiles() {
        let p = r##"
            [[profile]]
            name = "a"
            crosshair = { color = "#ffffff" }
        "##;
        assert!(parse(&format!("{p}{p}")).is_err());
    }

    #[test]
    fn requires_a_profile() {
        let err = parse("").unwrap_err();
        assert!(err.to_string().contains("profile"), "{err}");
    }

    #[test]
    fn rejects_typos() {
        assert!(parse("[general]\nmaster_togle = \"F1\"").is_err());
    }

    #[test]
    fn save_then_load() {
        let dir = std::env::temp_dir().join(format!("crossz-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let created = Config::load_or_create(&path).unwrap();
        assert_eq!(created, Config::default());
        assert_eq!(Config::load_or_create(&path).unwrap(), created);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
