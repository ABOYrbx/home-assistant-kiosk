# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The v1 release is not published yet; the release procedure and the required evidence are in [`docs/release-checklist.md`](docs/release-checklist.md).

## [Unreleased]

### Added

- Release preflight command `scripts/check-release.sh` with fail-fast automated gates
  and an optional `--with-artifacts` mode.
- Release artifact builds now validate version and target names and verify
  `SHA256SUMS` automatically.
- Operator documentation: installation, upgrade, and rollback steps in [`README.md`](README.md).
- Configuration reference for every `config.toml` key in [`README.md`](README.md), matching [`config.example.toml`](config.example.toml).
- Troubleshooting guide for cross-compilation, glibc mismatch, display backends, touch rotation, token permissions, Home Assistant version, offline state, timezone, and memory in [`docs/troubleshooting.md`](docs/troubleshooting.md).
- Release checklist covering gates, cross artifacts, checksums, changelog, hardware evidence, known limitations, publication, and rollback in [`docs/release-checklist.md`](docs/release-checklist.md).

### Security

- Documented token handling, file permissions, TLS validation, and the offline-cache exposure model.

### Known limitations

- Hardware (Raspberry Pi Zero 2 W / Pi 3), 24-hour soak, and display-path
  evidence are not yet recorded; see `docs/release-checklist.md` sections 7-8.
- Slint license decision remains open; binaries must not be distributed until it is recorded.

[Unreleased]: https://github.com/ABOYrbx/home-assistant-kiosk/commits/main
