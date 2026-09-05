#!/usr/bin/env bash
set -euo pipefail

archive="sift-$TAG-x86_64-unknown-linux-gnu.tar.gz"
mkdir -p dist
cp target/x86_64-unknown-linux-gnu/release/sift README.md LICENSE CHANGELOG.md dist/
tar -czf "$archive" -C dist .
sha256sum "$archive" > "$archive.sha256"
