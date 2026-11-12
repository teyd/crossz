//! Keeps source files small enough to read whole: at most `MAX_LINES` lines of code in
//! each `.rs` file under `crates/*/src`, not counting its `#[cfg(test)]` module or files
//! of tests (named `*tests.rs`). Split a file by what it does when it grows past that.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 800;

#[test]
fn source_files_stay_small() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for entry in fs::read_dir(&crates).unwrap() {
        rust_files(&entry.unwrap().path().join("src"), &mut files);
    }
    assert!(
        !files.is_empty(),
        "no sources found under {}",
        crates.display()
    );

    let too_long: Vec<String> = files
        .iter()
        .filter_map(|path| {
            let lines = code_lines(&fs::read_to_string(path).unwrap());
            (lines > MAX_LINES).then(|| format!("{} has {lines} lines", path.display()))
        })
        .collect();
    assert!(
        too_long.is_empty(),
        "split these files, the limit is {MAX_LINES} lines outside tests:\n{}",
        too_long.join("\n")
    );
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.map(|e| e.unwrap().path()) {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.to_string_lossy().ends_with(".rs")
            && !path.to_string_lossy().ends_with("tests.rs")
        {
            out.push(path);
        }
    }
}

/// Lines before the file's `#[cfg(test)] mod tests`, by convention its last item.
fn code_lines(source: &str) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    (lines.windows(2))
        .position(|pair| pair[0].trim() == "#[cfg(test)]" && pair[1].starts_with("mod tests"))
        .unwrap_or(lines.len())
}

#[test]
fn only_the_test_module_is_not_counted() {
    assert_eq!(code_lines("fn a() {}\n\n#[cfg(test)]\nmod tests {}\n"), 2);
    assert_eq!(code_lines("#[cfg(test)]\nfn helper() {}\nfn a() {}\n"), 3);
}
