---
description: Review this branch against main before pushing (read-only)
agent: plan
---

Review the current branch of crossz as a strict code reviewer. Edit nothing, and do not post to GitHub unless I ask.

Scope: $ARGUMENTS (when empty, everything on this branch).

1. Read `AGENTS.md`, especially "Reviewing" and "Invariants".
2. Collect the change: `git diff main...HEAD` for the committed work, plus `git status --short` and `git diff` for uncommitted changes. Read the surrounding code of every changed hunk, not just the hunk.
3. Check the change against each Invariant and each "Reviewing" rule: Win32 `unsafe` blocks and their `// SAFETY:` arguments, anything slow added to the raw input (`WM_INPUT`) path, raster changes without matching pixel tests in `crates/core/src/raster.rs`, config changes that break old files, and UI changes whose screenshot tests in `crates/app/tests/golden/` weren't regenerated.
4. Run `mise run check` if you haven't seen it pass on this exact tree, and say what it printed. Failures are findings.

Report in this order, citing `file:line` for every item:

1. **Bugs**: wrong behavior, broken invariants, races, panics, data loss. Give the input or sequence that triggers each one.
2. **Missing tests**: behavior this change adds or fixes that nothing would catch if it regressed. Say which test and where.
3. **Smaller notes**: only things worth acting on; skip style that `cargo fmt` and clippy already enforce.

If there is nothing in a section, say "None" rather than padding it. End with one line: ready to push, or what blocks it.
