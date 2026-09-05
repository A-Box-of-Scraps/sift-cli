#!/usr/bin/env bash
set -euo pipefail

[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]
git checkout --detach "refs/tags/$TAG"
git merge-base --is-ancestor HEAD "origin/$DEFAULT_BRANCH"
python3 scripts/release/changelog.py prepare "$TAG" "$RUNNER_TEMP"
