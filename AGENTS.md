# crossz

Crosshair overlay for borderless windowed games on Windows. Rust workspace:

- `crates/core`: platform-independent model, scaling, rasterizer, config and the visibility `Engine`. All logic that can be tested without Windows lives here.
- `crates/overlay-win`: Win32 overlay window, foreground tracking and raw input. Compiles to nothing on other platforms.
- `crates/overlay-mac`: the same for macOS: an `NSPanel`, the frontmost app from the window list, a listen-only event tap. Same public API as `overlay-win`, so the app picks one by `cfg`.
- `crates/app`: the `crossz` app (`main.rs` is a stub over `lib.rs`, so the screenshot tests can reach the UI): settings UI (gpui-kit), tray, config hot-reload, updater. On Linux it runs the settings UI only, for now.

## Commands

- `mise run check`: the local loop. fmt check, clippy `-D warnings`, every test (including the UI integration and screenshot tests) and the Windows cross-check. Run it before pushing.
- `/review` (OpenCode command, `.opencode/commands/review.md`): read-only review of the branch against `main`, run before pushing. It reports bugs, then missing tests, citing file:line. There is no review bot; Pullfrog is gone.
- `mise run goldens`: rewrite the screenshot PNGs in `crates/app/tests/golden/` after an intended UI change, then look at the images and the diff. Screenshots are macOS renders and only compared on macOS (the test is a no-op elsewhere).
- `mise run core`: fmt check, clippy and tests for core (runs anywhere; this is what the Linux CI job runs)
- `mise run check-win`: cross-clippy the Win32 overlay from macOS/Linux (the app itself needs `llvm-rc` to cross-build, so CI builds it on Windows)
- `mise run run`: run crossz
- `mise run win`: lint and test the Windows crates (Windows only)
- `mise run mac`: lint and test the macOS crates (macOS only)

Checks that run every time:

- Lints in the root `Cargo.toml` are errors: `unsafe_op_in_unsafe_fn`, `unused_must_use`, and clippy's `undocumented_unsafe_blocks`, `await_holding_lock`, `dbg_macro`, `todo`, `unimplemented`.
- `crates/core/tests/code_shape.rs` fails when a `.rs` file under `crates/*/src` has more than 800 lines before its `#[cfg(test)] mod tests`. Split the file instead of raising the limit. Files named `*tests.rs` are skipped.
- `crates/app/src/ui/ui_tests.rs`: UI integration tests that drive the real settings window headless (`gpui_kit::test`). Hooks for them are `.test_support().aria_label(..)` on the element, not test-only state.
- `crates/app/tests/golden.rs`: screenshot tests of each settings page (`harness = false`, because the headless Metal renderer needs the main thread). On failure it writes `<scene>.actual.png` next to the golden.

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

## Invariants

Each is checkable; the test that holds it is named where there is one.

- Selecting a crosshair sets `general.crosshair` and applies to every program, unless a game has its own override (`Game.crosshair`). (`engine::the_selected_crosshair_applies_everywhere_unless_a_game_has_its_own`, `ui_tests::clicking_a_crosshair_selects_and_saves_it`)
- Old configs migrate on load: `other_programs.crosshair` becomes `general.crosshair` when the file has none. (`config::old_other_programs_crosshair_becomes_the_selected_one`)
- Removing a crosshair resets every reference to it: games to `None`, binds and the selection to the first built-in. (`store::remove_falls_back_to_the_active_crosshair`)
- The status line ignores reports while crossz itself is in front and is recomputed from the current config. It says "Off" whenever the master toggle is off, even if the last report was "Hidden". (`store::status_says_off_whenever_the_master_toggle_is_off`, `ui_tests::the_status_line_explains_why_nothing_shows`)
- Closing the paint editor saves an in-progress stroke. Undo and redo do nothing mid-stroke. The editor takes focus when it opens. (focus: `ui_tests::a_new_crosshair_opens_the_editor_and_escape_closes_it`; the stroke rules have no test yet because the paint canvas needs pointer-down without pointer-up)
- Nothing slow runs in the raw input path (`WM_INPUT`, the macOS event tap callback).
- Every `unsafe` block has a `// SAFETY:` comment (enforced by clippy) and says why it is sound, including handle lifetimes.
- Raster changes come with pixel tests in `crates/core/src/raster.rs`.
- Settings UI changes come with regenerated goldens (`mise run goldens`) whose images you looked at.
- Config file changes stay backwards compatible: an old file loads and migrates.
- Never inject into games, read their memory or hook their rendering.
