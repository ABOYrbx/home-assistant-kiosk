# AGENTS.md

This file tells every agent and contributor how to work in this repository. Read it before changing code, documentation, issues, or git state.

## 1. Language

- **English only.** Code, comments, identifiers, commit messages, branch names, documentation, issues, pull-request titles and descriptions, test names, and UI strings are written in English.
- The only exception is a quotation of an external source that is already in another language; translate it and keep the original only if it is required for accuracy.
- Keep user-facing UI strings in one place so a future localization does not require touching every Slint file.
- Do not add German prose to `PLAN.md`, `README.md`, `AGENTS.md`, source files, or GitHub issues. The project language is English even though the owner speaks German.

## 2. GitHub identity and push rules

- The repository is private and belongs to the GitHub account **`ABOYrbx`**.
- **Every push must be made with the `ABOYrbx` account.** Do not push with another account, a shared account, a bot account, or an unauthenticated remote.
- Before the first push in a session, and again before a merge or release push, verify:

  ```bash
  gh auth status
  git config user.name
  git config user.email
  git remote -v
  ```

  The active `gh` account must be `ABOYrbx`. The local commit identity must be:

  ```text
  ABOYrbx
  141421787+ABOYrbx@users.noreply.github.com
  ```

- If another account is active, stop and ask the owner. Do not run `gh auth switch` on your own, and do not create a second remote.
- Never add a remote that points at a fork, mirror, or another person’s repository.
- Never force-push a shared branch. Force-push is allowed only on a private feature branch after the owner explicitly asks for it.
- Never commit credentials, tokens, private keys, `.env` files, real Home Assistant URLs, real entity IDs, real stop IDs, or personal data.
- Commits follow Conventional Commits, for example `feat: add websocket heartbeat`, `fix: handle removed attributes`, `docs: document kiosk install`.
- Issues and pull requests are in English and link to each other. A pull request states which wave and issue it closes and records the gate commands and results.

## 3. What this project is

A native Home Assistant kiosk dashboard:

- Rust 1.95+, edition 2024.
- Slint 1.19 for the UI.
- Tokio for background I/O.
- One Home Assistant WebSocket connection for state and service calls.
- TOML configuration; no provider-specific code.
- Targets: Raspberry Pi Zero 2 W, Raspberry Pi 3, generic Linux x86_64/ARM.
- Touch-first, full-screen, offline-tolerant, low memory.

Read [`PLAN.md`](PLAN.md) for the full specification. This file is the short operating manual; `PLAN.md` wins if the two disagree.

## 4. Repository layout

```text
.
├── AGENTS.md                    # this file
├── PLAN.md                      # implementation plan and source of truth
├── README.md                    # operator-facing overview
├── Cargo.toml                   # package `ha-kiosk`
├── Cargo.lock
├── Cross.toml                   # `cross` targets and Slint images
├── build.rs                     # compiles ui/main.slint
├── config.example.toml          # safe placeholders only
├── docs/
│   ├── plan-review.md           # fact-check findings and corrections
│   ├── departures-entity-contract.md
│   └── issues/                  # issue drafts for the six waves
├── src/
│   ├── main.rs                  # CLI, runtime, Slint bootstrap
│   ├── lib.rs                   # public modules for tests
│   ├── config.rs                # TOML loading and validation
│   ├── events.rs                # AppEvent, HaEvent, HaCommand
│   ├── state.rs                 # AppState and view models
│   ├── app.rs                   # AppController and navigation
│   ├── cache.rs                 # versioned atomic cache
│   ├── kiosk.rs                 # fullscreen, display, platform handling
│   ├── ha/                      # WebSocket client, models, reconnect
│   ├── sources/                 # weather, calendar, media, departures
│   └── ui/                      # bridge, view models, image decoding
├── ui/
│   ├── main.slint
│   ├── theme.slint
│   ├── pages/                   # overview, media, agenda, status
│   └── widgets/                 # clock, weather, tile, media, departures, calendar
├── deploy/                       # systemd unit and install scripts
├── scripts/                      # build-all.sh, run-dev.sh
└── tests/                        # integration and UI smoke tests
```

There is no nested `kiosk-dashboard/` directory and no multi-crate workspace in v1. The repository root is the Cargo package.

## 5. Build and test commands

```bash
# Formatting, linting, tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Desktop development (winit + software renderer)
cargo run --features desktop -- --config config.example.toml

# Kiosk build (LinuxKMS + software renderer)
cargo build --release --no-default-features --features kiosk

# Cross-compilation (requires Docker on macOS)
cross build --release --target aarch64-unknown-linux-gnu \
  --no-default-features --features kiosk
cross build --release --target armv7-unknown-linux-gnueabihf \
  --no-default-features --features kiosk
cross build --release --target x86_64-unknown-linux-gnu \
  --no-default-features --features kiosk
```

- `Cargo.lock` is committed; this is an application, not a library.
- Keep the `rust-version` field at `1.95` unless a Slint upgrade requires newer.
- Do not add a dependency without checking its license, maintenance status, and target support.
- Do not enable Slint's default features on the kiosk build. The software renderer is mandatory for the Pi Zero 2 W.
- Release profile: `opt-level = "s"`, LTO, `strip`, `panic = "abort"`. Measure before changing these.

## 6. Architecture rules

