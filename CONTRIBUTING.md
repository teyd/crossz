# Contributing

1. Open an issue first for anything bigger than a small fix.
2. Run `mise run check` before opening a PR. If the settings UI changed on purpose, run `mise run goldens` and commit the updated screenshots in `crates/app/tests/golden/`.
3. Fill in the PR template. Overlay changes need a screenshot over a game; the settings window is covered by the golden screenshots.

crossz must never inject into games or read their memory. PRs that do will be closed.
