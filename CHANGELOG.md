# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The v1 release is not published yet; the release procedure and the required evidence are in [`docs/release-checklist.md`](docs/release-checklist.md).

## [Unreleased]

### Added

- Wave 3 feature widgets: local clock/date, weather and forecasts, Home Assistant tiles, media controls and album art, generic departures, and calendar agenda.
- Responsive adopted Overview with hero light controls, scenes/quick actions, tile groups, weather, and departures at kiosk and desktop widths.
- Headless software-renderer screenshot coverage for the required Wave 3 layouts.

- Operator documentation: installation, upgrade, and rollback steps in [`README.md`](README.md).
- Configuration reference for every `config.toml` key in [`README.md`](README.md), matching [`config.example.toml`](config.example.toml).
- Troubleshooting guide for cross-compilation, glibc mismatch, display backends, touch rotation, token permissions, Home Assistant version, offline state, timezone, and memory in [`docs/troubleshooting.md`](docs/troubleshooting.md).
- Release checklist covering gates, cross artifacts, checksums, changelog, hardware evidence, known limitations, publication, and rollback in [`docs/release-checklist.md`](docs/release-checklist.md).
- End-to-end failure scenarios against a mock WebSocket server: invalid token, failed service call, malformed frames, unknown domains, offline cache, and resync in [`tests/e2e_test.rs`](tests/e2e_test.rs).
- Performance evidence tooling: [`scripts/perf-record.py`](scripts/perf-record.py) builds a validated record from raw evidence, [`scripts/perf-memory.py`](scripts/perf-memory.py) captures GNU time or systemd cgroup values, and [`scripts/perf-heaptrack.sh`](scripts/perf-heaptrack.sh) runs a guarded x86_64 allocation profile.
- A dependency audit workflow contract test that also enforces the repository's GitHub-owned-actions policy.
- Signed catalog client: fetch `index.json` and its Minisign signature over HTTPS, verify against configured public keys (with key rotation), cache the last verified index for offline use, and enforce the configured catalog size cap (#119).

### Fixed

- The desktop CI gate passes again: tests that referenced APIs removed by the UI consolidation now cover the current state, widget, theme and shell surfaces (#231).
- An invalid token marks cached data stale and stops retrying instead of letting old values look live.
- A malformed frame during the WebSocket handshake no longer forces a reconnect; a bounded skip tolerates a corrupt frame and abandons a peer that only sends garbage.
- A failed tile service call renders its error, keeps the confirmed state, and allows a retry.

### Changed

- The authentication failure badge reads "Authentication failed".
- The dependency audit runs a pinned `cargo-audit` 0.23.0 against the committed `Cargo.lock`, and both workflows install Rust with the runner's `rustup`.
- The repository is public and `main` is protected by the `main-protection` ruleset: pull request, `docs` check on an up-to-date branch, linear history, resolved conversations, no force-push or deletion, and no bypass actors (#10).

### Fixed

- A reconnect keeps the stale badge until the first fresh snapshot arrives, so cached values never look live; the first batch after a reconnect also replaces entities that disappeared while Home Assistant was offline.

### Security

- Documented token handling, file permissions, TLS validation, and the offline-cache exposure model.
- Album-art requests are size-bounded, use a capped LRU cache, never forward the Home Assistant token across origins, and omit query strings from logs.
- A catalog signature fetch failure is no longer treated as an unsigned catalog: only a genuine 404 in development mode is accepted as unsigned, and the production fetcher rejects plain HTTP even after a redirect.

[Unreleased]: https://github.com/ABOYrbx/home-assistant-kiosk/commits/main