1. **Home Assistant-first.** Weather, media, calendar, tiles, and departures come from Home Assistant entities or actions. No direct weather, transit, or calendar provider integrations in v1.
2. **Provider-neutral.** Never hardcode a URL, stop ID, entity ID, address, timezone, or person’s name. Examples use placeholders such as `weather.home`, `light.example`, `calendar.family`, and `sensor.departures`.
3. **UI thread owns state.** `AppState` lives on the Slint thread. Background tasks send typed `AppEvent` values over an `mpsc` channel. Do not wrap `AppState` in a mutex and do not touch Slint handles from Tokio tasks.
4. **No blocking on the UI thread.** File I/O, image decoding, JSON parsing of large payloads, and network calls run on background tasks or the Tokio runtime.
5. **Images cross threads as `SharedPixelBuffer`.** `slint::Image` is not `Send`; never send it between threads.
6. **Subscribe narrowly.** Use `subscribe_entities` with the configured entity IDs. Do not subscribe to every `state_changed` event by default.
7. **Reconnect correctly.** Re-authenticate, re-subscribe, and replace state from a fresh snapshot after every reconnect. A bad token stops retrying.
8. **Honest state.** Show unknown, unavailable, stale, and offline states explicitly. Never render cached values as live, and never optimistically show a service call as successful before the server confirms it.
9. **Bounded memory.** Cache only the configured views and entities. Cap album-art and icon caches. Measure RSS on the Pi Zero 2 W.
10. **Graceful degradation.** A missing entity, unsupported domain, failed forecast, or failed calendar must not crash the app or blank the whole page.

## 7. Slint rules

- Pin Slint to `1.19.x`; do not mix versions of `slint`, `slint-build`, and `i-slint-backend-testing`.
- Base features: `compat-1-18`, `std`, `renderer-software`.
- Desktop feature: `backend-winit`.
- Kiosk feature: `backend-linuxkms`, `backend-linuxkms-libinput`, `backend-linuxkms-libseat`.
- PNG/JPEG and SVG support come with `std`; add `image-default-formats` only if an extra format is actually needed.
- Do not build with `--all-features` on macOS or in a non-Linux CI job: the LinuxKMS backend is Linux-only.
- LinuxKMS is primary for the kiosk; X11 + winit is the fallback. `SLINT_BACKEND=linuxkms-software` selects the direct-to-DRM path.
- Slint 1.19 does not expose a cursor-visibility API. Cursor hiding belongs to deployment: `unclutter` on X11, compositor settings on Wayland, and no cursor under LinuxKMS software rendering.
- Use `slint::Window::set_fullscreen(true)` for fullscreen. Rotation is a deployment/backend concern (`SLINT_KMS_ROTATION`).
- UI tests use `i-slint-backend-testing`; do not require a real display in CI.
- Do not add continuous animations. A 1 Hz clock is the expected baseline.

## 8. Coding conventions

- `rustfmt` defaults; no custom style debates in review.
- `clippy` with `-D warnings` is part of the gate.
- `thiserror` for library/module errors, `anyhow` only in `main.rs` and scripts.
- `tracing` for logs; `RUST_LOG=info` by default. Logs go to journald under systemd.
- Never log the token, authorization header, full WebSocket URL with credentials, or the contents of the token file.
- Prefer explicit types at module boundaries; keep view models free of Slint types so they can be tested.
- Use `#[serde(deny_unknown_fields)]` for configuration and cache schemas to catch drift.
- Write tests with small recorded fixtures; never call a live Home Assistant from a test.
- Add a test for every bug fix that changes behavior.
- Keep functions focused; split a module when it stops fitting in one review.

## 9. Security

- Token sources, in order: `HA_TOKEN` environment variable, `ha.token_file`, `ha.token` (development only).
- The token file must be owned by the service user and mode `0600`. Startup rejects a world-readable token file.
- TLS certificate validation is on by default. `allow_invalid_certs = true` is a documented last resort for a self-signed LAN certificate; prefer installing the CA on the device.
- The config file is mode `0600` and owned by the service user. The systemd unit must not run as root.
- No secrets in test fixtures, screenshots, issue bodies, commit messages, or logs.
- Release artifacts never bundle a config file or a token.

## 10. Issues, branches, and pull requests

- Work one wave per issue. The issue body is the checklist; `PLAN.md` is the specification.
- Branch naming: `wave-0/scaffold`, `wave-1/ha-client`, `wave-3/media`, and so on.
- One pull request per wave unless the wave is split by an explicit agreement. A pull request records:
  - the issue it closes,
  - the commands run and their result,
  - any manual/hardware test evidence,
  - known limitations.
- Do not mark a wave complete while a gate is unverified. If hardware is unavailable, write “not verified on hardware” and list what was tested instead.
- Reviewers check the gate, not only the diff. A green unit test suite does not prove the Pi memory budget or the display path.

## 11. Definition of done

A task is done when:

- [ ] The code builds with the declared feature set.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.
- [ ] New behavior has tests or a recorded manual test.
- [ ] Error, offline, empty, and unavailable states are handled.
- [ ] No secret, personal identifier, or provider-specific URL was added.
- [ ] Documentation and `config.example.toml` match the behavior.
- [ ] The commit messages and issue/PR text are in English.
- [ ] The push used the `ABOYrbx` account.

## 12. License note

Slint is available under GPL-3.0-only, a royalty-free license that excludes embedded systems, or a commercial license. A single-purpose kiosk device may be classified as an embedded system. Do not add a project license or distribute binaries until the owner records the license decision in `PLAN.md` section 12. This is a legal decision, not an implementation detail.
