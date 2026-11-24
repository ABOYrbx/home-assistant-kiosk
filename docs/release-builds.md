# Reproducible release builds

This page covers the release artifacts for Wave 4.1: how they are built, how to
verify that a rebuild produces the same bytes, which platforms the cross-built
binaries support, and how to build natively on a Raspberry Pi when a
cross-built binary cannot run.

## Artifacts

`scripts/build-all.sh` builds the kiosk feature set for every supported Linux
target and writes versioned binaries plus checksums to `dist/`:

| Target | Typical device | Artifact |
| --- | --- | --- |
| `aarch64-unknown-linux-gnu` | Raspberry Pi Zero 2 W, Pi 3 (64-bit OS) | `dist/ha-kiosk-<version>-aarch64-unknown-linux-gnu` |
| `armv7-unknown-linux-gnueabihf` | Raspberry Pi 3 with a 32-bit OS | `dist/ha-kiosk-<version>-armv7-unknown-linux-gnueabihf` |
| `x86_64-unknown-linux-gnu` | Desktop Linux, CI smoke test | `dist/ha-kiosk-<version>-x86_64-unknown-linux-gnu` |

`dist/SHA256SUMS` lists every artifact in the same format as `sha256sum`:

```text
$ (cd dist && sha256sum -c SHA256SUMS)
ha-kiosk-1.0.0-aarch64-unknown-linux-gnu: OK
ha-kiosk-1.0.0-armv7-unknown-linux-gnueabihf: OK
ha-kiosk-1.0.0-x86_64-unknown-linux-gnu: OK
```

`dist/` is ignored by git; attach the artifacts and `SHA256SUMS` to the release.

## Requirements

- Rust 1.95 or newer and the `cross` subcommand.
- Docker; `cross` runs the pinned images in containers. On macOS this is the
  only supported way to produce Linux kiosk binaries.
- A committed `Cargo.lock`. The build passes `--locked`, so a missing or stale
  lockfile fails instead of silently resolving new dependency versions.
- GNU coreutils `sha256sum` on the build host.

## Build

```bash
scripts/build-all.sh
```

| Variable | Default | Purpose |
| --- | --- | --- |
| `VERSION` | `[package] version` in `Cargo.toml` | Version in the artifact names |
| `TARGETS` | all three targets above | Build a subset, for example `TARGETS=aarch64-unknown-linux-gnu` |
| `SOURCE_DATE_EPOCH` | timestamp of the last commit | Fixed build timestamp |
| `DIST_DIR` | `<repo>/dist` | Artifact directory |
| `CARGO_TARGET_DIR` | `<repo>/target` | Cargo target directory |
| `CROSS` | `cross` | Cross binary to invoke |

The script removes old `ha-kiosk-<version>-<target>` files and the previous
`SHA256SUMS` before it starts, so a partial build cannot leave stale artifacts
in `dist/`.

## What makes the build reproducible

- **Pinned toolchains.** `Cross.toml` pins every image by tag and digest, so a
  rebuild uses the same compiler, sysroot, and system libraries.
- **Fixed timestamp.** `SOURCE_DATE_EPOCH` defaults to the last commit time and
  is exported to the compiler, so timestamps embedded in the binary do not
  change between runs.
- **Locked dependencies.** `cross build --locked` builds exactly the versions
  in `Cargo.lock`.
- **Deterministic codegen.** The script overrides the release profile with
  `opt-level=s`, `lto=fat`, `codegen-units=1`, `panic=abort`, and
  `strip=symbols`, and sets `CARGO_INCREMENTAL=0`. A local profile tweak or a
  warm incremental cache therefore cannot change the output.
- **Stable paths.** `--remap-path-prefix` replaces the checkout path and the
  Cargo home in embedded paths, so the build directory does not leak into the
  artifact.
- **Stable environment.** `LC_ALL=C`, `TZ=UTC`, and `umask 022` keep file names,
  ordering, and permissions identical on every host.

## Verify a rebuild

Build the same commit twice into separate directories and compare the
checksums:

```bash
export SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)"
DIST_DIR=dist-a scripts/build-all.sh
DIST_DIR=dist-b scripts/build-all.sh
diff dist-a/SHA256SUMS dist-b/SHA256SUMS
```

An empty `diff` means the artifacts are byte-identical. If the checksums
differ, compare the binaries with `cmp` or `diffoscope` before publishing and
record the cause in the release notes. Do not publish artifacts that fail this
check without an explanation.

## Platform support and the glibc floor

The pinned images use an Ubuntu 22.04 sysroot, so cross-built binaries link
against **glibc 2.35** and require glibc 2.35 or newer on the device.

- Supported: Debian 12 (Bookworm, glibc 2.36), Raspberry Pi OS Bookworm 64-bit,
  Ubuntu 22.04 and newer.
- Not supported for cross-built binaries: Raspberry Pi OS Bullseye (glibc 2.31)
  and older. Cross-built binaries are **Bookworm-only**; on Bullseye use the
  native build below.

Check the floor of an artifact on the build host:

```bash
objdump -T dist/ha-kiosk-<version>-aarch64-unknown-linux-gnu \
  | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -n1
```

## Native build fallback on the Pi

Use a native build when the device runs an older distribution, when a pinned
image is unavailable, or when debugging a link failure on the target.

```bash
sudo apt install build-essential pkg-config libfontconfig1-dev \
  libxkbcommon-dev libseat-dev libinput-dev libudev-dev
cargo build --release --no-default-features --features kiosk
```

Notes:

- Build from the same commit or tag as the release, with the committed
  `Cargo.lock`.
- A Pi Zero 2 W has 512 MB of RAM. Build with `CARGO_BUILD_JOBS=1` and enable
  at least 1 GB of swap; the link step is slow and may take hours. A Pi 3 is
  considerably faster.
- Name and checksum the native artifact in a separate directory, so a later
  cross build does not clean it up:

  ```bash
  VERSION="$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml | head -n1)"
  install -Dm0755 target/release/ha-kiosk \
    "dist-native/ha-kiosk-${VERSION}-$(uname -m)-native"
  (cd dist-native && sha256sum ha-kiosk-*-native > SHA256SUMS)
  ```

- A native build links against the glibc on that device. Record the device and
  OS version next to the checksum; the result is not interchangeable with a
  cross-built artifact.

## Updating a pinned image

1. Inspect the tag:

   ```bash
   docker buildx imagetools inspect ghcr.io/cross-rs/aarch64-unknown-linux-gnu:main
   ```

2. Replace both the tag and the digest in `Cross.toml`.
3. Run `scripts/build-all.sh` and the rebuild comparison above.
4. Record the old and new digest and the build result in the pull request.

Pinning protects the build from a moving tag; it does not make an unverified
image trustworthy. Bump digests deliberately, one target at a time, and keep
the native build path available.
