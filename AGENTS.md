# crossz

Crosshair overlay for borderless windowed games on Windows. Rust workspace:

- `crates/core`: platform-independent model, scaling, rasterizer, config and the visibility `Engine`. All logic that can be tested without Windows lives here.
- `crates/overlay-win`: Win32 overlay window, foreground tracking and raw input. Compiles to nothing on other platforms.
- `crates/app`: the `crossz` binary: settings UI, tray, config hot-reload.

## Commands

- `mise run core`: fmt check, clippy and tests for core (runs anywhere)
- `mise run check-win`: cross-clippy the Windows crate from macOS/Linux
- `mise run win`: full lint, test and release build (Windows only)

## Rules

- Never inject into games, read their memory, or hook their rendering. The overlay is an external, click-through window only.
- Prefer clear code over comments. Comment only what the code can't say: Win32 quirks, `// SAFETY:` on every `unsafe` block, non-obvious math.
- Keep platform code thin; put decisions in `crossz-core` with tests.

## Reviewing

- Check every Win32 `unsafe` block for a correct `// SAFETY:` argument and handle lifetimes.
- Watch for input latency: nothing slow may run in the raw input (`WM_INPUT`) path.
- Raster changes need matching pixel tests in `crates/core/src/raster.rs`.
