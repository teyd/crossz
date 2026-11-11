# crossz

A crosshair overlay for games in borderless windowed mode, with per-game profiles and show/hide keybinds.

Play at native 1440p and keep the crosshair you're used to from stretched 1280×960: crossz draws it scaled 2.0× horizontally and 1.5× vertically, exactly as the stretched game would.

It's an external click-through window. It doesn't inject into the game or read its memory, and it doesn't work in exclusive fullscreen.

crossz runs in the tray; click the icon for settings. Everything is also in `%APPDATA%\crossz\config.toml`, which applies as soon as you save it. Start with `--hidden` to skip the settings window.

## Build

Requires [mise](https://mise.jdx.dev) (or rustup alone; the Rust version is in `rust-toolchain.toml`).

```sh
mise run run      # run crossz
mise run core     # format check, lint and test the core
mise run check-win  # cross-check the Windows crates from macOS/Linux
```

## License

MIT
