#!/usr/bin/env bash
set -euo pipefail

gh release create "$TAG" \
  "sift-$TAG-x86_64-unknown-linux-gnu.tar.gz" \
  "sift-$TAG-x86_64-unknown-linux-gnu.tar.gz.sha256" \
  --verify-tag --title "$TAG" --notes-file "$RUNNER_TEMP/release-notes.md"
