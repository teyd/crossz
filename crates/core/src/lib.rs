//! Platform-independent core of crossz. Everything here is unit-tested on any OS;
//! the Win32 overlay and the settings UI only translate between this and the platform.

pub mod color;
pub mod config;
pub mod engine;
pub mod input;
pub mod model;
pub mod pixels;
pub mod presets;
pub mod raster;
pub mod scale;

pub use color::Color;
pub use config::{Action, Binding, Config, Game, General, Show};
pub use engine::{Engine, Shown, Status, Target, Visibility};
pub use input::{Chord, Key, Mods};
pub use model::{Arms, Circle, Crosshair, Dot, Outline, Shape};
pub use pixels::Pixels;
pub use raster::{Image, render};
pub use scale::{Resolution, Scale, Snap};
