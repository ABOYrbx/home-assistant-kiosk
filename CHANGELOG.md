# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). The v1 release is not published yet; the release procedure and the required evidence are in [`docs/release-checklist.md`](docs/release-checklist.md).

## [Unreleased]

### Added

- Runtime-only performance budget check for RSS, first-frame time, and idle CPU, with finite-value validation and a result that is distinct from the full Wave 5 soak gate (`scripts/perf-budget.py --runtime-only`).
- Reproducible host and Pi soak procedure with a streaming RSS trend, entity and pending-action churn, bounded cache and album-art checks, and a validated `HA_KIOSK_SOAK_SECONDS` duration.
- Documentation gate for the release profile (`opt-level = "s"`, LTO, `strip`, `panic = "abort"`) and for placeholder-only example configuration.
- Operator documentation: installation, upgrade, and rollback steps in [`README.md`](README.md).
- Configuration reference for every `config.toml` key in [`README.md`](README.md), matching [`config.example.toml`](config.example.toml).
- Troubleshooting guide for cross-compilation, glibc mismatch, display backends, touch rotation, token permissions, Home Assistant version, offline state, timezone, and memory in [`docs/troubleshooting.md`](docs/troubleshooting.md).
- Release checklist covering gates, cross artifacts, checksums, changelog, hardware evidence, known limitations, publication, and rollback in [`docs/release-checklist.md`](docs/release-checklist.md).

### Changed

- The repository is public and `main` is protected by the `main-protection` ruleset: pull request, `docs` check on an up-to-date branch, linear history, resolved conversations, no force-push or deletion, and no bypass actors (#10).
- `README.md` now documents features, supported hardware, installation, the full configuration reference, troubleshooting, security, and license status without duplicated sections.
- `config.example.toml` comments every setting and uses reserved `.example` hosts instead of installation-specific values.

### Fixed

- A reconnect keeps the stale badge until the first fresh snapshot arrives, so cached values never look live; the first batch after a reconnect also replaces entities that disappeared while Home Assistant was offline.
- Removed duplicated documentation-check functions and section lists that a merged branch had left behind; the gate now runs the release-profile and performance-document checks again.

### Security

- Documented token handling, file permissions, TLS validation, and the offline-cache exposure model.
- The docs gate rejects active `ha.token` values, personal paths, personal identifiers, and active extension settings in the shipped example configuration.

[Unreleased]: https://github.com/ABOYrbx/home-assistant-kiosk/commits/main
