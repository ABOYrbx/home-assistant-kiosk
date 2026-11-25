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

### Security

- Documented token handling, file permissions, TLS validation, and the offline-cache exposure model.
- Album-art requests are size-bounded, use a capped LRU cache, never forward the Home Assistant token across origins, and omit query strings from logs.

[Unreleased]: https://github.com/ABOYrbx/home-assistant-kiosk/commits/main
