# Release Checklist

This checklist is the release procedure for a v1 build. It turns the Wave 5 gate in [`PLAN.md`](../PLAN.md) into a concrete sequence. A release is made by `ABOYrbx` from `main`; see [`github-rules.md`](github-rules.md).

Do not distribute a binary until the license decision is recorded in `PLAN.md` section 12. Slint is GPL-3.0-only, royalty-free for non-embedded use, or commercial; a single-purpose kiosk may count as an embedded system. This is a legal gate, not a formality.

## 1. Preconditions

- [ ] Every subtask issue for the wave is closed, and the wave epic lists no open checkbox.
- [ ] `main` is green: the `docs` workflow passed on the current head.
- [ ] The license decision is recorded in `PLAN.md` section 12.
- [ ] The version number is chosen (semver) and matches the planned tag, for example `v1.0.0`.
- [ ] No open pull request changes behaviour that the release notes do not mention.

## 2. Automated gates

Run from a clean checkout of the release commit. The automated gates are also
available as a single fail-fast command:

```bash
scripts/check-release.sh
```

Equivalent manual sequence:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p 'test_*.py'
bash tests/deploy_scripts_test.sh
```

Cross artifacts and checksums are covered by:

```bash
scripts/check-release.sh --with-artifacts
```

- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy --all-targets -- -D warnings` passes.
- [ ] `cargo test` passes, including the end-to-end and failure-scenario tests.
- [ ] `cargo audit` reports no unaccepted advisory. Record every accepted advisory and its reason in the release notes.
- [ ] `python3 scripts/check-docs.py` passes. It validates TOML, Markdown links, required files, and personal-data markers.
- [ ] The desktop smoke run opens a window with `config.example.toml`:

  ```bash
  cargo run --features desktop -- --config config.example.toml
  ```

  Expect a clear configuration error for the placeholder token, not a crash or a blank window.

## 3. Cross artifacts

Build every supported target with the kiosk feature set:

```bash
cross build --release --target aarch64-unknown-linux-gnu \
  --no-default-features --features kiosk
cross build --release --target armv7-unknown-linux-gnueabihf \
  --no-default-features --features kiosk
cross build --release --target x86_64-unknown-linux-gnu \
  --no-default-features --features kiosk
```

Or run `scripts/build-all.sh` (or `scripts/check-release.sh --with-artifacts`),
which produces the same targets, writes them to `dist/`, and verifies `SHA256SUMS`.

- [ ] All three targets build from the release commit with the pinned `Cross.toml` digests.
- [ ] Artifact names contain the version and the target triple.
- [ ] No artifact bundles a configuration file, a token, a cache, or a debug symbol table.
- [ ] The release profile is `opt-level = "s"`, LTO, `strip`, and `panic = "abort"`. A change to any of these is re-measured, not assumed.
- [ ] The x86_64 artifact starts on a clean Bookworm host.

## 4. Checksums

Generate and publish a SHA-256 checksum for every artifact:

```bash
cd dist
sha256sum ./* > SHA256SUMS
sha256sum -c SHA256SUMS
```

`scripts/build-all.sh` performs this verification automatically and prints
`Verified SHA256SUMS`.


- [ ] `SHA256SUMS` is generated from the final artifacts, after the last rebuild.
- [ ] `sha256sum -c SHA256SUMS` passes on a second machine or a fresh directory.
- [ ] The checksum file is attached to the release together with the binaries.
- [ ] The checksums are copied into the release notes or the changelog entry.

## 5. Changelog and release notes

- [ ] [`CHANGELOG.md`](../CHANGELOG.md) has a dated entry for the version, with Added, Changed, Fixed, and Security sections as applicable.
- [ ] The release notes state the supported hardware, the required OS (Raspberry Pi OS Bookworm), and the glibc 2.35+ requirement for cross-built binaries.
- [ ] The release notes state the Home Assistant minimum version (2023.9) and the token and file-permission requirements.
- [ ] The release notes list known limitations from section 8 below.
- [ ] The release notes link the closed wave epic and the relevant pull requests.

