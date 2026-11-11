//! One crossz per user on macOS and Linux: the first one listens on a Unix socket, and
//! later launches connect to it to bring up its settings window, then exit.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// Keeps the socket file alive for the life of the process.
pub struct SingleInstance {
    path: PathBuf,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn socket_path() -> PathBuf {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("crossz-{uid}.sock"))
}

/// `None` if crossz is already running, after asking it to show its settings.
/// Otherwise calls `on_show` on a background thread whenever a later launch asks.
pub fn acquire(on_show: impl Fn() + Send + 'static) -> Option<SingleInstance> {
    let path = socket_path();
    if let Ok(mut running) = UnixStream::connect(&path) {
        let _ = running.write_all(b"show");
        return None;
    }
    // Nobody answered, so a socket file left here is from a crash.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(listener) => listener,
        // Lost a race with another launch starting at the same moment.
        Err(e) if e.kind() == ErrorKind::AddrInUse => return None,
        // Can't tell; run without single-instance protection rather than not at all.
        Err(_) => return Some(SingleInstance { path }),
    };
    std::thread::Builder::new()
        .name("crossz-instance".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = [0u8; 4];
                if stream.read_exact(&mut request).is_ok() && &request == b"show" {
                    on_show();
                }
            }
        })
        .expect("spawning the instance listener");
    Some(SingleInstance { path })
}
