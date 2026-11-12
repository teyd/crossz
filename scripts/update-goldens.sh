#!/usr/bin/env bash
# Screenshots are rendered on Linux only (see crates/app/tests/golden.rs), so a refresh runs
# the ci workflow on the pushed branch with update-goldens=true and downloads its artifact.
set -euo pipefail
cd "$(dirname "$0")/.."

branch=$(git branch --show-current)
git diff --quiet HEAD || echo "note: CI renders the pushed commit, not your uncommitted changes" >&2
git fetch -q origin "$branch"
if [ "$(git rev-parse HEAD)" != "$(git rev-parse "origin/$branch")" ]; then
  echo "push $branch first: CI renders what is on the remote" >&2
  exit 1
fi

gh workflow run ci.yml --ref "$branch" -f update-goldens=true
sleep 5
run=$(gh run list --workflow ci.yml --branch "$branch" --event workflow_dispatch --limit 1 --json databaseId --jq '.[0].databaseId')
gh run watch "$run" --exit-status || true
rm -rf target/golden-download
gh run download "$run" -n golden -D target/golden-download
cp target/golden-download/*.png crates/app/tests/golden/
rm -rf target/golden-download
git status --short crates/app/tests/golden
echo "Look at every changed image in crates/app/tests/golden/, then commit them."
