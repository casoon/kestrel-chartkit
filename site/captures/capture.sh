#!/usr/bin/env bash
# Captures the run shown in the start-page scene: `cargo run --quiet --example basic_indicator`
# from this checkout, written unchanged to basic-indicator.txt.
#   site/captures/capture.sh
# Rebuild the SVG after a capture:
#   cd site/captures && npx -y -p @casoon/castwright@0.5.0 -p shiki castwright build basic-indicator.terminal.yaml --format svg -o .
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
cargo run --quiet --manifest-path "$repo/Cargo.toml" --example basic_indicator > "$here/basic-indicator.txt"
