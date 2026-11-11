use std::path::PathBuf;
use std::sync::mpsc;

use crossz_core::Config;
use notify::{EventKind, RecursiveMode, Watcher};

use crate::Message;

/// Reports every change to the config file until the process ends. Invalid versions are
/// reported as errors, so a typo never takes the crosshair down.
pub fn spawn(path: PathBuf, messages: async_channel::Sender<Message>) {
    std::thread::Builder::new()
        .name("crossz-config-watch".into())
        .spawn(move || run(path, messages))
        .expect("spawning the config watcher");
}

fn run(path: PathBuf, messages: async_channel::Sender<Message>) {
    let (tx, rx) = mpsc::channel();
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(watcher) => watcher,
        Err(e) => return report(&messages, format!("can't watch the config file: {e}")),
    };
    // Watch the directory: saving via rename replaces the file and would drop a file watch.
    // Events carry canonical paths (e.g. /private/var on macOS), so compare against one.
    let Some(dir) = path.parent().and_then(|dir| dir.canonicalize().ok()) else {
        return;
    };
    let path = dir.join(path.file_name().expect("config path has a file name"));
    if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
        return report(&messages, format!("can't watch {}: {e}", dir.display()));
    }

    let mut last_text = std::fs::read_to_string(&path).ok();
    for event in rx.into_iter().flatten() {
        let touches_config = event.paths.contains(&path);
        if !touches_config || !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // One save fires several events.
        if last_text.as_ref() == Some(&text) {
            continue;
        }
        let message = match Config::parse(&text, &path) {
            Ok(config) => Message::Reloaded(config),
            Err(e) => Message::FileError(format!("keeping the previous settings: {e}")),
        };
        if messages.send_blocking(message).is_err() {
            return;
        }
        last_text = Some(text);
    }
}

fn report(messages: &async_channel::Sender<Message>, error: String) {
    let _ = messages.send_blocking(Message::FileError(error));
}
