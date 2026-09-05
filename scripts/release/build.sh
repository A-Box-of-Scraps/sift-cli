#!/usr/bin/env bash
set -euo pipefail

cargo build --locked --release --target x86_64-unknown-linux-gnu
