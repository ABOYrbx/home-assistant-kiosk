# Home Assistant Kiosk

A small, native Home Assistant dashboard for wall-mounted touchscreens and single-purpose Linux devices. It is written in Rust with Slint, uses the Home Assistant WebSocket API, and is designed to stay useful on a Raspberry Pi Zero 2 W with 512 MB RAM.

[![Docs check](https://github.com/ABOYrbx/home-assistant-kiosk/actions/workflows/docs.yml/badge.svg)](https://github.com/ABOYrbx/home-assistant-kiosk/actions/workflows/docs.yml)

**Status:** planning. The repository currently contains the implementation plan, project rules, and documentation, not a working application. The plan is in [`PLAN.md`](PLAN.md).

## Why this exists

A browser-based dashboard is convenient but expensive on a Pi Zero 2 W: a browser, a compositor, and a frontend bundle consume most of the available memory. This project uses one native process, one Home Assistant WebSocket connection, and a software renderer. It keeps the last known state on disk so the display survives a network or Home Assistant outage.

## Design

The visual design is adopted, not a placeholder. [`DESIGN.md`](DESIGN.md) and [`tokens.css`](tokens.css) are the authoritative design system; [`index.html`](index.html) and [`styles.css`](styles.css) are the Vite reference prototype. The production kiosk is Slint, not a browser.

The Overview follows the prototype: top bar, hero light with brightness/colour temperature, scenes/quick actions, and a configurable rooms/tile group. The layout must also work at 800 × 480 and 1024 × 600. Energy and Schedule are post-v1. The prototype's mock values are not production data.

## Planned v1 features

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

Primary OS: Raspberry Pi OS Bookworm (64-bit for Pi Zero 2 W and Pi 3). Cross-compiled binaries require glibc 2.35 or newer; Bullseye is not supported for cross-built binaries.

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
DESIGN.md                       adopted design system (authoritative)
tokens.css                      colours, type, spacing, motion tokens
index.html                      Vite design reference prototype
styles.css                      prototype layout and components
package.json                    prototype only (Vite), not the kiosk runtime
package-lock.json
docs/
├── plan-review.md              fact-check findings and corrections
├── departures-entity-contract.md
├── extensions.md               Wave 6 author guide (post-v1)
├── extension-catalog.md        Wave 6 catalog and signing spec
├── github-rules.md             repository and branch policy
├── herdr-agent.md              Herdr agent launcher guide
├── agent-prompts/
│   └── implement-issue.md      implementation prompt template
└── issues/                     subtask index, mapping, and wave epic drafts
config.example.toml             safe, provider-neutral example configuration
src/                            Rust application (once Wave 0 starts)
ui/                             Slint UI files (once Wave 0 starts)
deploy/                         systemd unit and install scripts
scripts/                        build, development, docs and repository helpers
tests/                          integration and UI tests
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

For up to four issues in a new Herdr workspace:

```bash
scripts/herdr-issue-team.sh
```

For one subtask issue:

```bash
scripts/herdr-issue-agent-interactive.sh
```

Or non-interactively:

```bash
scripts/herdr-issue-agent.sh --issue 11 --model opencode-go/deepseek-v4.1-flash
```

The launcher creates worktrees/branches, starts the agents in Herdr, waits for them, runs the gates, and opens a PR per issue. It never merges. See [`docs/herdr-agent.md`](docs/herdr-agent.md).

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

Documentation checks run without a Rust toolchain:

```bash
python3 scripts/check-docs.py
```

The design reference prototype is a Vite app; `node_modules/` and its build output are ignored:

```bash
npm install
npm run dev
```

## Configuration

[`config.example.toml`](config.example.toml) contains placeholders only. Copy it to `/etc/ha-kiosk/config.toml`, set the Home Assistant URL and token file, and choose real entity IDs for your installation. Do not commit the resulting file.

Create the token in Home Assistant under **Profile → Security → Long-lived access tokens**. The token is shown once. Prefer a dedicated Home Assistant user with only the entities and actions the kiosk needs; some actions may require an admin user.

The token file must be readable only by the service user. The cache directory is `0700` and the cache file `0600`:

```bash
sudo install -d -m 0750 -o ha-kiosk -g ha-kiosk /etc/ha-kiosk
sudo install -d -m 0700 -o ha-kiosk -g ha-kiosk /var/lib/ha-kiosk
sudo install -m 0600 -o ha-kiosk -g ha-kiosk ha_token /etc/ha-kiosk/ha_token
sudo install -m 0600 -o ha-kiosk -g ha-kiosk config.toml /etc/ha-kiosk/config.toml
```

The default kiosk deployment uses LinuxKMS with `seatd`; the X11 fallback needs a minimal window manager and `unclutter`. Install `fonts-dejavu-core` (or bundle a licensed font) and the runtime libraries `libseat1`, `libinput10`, `libudev1`, `libxkbcommon0`, `libfontconfig1`, plus `ca-certificates`. Ensure NTP and the device timezone are configured; a Pi without an RTC otherwise shows the wrong clock after boot. The exact install steps are produced in Wave 4.

## Security

- The Home Assistant token is never logged, committed, or sent to a third-party image host.
- The config and cache are mode `0600`; the cache directory is `0700`.
- TLS certificate validation is on by default. `allow_invalid_certs` is a documented last resort.
- Community extensions are untrusted third-party code. The catalog signature proves provenance, not safety; the shop shows permissions and a warning before installation.
- Extensions cannot access the filesystem, processes, environment, or Home Assistant services, and never receive the token.
- Report a vulnerability privately, not in a public issue. See [`SECURITY.md`](SECURITY.md) for the current reporting path.

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
