#!/usr/bin/env bash
# Run the kiosk in desktop (winit) mode against the example configuration.
# Usage: ./scripts/run-dev.sh [path-to-config.toml]
set -euo pipefail

cd "$(dirname "$0")/.."

CONFIG="${1:-config.example.toml}"

if [[ ! -f "$CONFIG" ]]; then
    echo "config file not found: $CONFIG" >&2
    exit 2
fi

exec cargo run --features desktop -- --config "$CONFIG"