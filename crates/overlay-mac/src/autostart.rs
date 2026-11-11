use std::path::PathBuf;

const LABEL: &str = "com.teyd.crossz";

fn plist_path() -> std::io::Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| std::io::Error::other("HOME isn't set"))?;
    Ok(PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

/// Starts this executable with `args` when the user logs in, through a LaunchAgent, or
/// stops doing so. Writing it on every launch keeps it right if the app was moved.
pub fn set(enabled: bool, args: &str) -> std::io::Result<()> {
    let path = plist_path()?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let exe = std::env::current_exe()?;
    let arguments: String = std::iter::once(exe.to_string_lossy().into_owned())
        .chain(args.split_whitespace().map(str::to_owned))
        .map(|arg| format!("\n        <string>{}</string>", escape(&arg)))
        .collect();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>{arguments}
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    );
    if std::fs::read_to_string(&path).is_ok_and(|current| current == plist) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, plist)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
