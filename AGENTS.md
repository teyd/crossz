# crossz

Crosshair overlay for borderless windowed games on Windows. Rust workspace:

- `crates/core`: platform-independent model, scaling, rasterizer, config and the visibility `Engine`. All logic that can be tested without Windows lives here.
- `crates/overlay-win`: Win32 overlay window, foreground tracking and raw input. Compiles to nothing on other platforms.
- `crates/overlay-mac`: the same for macOS: an `NSPanel`, the frontmost app from the window list, a listen-only event tap. Same public API as `overlay-win`, so the app picks one by `cfg`.
- `crates/app`: the `crossz` binary: settings UI (gpui-kit), tray, config hot-reload, updater. On Linux it runs the settings UI only, for now.

## Commands

- `mise run core`: fmt check, clippy and tests for core (runs anywhere)
- `mise run check-win`: cross-clippy the Win32 overlay from macOS/Linux (the app itself needs `llvm-rc` to cross-build, so CI builds it on Windows)
- `mise run run`: run crossz
- `mise run win`: lint and test the Windows crates (Windows only)
- `mise run mac`: lint and test the macOS crates (macOS only)

## Rules

- Never inject into games, read their memory, or hook their rendering. The overlay is an external, click-through window only.
- Prefer clear code over comments. Comment only what the code can't say: Win32 quirks, `// SAFETY:` on every `unsafe` block, non-obvious math.
- Keep platform code thin; put decisions in `crossz-core` with tests.

## Reviewing

- Check every Win32 `unsafe` block for a correct `// SAFETY:` argument and handle lifetimes.
- Watch for input latency: nothing slow may run in the raw input (`WM_INPUT`) path.
- Raster changes need matching pixel tests in `crates/core/src/raster.rs`.

## Releasing

1. Bump `version` in the root `Cargo.toml` in a PR, with release notes in the PR body. Merge it.
2. Run the `release` workflow from main: `gh workflow run release -f version=X.Y.Z`. It checks the version, builds `crossz.exe`, publishes it with `latest.json` (read by the in-app updater) and creates the `vX.Y.Z` tag last. Pushing a tag does nothing.
