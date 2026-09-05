#!/usr/bin/env bash
set -euo pipefail

cp scripts/release/changelog.py "$RUNNER_TEMP/changelog.py"
git fetch origin "$DEFAULT_BRANCH"
git checkout -B release-changelog "origin/$DEFAULT_BRANCH"
python3 "$RUNNER_TEMP/changelog.py" finalize "$TAG" "$RUNNER_TEMP"
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git add CHANGELOG.md
git commit -m 'Add [Unreleased] section for next cycle'
git push origin "HEAD:refs/heads/$DEFAULT_BRANCH"
