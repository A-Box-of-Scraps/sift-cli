#!/usr/bin/env bash
set -euo pipefail

rustup toolchain install stable --profile minimal --component rustfmt --component clippy && rustup default stable
