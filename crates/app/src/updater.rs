//! Self-update in the spirit of Hex's updater: poll a manifest published with each
//! release, download the new executable next to the running one, check its size and
//! SHA-256, then swap it in when the user restarts. The running exe is renamed aside
//! rather than overwritten (Windows allows that), so its path, and with it the
//! autostart entry, never changes.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const MANIFEST_URL: &str =
    "https://github.com/teyd/crossz/releases/latest/download/latest.json";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
const TARGET: &str = "windows-x64";
/// Generous cap so a broken manifest can't fill the disk.
const MAX_SIZE: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Checking,
    UpToDate,
    Downloading(String),
    /// Downloaded and verified; restarting installs it.
    Ready {
        version: String,
        notes_url: String,
    },
    /// This copy can't update itself, e.g. a development build.
    Unavailable(String),
    Failed(String),
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes_url: String,
    pub targets: std::collections::HashMap<String, Target>,
}

#[derive(Debug, Deserialize)]
pub struct Target {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// Why this copy can't update itself, if it can't.
pub fn unavailable() -> Option<String> {
    if !cfg!(windows) {
        return Some("Updates are published for Windows only.".into());
    }
    if cfg!(debug_assertions) {
        return Some("Development builds don't update.".into());
    }
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let probe = dir.join(".crossz-write-test");
    match File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            None
        }
        Err(_) => Some(format!(
            "Can't write to {}. Move crossz to a folder you own to get updates.",
            dir.display()
        )),
    }
}

/// Checks for a newer release and, if there is one, downloads and verifies it.
/// Blocking; run it off the UI thread. `progress` hears about the download starting.
pub fn check_and_download(progress: impl Fn(State)) -> State {
    if let Some(reason) = unavailable() {
        return State::Unavailable(reason);
    }
    let manifest: Manifest = match fetch_manifest() {
        Ok(m) => m,
        Err(e) => return State::Failed(format!("couldn't check for updates: {e}")),
    };
    if !is_newer(&manifest.version, VERSION) {
        return State::UpToDate;
    }
    let Some(target) = manifest.targets.get(TARGET) else {
        return State::UpToDate;
    };
    progress(State::Downloading(manifest.version.clone()));
    match download(target, &staged_path()) {
        Ok(()) => State::Ready {
            version: manifest.version,
            notes_url: manifest.notes_url,
        },
        Err(e) => State::Failed(format!("couldn't download {}: {e}", manifest.version)),
    }
}

fn fetch_manifest() -> Result<Manifest, Box<dyn std::error::Error>> {
    let mut response = ureq::get(MANIFEST_URL).call()?;
    Ok(response.body_mut().read_json()?)
}

/// Downloads to a partial file, checking size and SHA-256 before renaming it into place.
fn download(target: &Target, to: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if target.size > MAX_SIZE {
        return Err(format!("the update claims {} bytes", target.size).into());
    }
    let partial = to.with_extension("partial");
    let response = ureq::get(&target.url).call()?;
    let mut body = response.into_body().into_reader().take(target.size + 1);
    let mut file = File::create(&partial)?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        written += n as u64;
    }
    file.sync_all()?;
    drop(file);
    let digest = hex(&hasher.finalize());
    if written != target.size || !digest.eq_ignore_ascii_case(&target.sha256) {
        let _ = std::fs::remove_file(&partial);
        return Err("the download doesn't match the release's checksum".into());
    }
    std::fs::rename(&partial, to)?;
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn current_exe() -> PathBuf {
    std::env::current_exe().expect("the running executable has a path")
}

fn staged_path() -> PathBuf {
    current_exe().with_extension("exe.new")
}

fn aside_path() -> PathBuf {
    current_exe().with_extension("exe.old")
}

/// Swaps the staged update in and starts it; the caller quits right after. The new
/// process waits for this one to exit before it takes over.
pub fn install_and_restart(hidden: bool) -> std::io::Result<()> {
    let exe = current_exe();
    let aside = aside_path();
    let _ = std::fs::remove_file(&aside);
    std::fs::rename(&exe, &aside)?;
    if let Err(e) = std::fs::rename(staged_path(), &exe) {
        std::fs::rename(&aside, &exe)?;
        return Err(e);
    }
    let mut command = std::process::Command::new(&exe);
    command
        .arg("--after-update")
        .arg(std::process::id().to_string());
    if hidden {
        command.arg("--hidden");
    }
    command.spawn().map(drop)
}

/// Removes what the last update left behind. The old exe may still be locked for a
/// moment after it exits, so failure here is fine; the next start tries again.
pub fn clean_up() {
    let _ = std::fs::remove_file(aside_path());
}

/// Whether `candidate` is a newer `major.minor.patch` than `current`. Pre-release
/// suffixes are ignored; unparsable versions are never newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
        let mut parts = core.split('.').map(|p| p.parse().ok());
        Some((parts.next()??, parts.next()??, parts.next()??))
    }
    matches!((parse(candidate), parse(current)), (Some(a), Some(b)) if a > b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn parses_the_published_manifest() {
        let m: Manifest = serde_json::from_str(
            r#"{"version":"0.2.0","notes_url":"https://example.com","targets":{
                "windows-x64":{"url":"https://example.com/crossz.exe","sha256":"ab","size":3}}}"#,
        )
        .unwrap();
        assert_eq!(m.targets["windows-x64"].size, 3);
    }

    #[test]
    fn hex_encodes() {
        assert_eq!(hex(&[0x00, 0xab, 0xff]), "00abff");
    }
}
