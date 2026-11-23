#!/usr/bin/env bash
# Build reproducible release binaries for every supported Linux target.
#
# Usage:
#   scripts/build-all.sh
#
# Environment overrides:
#   VERSION            release version (default: [package] version in Cargo.toml)
#   TARGETS            space-separated Rust targets
#   SOURCE_DATE_EPOCH  build timestamp (default: timestamp of the last commit)
#   DIST_DIR           artifact directory (default: <repo>/dist)
#   CARGO_TARGET_DIR   Cargo target directory (default: <repo>/target)
#   CROSS              cross binary (default: cross)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export LC_ALL=C
export TZ=UTC
umask 022

VERSION="${VERSION:-$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml 2>/dev/null | head -n1)}"
if [ -z "$VERSION" ]; then
    echo "error: set VERSION or add a [package] version to Cargo.toml" >&2
    exit 1
fi

TARGETS="${TARGETS:-aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf x86_64-unknown-linux-gnu}"
CROSS="${CROSS:-cross}"
DIST_DIR="${DIST_DIR:-$ROOT/dist}"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# A fixed timestamp removes the most common source of run-to-run differences.
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct 2>/dev/null || true)}"
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-0}"
export SOURCE_DATE_EPOCH

# Deterministic codegen. These override the Cargo.toml profile so an artifact
# does not depend on local profile tweaks or a stale incremental cache.
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_OPT_LEVEL=s
export CARGO_PROFILE_RELEASE_LTO=fat
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1
export CARGO_PROFILE_RELEASE_PANIC=abort
export CARGO_PROFILE_RELEASE_STRIP=symbols

# Remove the checkout path and the Cargo home from embedded paths.
REMAP="--remap-path-prefix=${ROOT}=/build/ha-kiosk"
REMAP="${REMAP} --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }${REMAP}"

mkdir -p "$DIST_DIR"
rm -f "$DIST_DIR"/ha-kiosk-*-* "$DIST_DIR/SHA256SUMS"

for target in $TARGETS; do
    echo "Building ${target}..."
    "$CROSS" build \
        --locked \
        --release \
        --target "$target" \
        --target-dir "$TARGET_DIR" \
        --no-default-features \
        --features kiosk
    artifact="$DIST_DIR/ha-kiosk-${VERSION}-${target}"
    cp "$TARGET_DIR/${target}/release/ha-kiosk" "$artifact"
    chmod 0755 "$artifact"
done

(
    cd "$DIST_DIR"
    sha256sum ha-kiosk-* | sort -k2 > SHA256SUMS
)

echo "Wrote $(find "$DIST_DIR" -maxdepth 1 -name 'ha-kiosk-*' | wc -l) artifact(s) to ${DIST_DIR} (version ${VERSION}, SOURCE_DATE_EPOCH ${SOURCE_DATE_EPOCH})"
