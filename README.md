# Home Assistant Kiosk

A small, native Home Assistant dashboard for wall-mounted touchscreens and single-purpose Linux devices. It is written in Rust with Slint, uses the Home Assistant WebSocket API, and is designed to stay useful on a Raspberry Pi Zero 2 W with 512 MB RAM.

**Status:** planning. The repository currently contains the implementation plan and project rules, not a working application. The plan is in [`PLAN.md`](PLAN.md).

## Why this exists

A browser-based dashboard is convenient but expensive on a Pi Zero 2 W: a browser, a compositor, and a frontend bundle consume most of the available memory. This project uses one native process, one Home Assistant WebSocket connection, and a software renderer. It keeps the last known state on disk so the display survives a network or Home Assistant outage.

## Planned features

- Clock and date.
- Weather and a short forecast from a Home Assistant `weather` entity.
- A configurable grid of Home Assistant tiles.
- Now-playing card for a Home Assistant `media_player`.
- Departures from a provider-neutral Home Assistant sensor contract.
- Calendar agenda from Home Assistant calendars.
- Touch navigation, full-screen kiosk mode, and automatic reconnect.
- Offline cache with an honest stale/offline badge.
- TOML configuration and a systemd service.

## Supported hardware

| Device | Support |
| --- | --- |
| Raspberry Pi Zero 2 W (512 MB) | Primary target |
| Raspberry Pi 3 Model B/B+ | Primary target |
| Generic x86_64 Linux | Development and kiosk |
| Raspberry Pi 4/5 | Works, not a release gate |
| Raspberry Pi Zero v1 (ARMv6) | Not supported |

Primary OS: Raspberry Pi OS Bookworm (64-bit for Pi Zero 2 W and Pi 3). Cross-compiled binaries require glibc 2.35 or newer; Bullseye is not supported for cross-built binaries.

## Design in one paragraph

The Slint event loop and application state live on the main thread. A Tokio runtime handles the Home Assistant WebSocket connection, service calls, image fetches, and cache writes in the background. Background tasks send typed events to the UI thread, which updates Slint models. All data comes from Home Assistant; no weather, transit, or calendar provider is hardcoded. Configuration is a TOML file, and the token is read from a protected file or an environment variable.

## Repository layout

```text
PLAN.md                         implementation plan and source of truth
AGENTS.md                       rules for agents and contributors
docs/plan-review.md             fact-check findings and corrections
docs/departures-entity-contract.md
docs/issues/                    issue drafts for the six implementation waves
config.example.toml             safe, provider-neutral example configuration
src/                            Rust application (once Wave 0 starts)
ui/                             Slint UI files (once Wave 0 starts)
deploy/                         systemd unit and install scripts
tests/                          integration and UI tests
```

## Implementation waves

The plan is split into six waves, each tracked by a GitHub issue:

0. [Scaffold, interfaces, and cross-compile spike](https://github.com/ABOYrbx/home-assistant-kiosk/issues/5)
1. [Core: config, Home Assistant client, state and cache](https://github.com/ABOYrbx/home-assistant-kiosk/issues/3)
2. [UI shell: navigation, touch, theme](https://github.com/ABOYrbx/home-assistant-kiosk/issues/2)
3. [Feature widgets: clock/weather, tiles, media, departures, calendar](https://github.com/ABOYrbx/home-assistant-kiosk/issues/6)
4. [Kiosk and deployment: release builds, display, systemd](https://github.com/ABOYrbx/home-assistant-kiosk/issues/1)
5. [Integration, performance, and release](https://github.com/ABOYrbx/home-assistant-kiosk/issues/4)

## Development

Prerequisites: Rust 1.95 or newer, a C toolchain, and the Slint build dependencies. On Debian/Ubuntu:

```bash
sudo apt install build-essential pkg-config libfontconfig1-dev \
  libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

The application scaffold is created in Wave 0. Once it exists:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --features desktop -- --config config.example.toml
```

Cross-compilation uses [`cross`](https://github.com/rust-embedded/cross) and Docker:

```bash
cross build --release --target aarch64-unknown-linux-gnu \
  --no-default-features --features kiosk
```

## Configuration

`config.example.toml` contains placeholders only. Copy it to `/etc/ha-kiosk/config.toml`, set the Home Assistant URL and token file, and choose real entity IDs for your installation. Do not commit the resulting file.

The token file must be readable only by the service user:

```bash
sudo install -d -m 0750 -o ha-kiosk -g ha-kiosk /etc/ha-kiosk
sudo install -m 0600 -o ha-kiosk -g ha-kiosk ha_token /etc/ha-kiosk/ha_token
sudo install -m 0600 -o ha-kiosk -g ha-kiosk config.toml /etc/ha-kiosk/config.toml
```

## Contributing

- Read [`AGENTS.md`](AGENTS.md) first.
- The project language is English: code, comments, docs, commits, issues, and UI strings.
- Pushes are made only with the `ABOYrbx` GitHub account. Verify `gh auth status` before pushing.
- Do not add personal entity IDs, stop IDs, URLs, tokens, or addresses to the repository.
- Work by wave; each wave has a GitHub issue and a gate in `PLAN.md`.

## License

Not chosen yet. Slint is available under GPL-3.0-only, a royalty-free license that excludes embedded systems, or a commercial license. A single-purpose kiosk may count as an embedded system, so the license must be decided before binaries are distributed. See `PLAN.md` section 12.
