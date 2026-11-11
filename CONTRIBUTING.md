# Contributing

1. Open an issue first for anything bigger than a small fix.
2. Run `mise run core` (and `mise run check-win` if you touched Windows code) before opening a PR.
3. Fill in the PR template. Overlay and UI changes need a screenshot.

crossz must never inject into games or read their memory. PRs that do will be closed.
