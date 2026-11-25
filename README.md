# Home Assistant Kiosk

A small, native Home Assistant dashboard for wall-mounted touchscreens and single-purpose Linux devices. It is written in Rust with Slint, uses the Home Assistant WebSocket API, and is designed to stay useful on a Raspberry Pi Zero 2 W with 512 MB RAM.

[![Docs check](https://github.com/ABOYrbx/home-assistant-kiosk/actions/workflows/docs.yml/badge.svg)](https://github.com/ABOYrbx/home-assistant-kiosk/actions/workflows/docs.yml)

**Status:** planning. The repository currently contains the implementation plan, project rules, and the operator documentation, not a working application. The installation, configuration reference, troubleshooting, and release steps describe the intended v1 behaviour; the plan is in [`PLAN.md`](PLAN.md).

## Why this exists

A browser-based dashboard is convenient but expensive on a Pi Zero 2 W: a browser, a compositor, and a frontend bundle consume most of the available memory. This project uses one native process, one Home Assistant WebSocket connection, and a software renderer. It keeps the last known state on disk so the display survives a network or Home Assistant outage.

## Design

The visual design is adopted, not a placeholder. [`DESIGN.md`](DESIGN.md) and [`tokens.css`](tokens.css) are the authoritative design system; [`index.html`](index.html) and [`styles.css`](styles.css) are the Vite reference prototype. The production kiosk is Slint, not a browser.

The Overview follows the prototype: top bar, hero light with brightness/colour temperature, scenes/quick actions, and a configurable rooms/tile group. The layout must also work at 800 × 480 and 1024 × 600. Energy and Schedule are post-v1. The prototype's mock values are not production data.

## Features

- Clock and date.
- Weather and a short forecast from a Home Assistant `weather` entity.
- A configurable grid of Home Assistant tiles.
- An Overview with hero light controls (toggle, brightness, colour temperature), scenes/quick actions, and a configurable rooms/tile group.
- Now-playing card for a Home Assistant `media_player`.
- Departures from a provider-neutral Home Assistant sensor contract.
- Calendar agenda from Home Assistant calendars.
- Touch navigation, full-screen kiosk mode, and automatic reconnect.
- Offline cache with an honest stale/offline badge.
- TOML configuration and a systemd service.
- A read-only Status page with connection, version, last update, and errors.

Home Assistant 2023.9 or newer is required (`subscribe_entities` and `weather.get_forecasts`). The kiosk can run in a different timezone from Home Assistant; calendar times are converted using the instance timezone.

## Post-v1: community extension shop

Wave 6 adds a Shop page backed by a separate, public GitHub catalog. Community extensions can add data widgets such as bus departures without shipping a new kiosk binary.

The trust boundary is deliberately small: a Minisign-signed catalog, SHA-256-checked packages, and sandboxed Rhai scripts with declared network/HA permissions and hard resource limits. Extensions cannot ship native code, arbitrary Slint UI, filesystem access, or Home Assistant service calls. See [`docs/extensions.md`](docs/extensions.md) and [`docs/extension-catalog.md`](docs/extension-catalog.md).

## Supported hardware

| Device | Support |
| --- | --- |
| Raspberry Pi Zero 2 W (512 MB) | Primary target |
| Raspberry Pi 3 Model B/B+ | Primary target |
| Generic x86_64 Linux | Development and kiosk |
| Raspberry Pi 4/5 | Works, not a release gate |
| Raspberry Pi Zero v1 (ARMv6) | Not supported |

Primary OS: Raspberry Pi OS Bookworm (64-bit for Pi Zero 2 W and Pi 3). Cross-compiled binaries require glibc 2.35 or newer; Bullseye is not supported for cross-built binaries. Build natively on Bullseye if that release is unavoidable.

## Installation

The v1 release is not published yet. The procedure below is the intended operator flow and is exercised on a clean device before the first release; the evidence is recorded with the [release checklist](docs/release-checklist.md).

### Prerequisites

- Raspberry Pi OS Bookworm, or a comparable Linux distribution with glibc 2.35 or newer for cross-built binaries.
- Home Assistant 2023.9 or newer, reachable over HTTP(S).
- A long-lived access token from a Home Assistant user. Prefer a dedicated user that can see only the entities and actions the kiosk needs; some actions require an admin user.
- A supported display and a working DRM/seat setup (LinuxKMS) or an X11 session (fallback).
- Network time synchronization and the correct device timezone. A Pi without an RTC shows the wrong time until NTP syncs.

### Install from a release artifact

```bash
# Verify the download before installing it
sha256sum -c SHA256SUMS

sudo install -d -m 0755 /usr/local/bin
sudo install -m 0755 ha-kiosk /usr/local/bin/ha-kiosk

# Runtime packages for the LinuxKMS path; seatd creates the seat group
sudo apt install libseat1 libinput10 libudev1 libxkbcommon0 libfontconfig1 \
  fonts-dejavu-core ca-certificates seatd

# Service user, groups, and directories
sudo useradd --system --home-dir /var/lib/ha-kiosk --shell /usr/sbin/nologin ha-kiosk
sudo usermod -aG video,render,input,seat ha-kiosk
sudo install -d -m 0750 -o ha-kiosk -g ha-kiosk /etc/ha-kiosk
sudo install -d -m 0700 -o ha-kiosk -g ha-kiosk /var/lib/ha-kiosk

# Configuration and token, readable only by the service user
sudo install -m 0600 -o ha-kiosk -g ha-kiosk config.toml /etc/ha-kiosk/config.toml
sudo install -m 0600 -o ha-kiosk -g ha-kiosk ha_token /etc/ha-kiosk/ha_token

# Unit
sudo install -m 0644 ha-kiosk.service /etc/systemd/system/ha-kiosk.service
sudo systemctl daemon-reload
sudo systemctl enable --now ha-kiosk
```

The release includes installer scripts in `deploy/` for the same steps: `deploy/install.sh` creates the user and directories, installs the binary, configuration, and unit, and starts the service. `deploy/uninstall.sh` stops and removes the unit and binary and keeps the configuration unless `--purge` is passed.

### Install from source

A native build avoids the cross-compilation sysroot and is the supported fallback on Bullseye:

```bash
sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev \
  libxcb-shape0-dev libxcb-xfixes0-dev libseat-dev libinput-dev libudev-dev
cargo build --release --no-default-features --features kiosk
sudo install -m 0755 target/release/ha-kiosk /usr/local/bin/ha-kiosk
```

### Release installer

```bash
sudo deploy/install.sh --binary dist/ha-kiosk-example
sudo deploy/kiosk-setup.sh --rotation 0
```

### Verify the installation

```bash
systemctl status ha-kiosk --no-pager
journalctl -u ha-kiosk -n 50 --no-pager
```

The display shows a frame within 3 seconds, the Status page shows the Home Assistant version and the last update, and the journal contains no token. If the service runs but the screen stays black, the display backend is the problem; see [Troubleshooting](#troubleshooting).

### Upgrade

```bash
sudo systemctl stop ha-kiosk
sudo install -m 0755 ha-kiosk /usr/local/bin/ha-kiosk
sudo systemctl start ha-kiosk
journalctl -u ha-kiosk -n 50 --no-pager
```

Keep the previous binary until the new version has drawn a frame and reconnected. Configuration is not rewritten by an upgrade; a new required key appears as a startup error that names the key.

### Logs

The service logs to journald:

```bash
journalctl -u ha-kiosk -f
journalctl -u ha-kiosk --since "1 hour ago"
```

`RUST_LOG=info` is the default. Raise it to `debug` in a temporary unit drop-in for diagnosis, then remove the drop-in. Debug logging costs CPU on a Pi Zero 2 W.

### Rollback

Restore the previous binary, restart the service, and confirm the frame and the connection. A cache written by a newer schema is discarded with a warning, not migrated. The full rollback rules are in the [release checklist](docs/release-checklist.md).

### Display backends

- **LinuxKMS (default):** `SLINT_BACKEND=linuxkms-software`, input through libinput and `seatd`, no X server and no window manager. Select a mode with `SLINT_DRM_MODE` and an output with `SLINT_DRM_OUTPUT`; rotate with `SLINT_KMS_ROTATION`.
- **X11 fallback:** the winit build with a minimal window manager, `unclutter` for the cursor, and `xset s off -dpms` for blanking. Without a window manager, fullscreen may not cover the screen.
- **Wayland:** supported by winit, not a release gate.

## Performance

The Pi Zero 2 W budgets are recorded in [`docs/performance.md`](docs/performance.md): resident set size below 150 MB (target below 100 MB), first frame below 3 seconds, idle CPU at most a quarter of one core, and a 24-hour soak with no leak trend or unbounded cache.

A recorded measurement is checked against those budgets with:

```bash
python3 scripts/perf-budget.py perf-record.json
```

The current status is **not verified on hardware**: no Pi Zero 2 W record exists yet, so the Wave 5 performance gate stays open. The budget thresholds and the release profile (`opt-level = "s"`, LTO, `strip`, `panic = "abort"`) are verified in CI.

## Design in one paragraph

The Slint event loop and application state live on the main thread. A Tokio runtime handles the Home Assistant WebSocket connection, service calls, image fetches, and cache writes in the background. Background tasks send typed events to the UI thread, which updates Slint models. All v1 data comes from Home Assistant; no weather, transit, or calendar provider is hardcoded. Wave 6 adds sandboxed extension data sources as the only sanctioned exception. Configuration is a TOML file, and the token is read from a protected file or an environment variable.

## Repository layout

```text
.github/                        workflows, issue/PR templates, CODEOWNERS
PLAN.md                         implementation plan and source of truth
AGENTS.md                       rules for agents and contributors
CONTRIBUTING.md                 contribution workflow and PR rules
CODE_OF_CONDUCT.md              community conduct
SECURITY.md                     vulnerability reporting
README.md                       this overview
CHANGELOG.md                    release history and unreleased changes
herdr.sh                        interactive Herdr agent dispatcher
DESIGN.md                       adopted design system (authoritative)
tokens.css                      colours, type, spacing, motion tokens
index.html                      Vite design reference prototype
styles.css                      prototype layout and components
package.json                    prototype only (Vite), not the kiosk runtime
package-lock.json
docs/
├── plan-review.md              fact-check findings and corrections
├── departures-entity-contract.md
├── deployment.md               kiosk install, seat models, upgrade, rollback
├── extensions.md               Wave 6 author guide (post-v1)
├── extension-catalog.md        Wave 6 catalog and signing spec
├── github-rules.md             repository and branch policy
├── herdr-agent.md              Herdr agent launcher guide
├── troubleshooting.md          operator troubleshooting guide
├── release-checklist.md        release gates, artifacts, and sign-off
├── agent-prompts/
│   └── implement-issue.md      implementation prompt template
└── issues/                     subtask index, mapping, and wave epic drafts
config.example.toml             safe, provider-neutral example configuration
src/                            Rust application
ui/                             Slint UI files
deploy/                         systemd unit and install scripts
scripts/                        build, development, docs and repository helpers
tests/                          integration, UI, and deployment script tests
```

## Roadmap

Waves 0–5 are the v1 release; Wave 6 is post-v1. Each wave issue is an epic; every checkbox in `PLAN.md`, including commit steps, has its own subtask issue. The mapping is in [`docs/issues/index.md`](docs/issues/index.md).

| Wave | Scope |
| --- | --- |
| [0](https://github.com/ABOYrbx/home-assistant-kiosk/issues/5) | Scaffold, interfaces, cross-compile spike |
| [1](https://github.com/ABOYrbx/home-assistant-kiosk/issues/3) | Core: config, Home Assistant client, state and cache |
| [2](https://github.com/ABOYrbx/home-assistant-kiosk/issues/2) | UI shell: navigation, touch, theme |
| [3](https://github.com/ABOYrbx/home-assistant-kiosk/issues/6) | Feature widgets: clock/weather, tiles, media, departures, calendar |
| [4](https://github.com/ABOYrbx/home-assistant-kiosk/issues/1) | Kiosk and deployment: release builds, display, systemd |
| [5](https://github.com/ABOYrbx/home-assistant-kiosk/issues/4) | Integration, performance, and release |
| [6](https://github.com/ABOYrbx/home-assistant-kiosk/issues/7) | Community extension shop (post-v1) |

## Agent dispatch (Herdr)

`./herdr.sh` is the single entry point. It selects 1–4 open subtask issues, creates a new Herdr workspace, starts one agent per issue, and opens one PR per issue. It never merges.

```bash
# Interactive: pick up to four issues
./herdr.sh

# Preselected issues
./herdr.sh --issues 11,12 --model opencode-go/deepseek-v4.1-flash

# Preview only
./herdr.sh --list
./herdr.sh --issues 11 --model opencode-go/deepseek-v4.1-flash --dry-run
```

Each agent gets its own worktree, branch, tab titled with the issue, and PR. See [`docs/herdr-agent.md`](docs/herdr-agent.md) and the prompt template [`docs/agent-prompts/implement-issue.md`](docs/agent-prompts/implement-issue.md).

## Development

Prerequisites: Rust 1.95 or newer, a C toolchain, and the Slint build dependencies. On Debian/Ubuntu:

```bash
sudo apt install build-essential pkg-config libfontconfig1-dev \
  libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libseat-dev libinput-dev libudev-dev
```

The application scaffold is created in Wave 0. Once it exists:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
cargo run --features desktop -- --config config.example.toml
```

Cross-compilation uses [`cross`](https://github.com/rust-embedded/cross) and Docker:

```bash
cross build --release --target aarch64-unknown-linux-gnu \
  --no-default-features --features kiosk
```

For release artifacts, use `scripts/build-all.sh`. It builds all three targets
with the digest-pinned images from `Cross.toml`, writes
`dist/ha-kiosk-<version>-<target>`, and generates `dist/SHA256SUMS`. The
reproducibility rules, the glibc 2.35+ floor for cross-built binaries, and
the native-build fallback on the Pi are documented in
[`docs/release-builds.md`](docs/release-builds.md).

Documentation checks run without a Rust toolchain:

```bash
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p "test_*.py"
```

The design reference prototype is a Vite app; `node_modules/` and its build output are ignored:

```bash
npm install
npm run dev
```

## Configuration

[`config.example.toml`](config.example.toml) contains placeholders only. Copy it to `/etc/ha-kiosk/config.toml`, set the Home Assistant URL and token file, and choose real entity IDs for your installation. Do not commit the resulting file. The default path is `/etc/ha-kiosk/config.toml`; `--config <path>` overrides it.

The Overview top bar shows the configured page title, the Home Assistant connection state, and the local clock. The hero card controls the `light.*` entity in `[widgets.hero_light]`: the switch calls `light.toggle`, and the brightness and colour-temperature sliders call `light.turn_on` with `brightness_pct` and `color_temp_kelvin`. Slider updates are limited to one service call per 100 ms while dragging, with the final value sent on release. The card shows loading, empty, unavailable, and error states instead of a fake "on" state.

Create the token in Home Assistant under **Profile → Security → Long-lived access tokens**. The token is shown once. Prefer a dedicated Home Assistant user with only the entities and actions the kiosk needs; some actions may require an admin user.

The token source order is `HA_TOKEN`, then `ha.token_file`, then the development-only `ha.token`. The token file and the configuration file must be mode `0600` and owned by the service user; startup rejects a world-readable token file. The cache directory is `0700` and the cache file `0600`. The permission commands are in [Installation](#installation).

Unknown keys are rejected at startup, so a typo fails loudly instead of being ignored. File paths must be absolute. `ha.url` accepts `http://` and `https://` and preserves a path prefix; the WebSocket URL is derived automatically. At least one entity must be subscribed after widget, tile, and extra entity IDs are resolved.

### Configuration reference

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `ha.url` | string | required | Home Assistant base URL, `http://` or `https://`, optional path prefix. |
| `ha.token_file` | string | unset | Absolute path to a `0600` file holding the long-lived token. |
| `ha.token` | string | unset | Development only. Prefer `token_file` or `HA_TOKEN`. |
| `ha.allow_invalid_certs` | bool | `false` | Disables TLS validation. Last resort for a local self-signed certificate. |
| `ha.time_zone` | string | unset | IANA fallback when `get_config` provides no timezone. |
| `ha.extra_entity_ids` | array of strings | `[]` | Additional entity IDs to subscribe to. |
| `display.width` | integer | `800` | Desktop window width and layout-test reference. LinuxKMS uses the DRM mode. |
| `display.height` | integer | `480` | Desktop window height and layout-test reference. |
| `display.rotation` | integer | `0` | `0`, `90`, `180`, or `270`; maps to `SLINT_KMS_ROTATION` on LinuxKMS. |
| `display.fullscreen` | bool | `true` | Fullscreen window on the desktop and X11 backends. |
| `display.theme` | string | `"dark"` | `dark` or `light`; dark is the default target. |
| `pages.<name>.title` | string | empty | Page title shown in the top bar. |
| `pages.<name>.widgets` | array of strings | per page | Widgets rendered on a page. The page set is fixed: overview, media, agenda, status. |
| `widgets.clock.enabled` | bool | `true` | Enable the clock widget. |
| `widgets.clock.show_seconds` | bool | `false` | Show seconds. |
| `widgets.clock.use_24_hour` | bool | `true` | 24-hour format. |
| `widgets.weather.enabled` | bool | `true` | Enable the weather widget. |
| `widgets.weather.entity` | string | unset | A `weather.*` entity. |
| `widgets.weather.forecast_type` | string | `"daily"` | `daily` or `hourly`. |
| `widgets.weather.forecast_count` | integer | `3` | Number of forecast entries. |
| `widgets.weather.refresh_seconds` | integer | `900` | Forecast refresh interval. |
| `widgets.media.enabled` | bool | `true` | Enable the media widget. |
| `widgets.media.entity` | string | unset | A `media_player.*` entity. |
| `widgets.media.show_album_art` | bool | `true` | Fetch and show album art. |
| `widgets.media.volume_step` | float | `0.05` | Volume slider step in the range 0.0–1.0. |
| `widgets.hero_light.enabled` | bool | `true` | Enable the hero light card. |
| `widgets.hero_light.entity` | string | unset | A `light.*` entity. |
| `widgets.hero_light.name` | string | unset | Optional display name. |
| `widgets.hero_light.min_kelvin` | integer | `2700` | Minimum colour temperature in kelvin. |
| `widgets.hero_light.max_kelvin` | integer | `6500` | Maximum colour temperature in kelvin. |
| `widgets.departures.enabled` | bool | `true` | Enable the departures widget. |
| `widgets.departures.entity` | string | unset | A sensor with the documented `departures` attribute. |
| `widgets.departures.title` | string | `"Departures"` | Widget heading. |
| `widgets.departures.max_items` | integer | `6` | Maximum rows. |
| `widgets.departures.refresh_seconds` | integer | `60` | Optional periodic refresh. |
| `widgets.calendar.enabled` | bool | `true` | Enable the calendar widget. |
| `widgets.calendar.entities` | array of strings | unset | One or more `calendar.*` entities. |
| `widgets.calendar.days` | integer | `2` | Number of days to fetch. |
| `widgets.calendar.refresh_seconds` | integer | `300` | Refresh interval. |
| `tiles.columns` | integer | `3` | Grid columns. |
| `tiles.items[].entity` | string | required | Entity ID for a tile. |
| `tiles.items[].label` | string | entity name | Tile label. |
| `tiles.items[].icon` | string | unset | Bundled icon name. |
| `cache.path` | string | required | Absolute cache file path. The directory must be writable by the service user. |
| `cache.write_debounce_seconds` | integer | `10` | Debounce for cache writes; minimum 2 seconds. |
| `cache.max_bytes` | integer | `262144` | Upper bound for the cache file. |
| `extensions.enabled` | bool | `false` | Wave 6 shop. Disabled in v1. |
| `extensions.catalog_url` | string | unset | HTTPS URL of the signed catalog index. |
| `extensions.public_keys` | array of strings | `[]` | Minisign public keys for the catalog. |
| `extensions.allow_unsigned` | bool | `false` | Development only. Never enable in production. |
| `extensions.dev_mode` | bool | `false` | Development only: permit HTTP catalog URLs. |
| `extensions.refresh_hours` | integer | `24` | Catalog refresh interval. |
| `extensions.max_catalog_bytes` | integer | `2097152` | Catalog download cap. |
| `extensions.max_package_bytes` | integer | `5242880` | Package download cap. |
| `extensions.max_script_operations` | integer | `1000000` | Rhai operation limit per run. |
| `extensions.settings.<id>` | table | unset | Per-extension settings declared by the extension. |
| `extensions.store_path` | string | required | Installed extensions and the last verified catalog index. |

The departures sensor contract is documented in [`docs/departures-entity-contract.md`](docs/departures-entity-contract.md). Supported tile domains and actions are in `PLAN.md` section 4.3; an unsupported domain renders read-only.

## Troubleshooting

The full operator guide is [`docs/troubleshooting.md`](docs/troubleshooting.md). The most common failures:

| Symptom | First check |
| --- | --- |
| Startup rejects the token or config file | Mode is `0600` and the service user owns the file. |
| "Authentication failed" and no retry | The token is invalid or revoked; create a new one. |
| Service runs but the screen is black | `SLINT_BACKEND=linuxkms-software`, `/dev/dri`, the `seat` group, and `seatd`. |
| `GLIBC_2.35 not found` | The target is Bullseye; use Bookworm or build natively. |
| Touch is rotated or offset | `display.rotation` / `SLINT_KMS_ROTATION` and the input transform. |
| Values stay after Home Assistant stops | Expected: the stale badge and last update time are shown. |
| Calendar times are shifted | `get_config` timezone, `ha.time_zone`, and the device timezone. |
| The service restarts in a loop | `journalctl -u ha-kiosk` shows the configuration or display error. |
| RSS is above budget | Album art, debug logging, or an accidental global subscription. |

## Releases and changelog

[`CHANGELOG.md`](CHANGELOG.md) records released and unreleased changes. [`docs/release-checklist.md`](docs/release-checklist.md) is the release procedure: automated gates, cross artifacts, checksums, changelog, hardware evidence, known limitations, publication, and rollback. Releases and `v*` tags are created by `ABOYrbx` from `main`. No binary is distributed while the Slint license decision is open.

The full deployment guide, including seat models, hardening, upgrade, rollback,
logs, and troubleshooting, is in [`docs/deployment.md`](docs/deployment.md).

## Assets

- **Font:** the UI uses the system sans font and does not fetch a font at runtime. Install `fonts-dejavu-core` (DejaVu Fonts License, based on the Bitstream Vera license) or bundle a licensed font and record its license here.
- **Icons:** weather and status icons are compiled into the binary from the source tree when they are added. No icon assets are bundled yet; their license and attribution must be recorded in this section before they ship.
- **Prototype:** the Vite reference prototype in `index.html` and `styles.css` uses no third-party assets.

## Security

- The Home Assistant token is never logged, committed, or sent to a third-party image host.
- The config and cache are mode `0600`; the cache directory is `0700`.
- TLS certificate validation is on by default. `allow_invalid_certs` is a documented last resort.
- Community extensions are untrusted third-party code. The catalog signature proves provenance, not safety; the shop shows permissions and a warning before installation.
- Extensions cannot access the filesystem, processes, environment, or Home Assistant services, and never receive the token.
- Report a vulnerability privately, not in a public issue. See [`SECURITY.md`](SECURITY.md) for the current reporting path.

### Media player and album art

- The media page shows the configured `media_player` entity. Transport and volume controls are disabled unless the entity advertises the matching `supported_features` bit; `volume_level` must also be reported before the slider is enabled.
- Volume changes are debounced to at most one `media_player.volume_set` call per 100 ms, with a final call on release. Transport taps are ignored while a previous call is still pending.
- Album art is fetched over HTTP(S) only. A relative `entity_picture` is resolved against `ha.url`; `data:` and `file:` URLs are rejected.
- The access token is sent only when the image URL matches the Home Assistant origin (scheme, host, and port). Redirects are followed only within that origin, and a cross-origin redirect fails the fetch instead of forwarding the token.
- Images are limited to 5 MiB, 4096 pixels per side, and 8,000,000 pixels. Larger images are rejected and the previous art stays on screen.
- Decoded art is cached by URL with an 8 MiB memory budget; the least recently used image is evicted first.
- Logs contain only the image scheme, host, and path. The query string is never logged because it can carry a signed token.

## Repository rules

- **Only `ABOYrbx` can push.** The repository is private and has no other collaborators.
- **Changes go through pull requests.** Do not push directly to `main`; do not force-push or delete `main`.
- **Squash merge only**, delete the branch after merge, keep the history linear.
- **The `docs` check must pass.** The workflow validates TOML, Markdown links, required files, and obvious personal-data leaks.
- **English only** in code, docs, issues, PRs, and commits.
- **No secrets, tokens, personal entity IDs, stop IDs, addresses, or internal hostnames** in the repository.
- **Extensions are separate.** Extension source and the catalog live in the companion `home-assistant-kiosk-extensions` repository, never in this one.
- **CODEOWNERS** routes review to `@ABOYrbx`; issue and PR templates describe the required evidence.

GitHub Free does not expose branch protection or rulesets for **private** repositories. The policy above is enforced by process, CODEOWNERS, the `docs` check, and the settings script until the repository is public or on a plan that supports protection. Details, exact commands, and the tracking issue are in [`docs/github-rules.md`](docs/github-rules.md).

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`AGENTS.md`](AGENTS.md) before changing anything. The project language is English. External contributions are welcome through pull requests once the repository is public or access is granted; only `ABOYrbx` pushes to this repository.

## License

Not chosen yet. Slint is available under GPL-3.0-only, a royalty-free license that excludes embedded systems, or a commercial license. A single-purpose kiosk may count as an embedded system, so the license must be decided before binaries are distributed. See `PLAN.md` section 12.