## 6. Documentation matches the shipped behaviour

- [ ] `README.md` describes the features that actually ship, not the planned ones. Remove or mark post-v1 items.
- [ ] The configuration reference in `README.md` matches [`config.example.toml`](../config.example.toml) key by key.
- [ ] `config.example.toml` parses and contains placeholders only: no real URL, entity ID, stop ID, address, hostname, or token.
- [ ] `[extensions]` is disabled in the example; unsigned and HTTP catalogs are off.
- [ ] [`troubleshooting.md`](troubleshooting.md) covers the failures seen during the hardware test.
- [ ] The install, upgrade, log, and rollback steps were followed once on a clean device.
- [ ] `SECURITY.md` still names the current reporting path.

## 7. Hardware and soak record

The automated suite does not prove the Pi memory budget or the display path. Record the evidence:

- [ ] Raspberry Pi Zero 2 W, Bookworm 64-bit: service starts after reboot, shows a frame, reconnects.
- [ ] Raspberry Pi 3 Model B/B+, Bookworm: same checks.
- [ ] RSS below 150 MB, target below 100 MB, measured with `/usr/bin/time -v` or systemd memory accounting.
- [ ] First frame within 3 seconds with a warm cache.
- [ ] Idle CPU low enough that the device does not throttle; record the measurement method.
- [ ] 24-hour soak with simulated state changes shows no upward RSS trend and no unbounded cache growth.
- [ ] Display: no cursor, no blanking, correct rotation, touch reaches all four edges.
- [ ] Offline test: stop Home Assistant, the stale badge appears with the last update time, and the badge clears after Home Assistant returns.
- [ ] Invalid-token test: the error appears once and the app does not loop.
- [ ] Failed service call: the tile shows an error and never a fake "on" state.
- [ ] Logs contain no token, no authorization header, and no image query string.
- [ ] If a device is unavailable, write "not verified on hardware" in the release notes and list what was tested on x86_64 instead.

## 8. Known limitations

List the limitations that a user will meet, with the current behaviour and the tracking issue:

- [ ] Hardware that was not verified (for example Raspberry Pi 4/5, Wayland, Bullseye).
- [ ] Unsupported tile domains that render read-only.
- [ ] Climate setpoint, cover position, and other controls deferred beyond v1.
- [ ] Localization: English only in v1.
- [ ] Extension shop status: disabled in v1 or enabled with its own gate.
- [ ] Any accepted `cargo audit` advisory.
- [ ] Any Slint LinuxKMS limitation observed on the test panel.

## 9. Publish

- [ ] Merge the release pull request with a squash merge; the history stays linear.
- [ ] Create an annotated tag from `main`:

  ```bash
  git tag -a v1.0.0 -m "v1.0.0"
  git push origin v1.0.0
  ```

- [ ] Create the GitHub release from the tag with the notes, the artifacts, and `SHA256SUMS`.
- [ ] Do not move or delete a published tag.
- [ ] Verify the release page shows every artifact and the checksum file.
- [ ] Announce the known limitations; do not describe the build as hardware-verified unless the record exists.

## 10. Rollback

- [ ] The previous release tag and its artifacts stay available.
- [ ] A device can return to the previous binary without a configuration change; record the steps that were tested.
- [ ] A cache from the newer version is discarded with a warning on the older version, not migrated.
- [ ] If a release is withdrawn, mark it as withdrawn in the release notes and keep the tag immutable.

## Sign-off

| Item | Evidence | Result |
| --- | --- | --- |
| Automated gates | Command output in the release pull request | |
| Cross artifacts | `dist/` listing and target triples | |
| Checksums | `SHA256SUMS` and verification output | |
| Pi Zero 2 W | Hardware test record | |
| Pi 3 | Hardware test record | |
| 24-hour soak | RSS log and trend summary | |
| Documentation | Link to the release commit | |
| License decision | `PLAN.md` section 12 | |
