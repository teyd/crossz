//! The macOS side of crossz: a click-through panel that draws the crosshair, tracking
//! of the frontmost app, and a listen-only event tap for bindings.
//! Compiles to nothing on other platforms.
#![cfg(target_os = "macos")]

pub mod autostart;
mod input;
mod keys;
mod programs;
mod screen;
mod window;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossz_core::{Config, Crosshair, Engine, Image, Scale, Target, render};
use dispatch2::DispatchQueue;
use objc2::MainThreadMarker;

pub use programs::{Icon, Program, icon, running_programs};

pub enum Command {
    SetConfig(Box<Config>),
    SetEnabled(bool),
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The master toggle changed, from a hotkey or `Command::SetEnabled`.
    EnabledChanged(bool),
    /// Keys and buttons can't be read until crossz gets Input Monitoring permission.
    InputUnavailable,
}

/// How often the frontmost app and its window are checked. macOS has no notification
/// for another app's window moving without Accessibility permission.
const POLL: Duration = Duration::from_millis(150);

struct Shared {
    engine: Mutex<Engine>,
    foreground: Mutex<Option<screen::Foreground>>,
    /// What's on screen now, to skip redundant work: `None` until known, then
    /// `Some(None)` while hidden.
    shown: Mutex<Option<Option<Placement>>>,
    /// Bumped with every decision; the main thread drops updates that aren't the latest,
    /// since several threads can queue them out of order.
    generation: Arc<AtomicU64>,
    on_event: Box<dyn Fn(Event) + Send + Sync>,
    quit: AtomicBool,
}

#[derive(Clone, PartialEq)]
struct Placement {
    crosshair: Crosshair,
    scale: Scale,
    /// Top left in pixels, in global CoreGraphics coordinates.
    origin: (i64, i64),
    display_scale: f64,
}

/// Handle to the overlay. Create it on the main thread, after the app has launched.
pub struct Overlay {
    shared: Arc<Shared>,
}

impl Overlay {
    pub fn spawn(
        config: Config,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Overlay, std::convert::Infallible> {
        let mtm = MainThreadMarker::new().expect("the overlay is created on the main thread");
        window::create(mtm);
        window::exclude_from_capture(config.general.exclude_from_capture);
        let shared = Arc::new(Shared {
            engine: Mutex::new(Engine::new(config)),
            foreground: Mutex::new(None),
            shown: Mutex::new(None),
            generation: Arc::new(AtomicU64::new(0)),
            on_event: Box::new(on_event),
            quit: AtomicBool::new(false),
        });
        let poller = shared.clone();
        std::thread::Builder::new()
            .name("crossz-foreground".into())
            .spawn(move || poll_foreground(poller))
            .expect("spawning the foreground poller");
        input::spawn(shared.clone());
        Ok(Overlay { shared })
    }

    pub fn send(&self, command: Command) {
        match command {
            Command::SetConfig(config) => {
                window::exclude_from_capture(config.general.exclude_from_capture);
                self.shared.engine.lock().unwrap().set_config(*config);
                // Redraw even if the placement looks the same; the window may need hiding.
                *self.shared.shown.lock().unwrap() = None;
            }
            Command::SetEnabled(enabled) => {
                self.shared.engine.lock().unwrap().set_enabled(enabled);
                (self.shared.on_event)(Event::EnabledChanged(enabled));
            }
            Command::Quit => {
                self.shared.quit.store(true, Ordering::Relaxed);
                window::hide();
                return;
            }
        }
        refresh(&self.shared);
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.send(Command::Quit);
    }
}

fn poll_foreground(shared: Arc<Shared>) {
    while !shared.quit.load(Ordering::Relaxed) {
        let now = screen::foreground();
        let changed = {
            let mut last = shared.foreground.lock().unwrap();
            let changed = last.as_ref() != Some(&now);
            if last.as_ref().map(|f| &f.exe) != Some(&now.exe) {
                shared
                    .engine
                    .lock()
                    .unwrap()
                    .set_foreground(now.exe.as_deref());
            }
            *last = Some(now);
            changed
        };
        if changed {
            refresh(&shared);
        }
        std::thread::sleep(POLL);
    }
}

/// Works out what should be on screen and, if that changed, updates the panel on the
/// main thread. Callable from any thread.
fn refresh(shared: &Shared) {
    let next = placement(shared);
    let generation = {
        let mut shown = shared.shown.lock().unwrap();
        let placement = next.as_ref().map(|(p, _)| p.clone());
        if shown.as_ref() == Some(&placement) {
            return;
        }
        *shown = Some(placement);
        shared.generation.fetch_add(1, Ordering::SeqCst) + 1
    };
    let latest = shared.generation.clone();
    on_main(move || {
        if latest.load(Ordering::SeqCst) != generation {
            return;
        }
        match next {
            None => window::hide(),
            Some((p, image)) => window::show(&image, &frame(&p, &image)),
        }
    });
}

fn placement(shared: &Shared) -> Option<(Placement, Image)> {
    let foreground = shared.foreground.lock().unwrap().clone()?;
    let engine = shared.engine.lock().unwrap();
    let shown = engine.shown()?;
    let display = foreground.display;
    let rect = match (shown.target, foreground.window) {
        (Target::Window, Some(window)) => window,
        _ => display.bounds,
    };
    let s = display.scale;
    let (width_px, height_px) = ((rect.width * s).round(), (rect.height * s).round());
    let scale = shown
        .game
        .resolution
        .scale(width_px as u32, height_px as u32);
    let image = render(shown.crosshair, scale);
    // Positioned in whole pixels, so the crosshair lands on the display's pixel grid.
    let [dx, dy] = shown.game.offset;
    let center_x = ((rect.x + rect.width / 2.0) * s).round() as i64;
    let center_y = ((rect.y + rect.height / 2.0) * s).round() as i64;
    let placement = Placement {
        crosshair: shown.crosshair.clone(),
        scale,
        origin: (
            center_x - image.width as i64 / 2 + dx as i64,
            center_y - image.height as i64 / 2 + dy as i64,
        ),
        display_scale: s,
    };
    Some((placement, image))
}

fn frame(p: &Placement, image: &Image) -> window::Frame {
    let s = p.display_scale;
    let top = p.origin.1 as f64 / s;
    let height = image.height as f64 / s;
    window::Frame {
        x: p.origin.0 as f64 / s,
        y: screen::main_display_height() - top - height,
        scale: s,
    }
}

fn on_main(work: impl FnOnce() + Send + 'static) {
    DispatchQueue::main().exec_async(work);
}
