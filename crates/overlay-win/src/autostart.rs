use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("crossz");

/// Starts this executable with `args` when the user signs in, or stops doing so.
/// Writing it on every launch keeps the entry right if the exe was moved.
pub fn set(enabled: bool, args: &str) -> std::io::Result<()> {
    let result = if enabled {
        let command = format!("\"{}\" {args}", std::env::current_exe()?.display());
        let data: Vec<u16> = command.encode_utf16().chain([0]).collect();
        // SAFETY: `data` is a NUL-terminated UTF-16 string and the size covers it.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        }
    } else {
        // SAFETY: static wide strings.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) } {
            ERROR_FILE_NOT_FOUND => return Ok(()),
            other => other,
        }
    };
    result.ok().map_err(std::io::Error::other)
}
