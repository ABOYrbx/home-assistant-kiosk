#!/usr/bin/env bash
# Build release binaries for all supported Linux targets and write checksums.
set -euo pipefail

VERSION="${VERSION:-$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)}"
TARGETS="aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf x86_64-unknown-linux-gnu"

mkdir -p dist
for target in $TARGETS; do
    echo "Building ${target}..."
    cross build --release --target "${target}" --no-default-features --features kiosk
    cp "target/${target}/release/ha-kiosk" "dist/ha-kiosk-${VERSION}-${target}"
done

cd dist
sha256sum ha-kiosk-* > SHA256SUMS
echo "Artifacts written to dist/ (version ${VERSION})"
