use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crossz_core::Config;
use notify::{EventKind, RecursiveMode, Watcher};

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("crossz")
        .join("config.toml")
}

fn main() {
    let path = config_path();
    let config = match Config::load_or_create(&path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("crossz: {e}");
            std::process::exit(1);
        }
    };
    println!("crossz: using {}", path.display());
    run(config, &path);
}

#[cfg(windows)]
fn run(config: Config, path: &Path) {
    use crossz_overlay_win::{Command, Event, Overlay};

    crossz_overlay_win::enable_dpi_awareness();
    let overlay = Overlay::spawn(config, |Event::EnabledChanged(on)| {
        println!("crossz: {}", if on { "enabled" } else { "disabled" });
    })
    .unwrap_or_else(|e| {
        eprintln!("crossz: starting the overlay: {e}");
        std::process::exit(1);
    });
    watch(path, |config| overlay.send(Command::SetConfig(config)));
}

#[cfg(not(windows))]
fn run(_config: Config, path: &Path) {
    println!("crossz: the overlay only runs on Windows; watching the config for errors");
    watch(path, |_| {});
}

/// Calls `apply` with every valid version of the config file until the process ends.
/// Invalid edits are reported and skipped, so a typo never takes the crosshair down.
fn watch(path: &Path, apply: impl Fn(Config)) {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx).expect("creating the config watcher");
    // Watch the directory: saving via rename replaces the file and would drop a file watch.
    // Events carry canonical paths (e.g. /private/var on macOS), so compare against one.
    let dir = (path.parent())
        .and_then(|dir| dir.canonicalize().ok())
        .expect("config directory exists after load_or_create");
    let path = &dir.join(path.file_name().expect("config path has a file name"));
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .expect("watching the config directory");

    let mut last_text = std::fs::read_to_string(path).ok();
    for event in rx.into_iter().flatten() {
        let touches_config = event.paths.iter().any(|p| p == path);
        if !touches_config || !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // One save fires several events.
        if last_text.as_ref() == Some(&text) {
            continue;
        }
        match Config::parse(&text, path) {
            Ok(config) => {
                println!("crossz: reloaded config");
                apply(config);
            }
            Err(e) => eprintln!("crossz: keeping the previous config: {e}"),
        }
        last_text = Some(text);
    }
}
