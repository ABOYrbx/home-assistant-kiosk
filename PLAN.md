# Home Assistant Kiosk Dashboard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Track implementation with the checkbox syntax (`- [ ]`) and with the GitHub issues linked at the end of this document.

**Goal:** A small, native kiosk dashboard written in Rust and Slint. It runs on a Raspberry Pi 3, a Pi Zero 2 W, and ordinary Linux systems (x86_64/ARM), talks to Home Assistant over its WebSocket API, and shows clock, weather, Home Assistant tiles, media, departures, and calendar events. It is touch-first, works as a full-screen kiosk, and keeps the last known state when Home Assistant is offline.

**Architecture:** One native Rust binary with a Slint UI. The Slint event loop and the application state live on the main thread. A Tokio runtime runs in the background for the Home Assistant WebSocket connection, service calls, HTTP image fetches, and cache writes. Background tasks send typed events to the UI thread; the UI thread applies them to Slint models and properties. All v1 data is Home Assistant-first and configuration-driven: no provider-specific URLs, stop IDs, entity IDs, or personal data are compiled into the program. Wave 6 adds sandboxed, permissioned data extensions as the only sanctioned exception.

**Tech stack:** Rust 1.95+ (edition 2024), Slint 1.19 (software renderer; winit for desktop development, LinuxKMS for the kiosk), tokio 1.54, tokio-tungstenite 0.30, serde/serde_json, toml 1.2, reqwest 0.13 with rustls, chrono/chrono-tz, tracing, thiserror/anyhow, secrecy.

**Spec:** This file is the source of truth. The fact-check and change log is in [`docs/plan-review.md`](docs/plan-review.md). The generic departures entity contract is in [`docs/departures-entity-contract.md`](docs/departures-entity-contract.md). Repository rules for agents are in [`AGENTS.md`](AGENTS.md).

**Status:** Planning complete; no application code yet. Revision 5, 2026-11-23 (design template adopted).

---

## 1. Product definition

### In scope for v1

- Clock and date, updated once per second.
- Current weather and a short forecast from a Home Assistant `weather` entity.
- A configurable grid of Home Assistant tiles (switch, light, climate, cover, scene, script, and read-only sensors).
- An Overview based on the adopted minimal design: hero light with brightness and colour temperature, scenes/quick actions, and a configurable rooms/tile group.
- Now-playing card for a Home Assistant `media_player`: title, artist, album art, play/pause, previous/next, volume.
- Departures list from a documented, provider-neutral Home Assistant entity contract.
- Calendar events from one or more Home Assistant `calendar` entities.
- Touch navigation between pages: Overview, Media, Agenda, Status.
- Full-screen kiosk mode with no window manager on LinuxKMS, plus an X11 fallback.
- Offline mode: show the last cached state, mark it as stale, and reconnect automatically.
- TOML configuration, systemd service, install script, cross-build script, and documentation.

### Explicitly out of scope for v1

- Editing Home Assistant configuration or dashboards.
- An on-screen keyboard or in-app configuration editor.
- Direct weather, transit, or calendar provider APIs in v1. Providers stay behind Home Assistant integrations; Wave 6 may add sandboxed provider data sources through the extension shop.
- User accounts, multi-user profiles, or cloud services.
- Video playback, camera streams, or audio output on the kiosk itself.
- Localization. The UI is English in v1; all user-facing strings live in one place so a later translation is possible.

### Post-v1 roadmap

- **Wave 6 — Community extension shop:** a separate, signed GitHub catalog of sandboxed data extensions (for example bus departures). It adds a fifth Shop page and does not change the v1 gate. Design in section 4.6.

### Supported hardware and operating systems

| Target | Status | Notes |
| --- | --- | --- |
| Raspberry Pi Zero 2 W, 512 MB | Primary | Raspberry Pi OS Bookworm, 64-bit. LinuxKMS software renderer. |
| Raspberry Pi 3 Model B/B+, 1 GB | Primary | Bookworm 64-bit or 32-bit. |
| Generic x86_64 Linux | Development and kiosk | Bookworm or newer. |
| Raspberry Pi Zero (v1, ARMv6) | Not supported | The selected targets start at ARMv7; ARMv6 is out of scope. |
| Raspberry Pi 4/5 | Works, not a gate | More headroom than the target devices. |

Cross-compiled binaries are built against a glibc 2.35 sysroot (Slint's cross images use Ubuntu 22.04). They therefore require a target with glibc 2.35 or newer. Raspberry Pi OS Bookworm ships glibc 2.36. Raspberry Pi OS Bullseye (glibc 2.31) is **not** supported for cross-built binaries; build natively on Bullseye if it must be used.

---

## 2. Decisions

| # | Area | Decision | Reason |
| --- | --- | --- | --- |
| 1 | Rendering | Native Slint app, not a web kiosk | Low memory and CPU use on a Pi Zero 2 W; no browser stack. |
| 2 | Language | Rust, edition 2024 | Memory safety, small binaries, good cross-compilation. |
| 3 | Data source | Home Assistant-first | One WebSocket connection covers weather, media, calendar, entities, and departures. No per-provider API keys. |
| 4 | Provider neutrality | No hardcoded providers, URLs, stop IDs, or entity IDs | The project must work for any Home Assistant user. Everything is configured in TOML. |
| 5 | Auth | Long-lived access token, read from a file or environment variable | Token is never stored in the repository or printed in logs. |
| 6 | Offline behaviour | Last state from a local cache plus a visible stale/offline badge | The display remains useful during network or Home Assistant outages. |
| 7 | Input | Touch-first; minimum target 44 × 44 logical pixels | 7-inch displays and fingers, not a mouse. |
| 8 | Configuration | TOML file, validated at startup | Human-editable, explicit, and testable. |
| 9 | Navigation | Tab bar plus horizontal swipe | Works with touch and with a mouse during development. |
| 10 | Deployment | systemd service plus install script | Survives reboot, restarts on failure. |
| 11 | Transport | Home Assistant WebSocket for state and service calls; HTTP(S) only for entity images | One authenticated channel; fewer moving parts. |
| 12 | Kiosk backend | LinuxKMS with the software renderer as primary; X11 + winit as fallback | No X server or GPU needed on the Pi; a fallback keeps desktop Linux usable. |
| 13 | Cross-compilation | `cross` with pinned Slint cross images, plus a native-build path on the Pi | Slint maintains images with the required C libraries; native builds are the fallback. |
| 14 | UI scope | Four pages: Overview, Media, Agenda, Status | Status is read-only; configuration is done in the TOML file. |
| 15 | Language | English for code, comments, docs, commits, issues, and UI strings | One language for contributors and reviewers. |
| 16 | State ownership | The UI thread owns `AppState`; background tasks only send events | Avoids shared mutable state and cross-thread Slint access. |
| 17 | Seat access | `seatd` + system service with the service user in the `seat` group; alternatives documented | Non-root DRM/input access without an autologin session. |
| 18 | Timezone | Use `get_config.time_zone`; `ha.time_zone` is a fallback | Calendar times stay correct when the kiosk and Home Assistant differ. |
| 19 | Extensions | Signed catalog + sandboxed Rhai data scripts + declarative widgets | Bus departures and custom lists without native code or arbitrary UI on a 512 MB device. |
| 20 | Catalog trust | Separate public GitHub repository; Minisign-signed index; SHA-256 packages | Provenance without a private signing key in CI or the kiosk repo. |
| 21 | Shop page | Wave 6 adds a fifth page only when extensions are enabled; v1 remains four pages | Keeps the v1 release gate unchanged. |
| 22 | Design system | Adopt `DESIGN.md` + `tokens.css` and the Overview prototype as the visual reference; production UI is Slint | The owner chose the existing minimal prototype; it replaces the placeholder theme. |

---

## 3. Global constraints

- **Memory:** Raspberry Pi Zero 2 W has 512 MB RAM. Hard gate: resident set size below 150 MB. Target: below 100 MB at idle with cached data.
- **Startup:** First frame within 3 seconds using cached data. Home Assistant connection happens asynchronously and must not block the first frame.
- **Rendering:** No continuous animations. The clock updates once per second. Swipe/scroll should stay responsive; do not promise 60 fps on a Pi Zero 2 W software renderer.
- **Slint version:** Pin `slint = "1.19"` and `slint-build = "1.19"`. Do not upgrade a minor version inside a wave without a separate review.
- **Rust version:** `rust-version = "1.95"` in `Cargo.toml`. Slint's current build documentation requires at least Rust 1.95.
- **Slint features:** `default-features = false`; base features `compat-1-18`, `std`, `renderer-software`. Desktop adds `backend-winit`; kiosk adds `backend-linuxkms`, `backend-linuxkms-libinput`, and `backend-linuxkms-libseat`. Bundled icons are compiled at build time via `@image-url`; runtime album art is decoded with the `image` crate. Do not rely on `slint::Image::load_from_path` unless the runtime decoder feature is verified. Do not build with `--all-features` on macOS: the LinuxKMS backend is Linux-only.
- **Home Assistant minimum:** 2023.9, because `subscribe_entities` and `weather.get_forecasts` (introduced in 2023.9) are required. The UI must degrade gracefully if an action is not supported.
- **Security:** The token is never logged, never included in error messages, and never committed. Use a dedicated Home Assistant user for the kiosk and grant only the entities/actions it needs; some actions may require an admin user. The config file must be mode `0600` and owned by the service user; the cache is `0600`. TLS certificate validation stays on by default. Send the token only to the Home Assistant origin; never to third-party image hosts. Do not log image query strings.
- **Assets:** Bundle a font or declare the system font package as a runtime dependency (`fonts-dejavu-core` on Debian). Use an icon set with a documented, compatible license and record the attribution in `README.md`. No runtime icon downloads.
- **No personal data:** Examples use placeholders such as `light.example`, `weather.home`, `calendar.family`, and `sensor.departures`. Do not add a real address, stop ID, token, internal hostname, or person’s name to the repository.
- **Timeouts:** Every network operation has a timeout. WebSocket reconnect uses exponential backoff with jitter, capped at 60 seconds.
- **Design:** `DESIGN.md` and `tokens.css` are authoritative. Use tokens for colour, type, space, radius, and motion; no hardcoded values. The Vite prototype is a reference, not production UI. Check 800 × 480, 1024 × 600, and 1280+.
- **Touch:** Interactive elements are at least 44 × 44 logical pixels. Text must remain readable at 800 × 480 and at 1024 × 600.
- **Builds:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` must pass before a wave is accepted.
- **License:** Slint is available under GPL-3.0-only, a royalty-free license that excludes embedded systems, or a commercial license. Do not add a project license or distribute binaries until the license choice is recorded. See section 12.

---

## 4. Architecture

### 4.1 Runtime shape

```text
┌──────────────────────────────── main thread ────────────────────────────────┐
│ Slint event loop                                                            │
│   AppController ──owns──> AppState ──updates──> Slint models / properties   │
│        ▲                                                                    │
│        │ AppEvent (mpsc, Send)                                              │
└────────┼────────────────────────────────────────────────────────────────────┘
         │
┌────────┴────────────────────────── Tokio runtime ───────────────────────────┐
│ HaTask: connect, auth, subscribe_entities, call_service, ping/pong, backoff │
│ SchedulerTask: weather forecast, calendar, departures refresh               │
│ ImageTask: fetch and decode album art into SharedPixelBuffer                │
│ CacheTask: debounced atomic snapshot writes                                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

The one-second clock runs on the UI thread with a Slint timer; it does not travel through the network scheduler.

Rules:

- `AppState` is owned by the main thread. It is not wrapped in a mutex and is not shared with Tokio tasks.
- Background tasks send `AppEvent` values over `tokio::sync::mpsc`. The UI thread drains them and updates Slint from `slint::invoke_from_event_loop`.
- Bursts of Home Assistant state changes are coalesced before a redraw. Update at most four times per second; the clock is a separate one-second timer.
- Blocking work (file I/O, image decoding) never runs on the UI thread.
- A panic in a background task is logged and the task is restarted by its supervisor, except for invalid configuration or an invalid token, which stop the retry loop and show an error state.

### 4.2 Home Assistant connection

1. Derive the WebSocket URL from `ha.url` by appending `api/websocket` while preserving any path prefix. Examples: `http://host:8123` → `ws://host:8123/api/websocket`; `https://ha.example.com/ha` → `wss://ha.example.com/ha/api/websocket`. Do not assume the instance is at the root of the host or on port 8123.
2. Wait for `auth_required`, send `auth` with the access token.
3. Require `auth_ok`. On `auth_invalid`, stop reconnecting and show “Authentication failed”; do not hammer the server with a bad token.
4. After `auth_ok`, take `ha_version` from the auth response and call `get_config` once to read `time_zone` (IANA name). Use the Home Assistant timezone to interpret calendar timestamps that have no offset; fall back to `ha.time_zone` from the config, then the device timezone. Show the Home Assistant version on the Status page.
5. Subscribe with `subscribe_entities` and the explicit list of entity IDs needed by the configured widgets. Do **not** subscribe to every `state_changed` event; that traffic is unnecessary on a Pi.
6. Apply the initial snapshot from `subscribe_entities`, then apply `a` (added), `c` (changed), and `r` (removed) events. Handle removed attributes as well as changed values.
7. `get_states` remains available for diagnostics and tests. Do not implement a global `subscribe_events("state_changed")` fallback in v1: Home Assistant 2023.9 and newer support `subscribe_entities`, which is the declared minimum.
8. Send `ping` every 30 seconds and expect `pong`. A missed heartbeat or a closed socket triggers reconnect with exponential backoff and jitter (1 s, 2 s, 4 s, … 60 s).
9. After a reconnect, authenticate again, re-subscribe, and replace the local entity map with the fresh snapshot. Never assume subscriptions survive a disconnect.
10. Service calls use `call_service` with `target` and `service_data`. Calls that return data (weather forecast, calendar events) set `return_response: true`. Every service call carries a UI-generated `request_id`; the result is matched back to the pending action so the tile or control can leave its pending state honestly.

### 4.3 Widget data mapping

| Widget | Home Assistant source | Refresh / update |
| --- | --- | --- |
| Clock | Local system time | 1 s timer; no network |
| Weather | `weather.*` entity state and `weather.get_forecasts` | State via subscription; forecast every 15 min (configurable) |
| Tiles | Configured entity IDs | State via subscription; tap sends a domain-specific action |
| Media | `media_player.*` entity and attributes | State via subscription; album art fetched when `entity_picture` changes |
| Departures | Configured sensor entity with the documented `departures` attribute | State via subscription; optional periodic refresh every 60 s |
| Calendar | `calendar.*` entities via `calendar.get_events` | Every 5 min (configurable), plus on reconnect |

Supported tile actions in v1:

| Domain | Tap action | Notes |
| --- | --- | --- |
| `switch`, `input_boolean` | `homeassistant.toggle` | Show current state. |
| `light` | `light.toggle`; hero uses `light.turn_on` with `brightness_pct` and `color_temp_kelvin` | Configured light only; sliders debounced; no colour wheel in v1. |
| `scene`, `script` | `scene.turn_on`, `script.turn_on` | Momentary action; no state toggle. |
| `cover` | `cover.open_cover` / `cover.close_cover` | Choose by current state. |
| `climate` | Read-only in v1 | Temperature display; setpoint control is a later feature. |
| `sensor`, `binary_sensor`, `weather` | Read-only | No tap action. |
| Anything else | Read-only | Render state and attributes that fit; never guess an action. |

Unknown or unsupported domains must render as read-only tiles. A failed service call shows a short error on the tile and in the status page; it never leaves the UI in a fake “on” state.

Media controls use `media_player.media_play_pause`, `media_player.media_next_track`, `media_player.media_previous_track`, and `media_player.volume_set` with `volume_level` in the range 0.0–1.0. Check `supported_features` and disable controls the entity does not advertise. Debounce volume changes as described in section 4.5.

Album art: resolve a relative `entity_picture` against `ha.url`; send the access token only to URLs whose scheme, host, and port match the Home Assistant origin. Never send the token to a third-party image host. Log only the image host and path, never the query string, which may contain a signed token.

Calendar calls use `calendar.get_events` with `start_date_time` and `end_date_time` (ISO 8601, Home Assistant local timezone) or a `duration`, and `return_response: true`. Timestamps without an offset are interpreted in the Home Assistant timezone from `get_config`, then converted to the device timezone for display. Weather calls use `weather.get_forecasts` with `type = "daily"` or `"hourly"`, and `return_response: true`.

### 4.4 Offline and stale data

- On startup, load the cache before opening the window; the first frame shows cached views.
- With no cache, the first frame shows a labelled “Connecting…” placeholder per widget, not a blank screen.
- Every view carries `last_success: Option<DateTime<Utc>>` and a `stale: bool`.
- If Home Assistant is unreachable, keep the last values, set `stale = true`, and show an “Offline — last update HH:MM” badge.
- Do not show old values as if they were live. Clock and date are always local and always live.
- On reconnect, replace stale values with the fresh snapshot and clear the badge.
- The cache stores only the views needed by the configured widgets, not the full Home Assistant state database. Include a schema version and a configuration fingerprint (widget/tile/entity IDs and refresh settings); discard a cache whose fingerprint or schema does not match.
- The cache directory is mode `0700` and the cache file `0600`. The cache can reveal presence, temperatures, and media habits even though it contains no token.

### 4.5 Touch interaction

- Page swipe starts only from a non-interactive background area or an edge zone. It must not start from a slider, a scrollable list, a button, or a tile.
- Long lists (departures, calendar) use a vertical `Flickable` or equivalent; page swipe and list scroll must not fight each other.
- Gesture rules: horizontal movement beyond the swipe threshold changes page; vertical movement scrolls the list; a tap activates the control. Axis lock prevents accidental page changes during scrolling.
- Volume sliders and tile taps are debounced: send at most one service call per 100 ms while dragging, and a final value on release. Ignore repeated taps on a tile while its action is pending.
- Text in tiles, media, departures, and calendar rows is elided or wrapped to a fixed number of lines; long titles must not push the layout off-screen.
- Test at 800 × 480 and 1024 × 600 logical pixels, with touch emulation in the Slint testing backend.

### 4.6 Extension host (Wave 6, post-v1)

The community extension shop is a post-v1 feature. It must not change the v1 release gate.

**Purpose:** a separate, public GitHub repository acts as a catalog. It lists community extensions that add data widgets, such as bus departures, without shipping a new kiosk binary.

**Chosen model:** signed catalog + sandboxed Rhai data scripts + a small declarative output schema. Extensions do **not** ship native code, Slint files, or arbitrary UI. They return one of three view models: `departures`, `list`, or `key_value`. The existing built-in widgets render those models. This covers bus departures and custom lists while keeping the trust boundary small on a 512 MB device.

Alternatives considered:

| Model | Trade-off | Decision |
| --- | --- | --- |
| Declarative JSON mapping only | Safest, but cannot express the parsing quirks of community transit APIs | Not enough for the stated use case |
| WASM plugins | Strong sandbox, but a large runtime and compile cost on a Pi Zero 2 W | Too heavy for v1 |
| Rhai scripts with declarative widgets | Small pure-Rust runtime, no default I/O, host-controlled permissions and resource limits | **Chosen** |

**Catalog trust:**

- The catalog is a separate repository with `index.json` and `index.json.minisig`. The index is signed offline with Minisign; the 0600 private key never enters the kiosk repo or CI.
- Configured Minisign public keys are the trust anchor. `allow_unsigned` exists only for local development and is off by default.
- Every package has a SHA-256 checksum and a size in the signed index. Download over HTTPS only.
- Package extraction rejects absolute paths, `..`, symlinks, hardlinks, device files, and decompressed-size bombs.
- The app caches the last verified index; an offline shop can still show and run installed extensions.

**Script sandbox:**

- Rhai has no built-in filesystem, process, environment, or network access. The host registers only:
  - `http_get_json(url)`: GET only, HTTPS, allowlisted host, no credentials, 10 s timeout, 1 MB response cap, no cross-host redirects.
  - `ha_state(entity_id)`: read-only, allowlisted entity IDs, snapshot only; **no service calls**.
  - `now()` and `log(message)`.
- Engine limits: 1,000,000 operations, max string 64 KB, max array/map 1,000 entries, max call depth, no modules/imports, 10 s wall-clock cap.
- Output is validated against the declared `provides` kind and capped at 100 items; unknown or malformed fields are rejected.
- A failing extension keeps its last good data and shows an error; repeated failures mark it degraded, never crash the app.

**Permissions and configuration:**

- The manifest declares `http_hosts` and `ha_entities`; the shop shows them before installation and the host enforces them.
- Configuration lives in `config.toml` under `[extensions.settings."<id>"]`; secret fields live in a per-extension file with mode `0600`. No on-screen keyboard is required: the shop shows the required keys and the file path.
- Extensions never receive the Home Assistant token.

**Lifecycle:**

- Installed extensions live under `/var/lib/ha-kiosk/extensions/<id>/<version>/`; installation is atomic, the previous version is kept for rollback, and removal keeps settings unless explicitly purged.
- Catalog refresh runs daily and on manual request; installed extensions continue to work offline.
- `api_version = 1`; `min_app_version` and semver guard compatibility. A downgrade requires an explicit action.

**Shop UI:** a fifth page, added only when `extensions.enabled = true` in Wave 6. It lists installed and available extensions, version, author, license, permissions, configuration status, and install/update/remove/enable actions. A first-install confirmation explains that community extensions are third-party code.

**Out of scope for extensions:** native code, arbitrary Slint UI, Home Assistant service calls, filesystem/process/env access, arbitrary secrets, and access to the kiosk's config or token.

**Companion repository:** [`ABOYrbx/home-assistant-kiosk-extensions`](https://github.com/ABOYrbx/home-assistant-kiosk-extensions) is separate from this repo and public for community use. It contains the catalog schema, extension sources, release packaging, validation CI, and the offline signing instructions. `ABOYrbx` is the designated production-key custodian and will generate the Minisign private key on encrypted offline storage before the first real release; the decision is recorded in section 12.

### 4.7 Design system and Overview (adopted)

The kiosk uses the minimal design system in [`DESIGN.md`](DESIGN.md) and [`tokens.css`](tokens.css). The Vite prototype in [`index.html`](index.html) and [`styles.css`](styles.css) is the visual reference; the production UI is Slint. Adopted on 2026-11-23.

- The visual language is authoritative: white/warm surfaces, near-black accent, Inter/system sans, thin borders, generous whitespace, one warm light glow.
- The **Overview** page follows the prototype: top bar (brand, connection, clock/date), hero light card with toggle + brightness + colour temperature, scenes/quick actions, and a configurable rooms/tile group.
- **Media, Agenda, and Status** remain separate pages and use the same tokens and components.
- **Energy** and **Schedule** cards are post-v1; they need data sources that are not part of the v1 gate.
- Breakpoints: compact 800 × 480, two-column 1024 × 600, three-column 1280+; no horizontal page scroll; tap targets at least 44 × 44 px.
- The prototype's mock values are not production data. Rooms, scenes, lights, and metrics must come from Home Assistant or configuration; never hardcode them.
- Accessibility: AA contrast, visible focus, labelled controls, five data states (Loading, Empty, Error, Populated, Edge), reduced-motion equivalent.
- Slint adaptation: port tokens to `ui/theme.slint`; use flat colours and simple gradients instead of CSS `color-mix`/blur; bundle Inter with its OFL licence or use the system sans fallback; simplify the lamp glow for the software renderer.
- The prototype and the Slint port must be kept in sync. A design change updates `DESIGN.md`, `tokens.css`, the prototype, and the Slint implementation in one change.

---

## 5. Configuration

The default path is `/etc/ha-kiosk/config.toml`. A `--config <path>` argument overrides it. The example file is [`config.example.toml`](config.example.toml).

Design rules:

- `ha.token_file` is the normal source for a service. For development, the `HA_TOKEN` environment variable overrides it. A literal `ha.token` is accepted for development but is discouraged.
- The token file must be readable only by the service user (`0600`). Startup fails with a clear error if permissions are too open.
- `ha.url` must be `http://` or `https://`; `ws://`/`wss://` are derived internally. Preserve a path prefix for instances behind a reverse proxy.
- `ha.token_file`, `cache.path`, and any future file paths are absolute; relative paths are rejected at startup.
- `ha.time_zone` is an optional IANA fallback. The primary source is `get_config.time_zone`; the last fallback is the device timezone.
- Entity IDs are validated syntactically (`domain.object_id`) but not checked against the server at startup. Missing entities are reported per widget, not as a fatal error.
- At least one subscribed entity is required after resolving widget, tile, and extra IDs; an empty subscription list is a configuration error.
- Unknown keys are rejected (serde `deny_unknown_fields`) to catch typos.
- Durations are in seconds and have documented minimums.
- `display.rotation` accepts `0`, `90`, `180`, `270`. On LinuxKMS it maps to `SLINT_KMS_ROTATION`; touch calibration is a deployment concern, documented in the README.
- `display.width`/`height` size the desktop window and drive layout tests. On LinuxKMS the DRM mode wins; use `SLINT_DRM_MODE` to select a mode. Layouts must be responsive rather than assuming one exact resolution.
- No default entity IDs are shipped. A widget whose required entity is absent is hidden or shows a configuration hint.
- `[extensions]` is disabled in v1. Wave 6 enables it only when a signed catalog and public keys are configured; `allow_unsigned`/`dev_mode` are development-only.

---

## 6. Repository layout

The repository root **is** the Cargo package. There is no nested project folder and no multi-crate workspace in v1. A single crate with `lib.rs` and `main.rs` keeps the cross-build simple and the interfaces testable.

```text
home-assistant-kiosk/
├── AGENTS.md
├── PLAN.md
├── README.md
├── DESIGN.md                    # adopted visual design system (authoritative)
├── tokens.css                  # colour, type, space, motion tokens
├── index.html                  # Vite design reference prototype
├── styles.css                  # prototype layout and components
├── package.json                # prototype only (Vite); not the kiosk runtime
├── package-lock.json
├── Cargo.toml
├── Cargo.lock
├── Cross.toml
├── build.rs
├── config.example.toml
├── .gitignore
├── .github/workflows/ci.yml
├── docs/
│   ├── plan-review.md
│   ├── departures-entity-contract.md
│   ├── extensions.md      # Wave 6 author guide
│   ├── extension-catalog.md # Wave 6 catalog/signing spec
│   └── issues/
│       ├── index.md       # subtask issue map
│       ├── subtasks.json
│       └── wave-0.md … wave-6.md
├── src/
│   ├── main.rs            # entry point, CLI, runtime, Slint bootstrap
│   ├── lib.rs             # public modules for tests
│   ├── config.rs          # TOML loading, validation, redacted secrets
│   ├── events.rs          # AppEvent / HaEvent / HaCommand
│   ├── state.rs           # AppState and view models
│   ├── app.rs             # AppController: event loop, page navigation
│   ├── cache.rs           # versioned atomic snapshot cache
│   ├── kiosk.rs           # fullscreen, display rotation, platform notes
│   ├── ha/
│   │   ├── mod.rs
│   │   ├── client.rs      # auth, subscriptions, commands, heartbeat
│   │   ├── models.rs      # HA entities, compact subscription format
│   │   └── reconnect.rs   # backoff with jitter, auth-failure policy
│   ├── sources/
│   │   ├── mod.rs
│   │   ├── weather.rs     # weather entity + get_forecasts
│   │   ├── calendar.rs    # calendar.get_events
│   │   ├── media.rs       # media_player view model, entity_picture
│   │   └── departures.rs  # generic departures attribute parser
│   ├── extensions/        # Wave 6 only
│   │   ├── mod.rs
│   │   ├── catalog.rs     # index fetch + cache
│   │   ├── manifest.rs    # manifest/config schemas
│   │   ├── signature.rs   # Minisign verification
│   │   ├── store.rs       # atomic install/update/remove
│   │   ├── host.rs        # Rhai sandbox + permissions
│   │   └── install.rs     # package download/extraction
│   └── ui/
│       ├── mod.rs
│       ├── bridge.rs      # AppState -> Slint properties/models
│       └── images.rs      # background fetch/decode, SharedPixelBuffer
├── ui/
│   ├── main.slint
│   ├── theme.slint
│   ├── pages/
│   │   ├── overview.slint
│   │   ├── media.slint
│   │   ├── agenda.slint
│   │   ├── shop.slint     # Wave 6 only
│   │   └── status.slint
│   └── widgets/
│       ├── clock.slint
│       ├── weather.slint
│       ├── ha_tile.slint
│       ├── media_player.slint
│       ├── departures.slint
│       └── calendar.slint
├── deploy/
│   ├── ha-kiosk.service
│   ├── install.sh
│   ├── uninstall.sh
│   └── kiosk-setup.sh
├── scripts/
│   ├── build-all.sh
│   └── run-dev.sh
└── tests/
    ├── config_test.rs
    ├── ha_client_test.rs
    ├── cache_test.rs
    ├── ui_smoke_test.rs
    └── extensions_test.rs # Wave 6 only
```

`build.rs` calls `slint_build::compile("ui/main.slint")`. Slint files are compiled at build time; there is no runtime `.slint` loading.

---

## 7. Interfaces between waves

These names and shapes are frozen in Wave 0. Later waves may add fields but must not silently rename or change the meaning of these items.

### 7.1 Dependency contract

Use these features. They were checked against the published crate metadata; do not guess feature names.

```toml
[dependencies]
slint = { version = "1.19", default-features = false, features = [
  "compat-1-18", "std", "renderer-software",
] }
tokio = { version = "1.54", features = [
  "rt-multi-thread", "macros", "sync", "time", "fs", "signal",
] }
tokio-tungstenite = { version = "0.30", features = ["rustls-tls-native-roots"] }
futures-util = "0.3"
reqwest = { version = "0.13", default-features = false, features = ["rustls", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1.2"
chrono = { version = "0.4", features = ["serde"] }
chrono-tz = "0.10"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt"] }
thiserror = "2"
anyhow = "1"
url = { version = "2.5", features = ["serde"] }
secrecy = "0.10"
image = { version = "0.25", default-features = false, features = ["png", "jpeg"] }

[build-dependencies]
slint-build = "1.19"

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
i-slint-backend-testing = "1.19"

[features]
default = ["desktop"]
desktop = ["slint/backend-winit"]
kiosk = [
  "slint/backend-linuxkms",
  "slint/backend-linuxkms-libinput",
  "slint/backend-linuxkms-libseat",
]
```

`Cargo.lock` is committed. Update patch versions with `cargo update` only as part of a reviewed change. Do not use `--all-features` on macOS; the LinuxKMS backend is Linux-only.

### 7.2 Extension interfaces (Wave 6)

Wave 6 adds these dependencies; they are not part of the v1 build:

```toml
rhai = { version = "1.27", features = ["serde"] }
minisign-verify = "0.3"
sha2 = "0.11"
hex = "0.4"
tar = "0.4"
flate2 = "1.1"          # default rust_backend; no C zlib
semver = "1.0"
tempfile = "3"          # promoted from dev-dependencies
# Also enable the existing reqwest "blocking" feature for the synchronous Rhai host,
# or bridge through a blocking runtime handle. Do not call block_on on the UI thread.
```

```rust
pub struct ExtensionManifest {
    pub id: String,                    // reverse-DNS, immutable
    pub name: String,
    pub version: semver::Version,
    pub api_version: u32,              // must be 1
    pub min_app_version: semver::Version,
    pub entry: String,                 // "main.rhai"
    pub provides: ExtensionKind,
    pub permissions: Permissions,
    pub config_schema: Vec<ConfigField>,
    pub author: String,
    pub license: String,
    pub homepage: Option<url::Url>,
    pub description: String,
}

pub enum ExtensionKind {
    Departures,
    List,
    KeyValue,
}

pub struct Permissions {
    pub http_hosts: Vec<String>,       // exact host[:port], HTTPS only
    pub ha_entities: Vec<EntityId>,    // read-only
}

pub struct CatalogIndex {
    pub schema_version: u32,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub extensions: Vec<CatalogEntry>,
}

pub struct CatalogEntry {
    pub manifest: ExtensionManifest,
    pub package_url: url::Url,
    pub sha256: String,
    pub size_bytes: u64,
}

pub struct ExtensionHost {
    // catalog, verified index, installed store, script runner
}
impl ExtensionHost {
    pub async fn refresh_catalog(&mut self) -> Result<(), ExtensionError>;
    pub async fn install(&mut self, id: &str, version: &semver::Version) -> Result<(), ExtensionError>;
    pub async fn update(&mut self, id: &str) -> Result<(), ExtensionError>;
    pub async fn remove(&mut self, id: &str, purge_settings: bool) -> Result<(), ExtensionError>;
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), ExtensionError>;
    pub async fn refresh_due(&mut self, ha: &HaSnapshot) -> Vec<ExtensionResult>;
}
```

Script output contract:

```json
{"kind": "departures", "title": "Bus", "items": [
  {"line": "42", "destination": "Central Station", "departure": "2026-11-22T21:55:00+01:00",
   "delay_minutes": 2, "platform": "2", "cancelled": false, "remarks": null}
]}
```

`list` uses `columns` and `rows`; `key_value` uses `items` of `{label, value}`. The host validates the shape and limits items to 100.


```rust
// config.rs
pub struct Config {
    pub ha: HaConfig,
    pub display: DisplayConfig,
    pub pages: PagesConfig,
    pub widgets: WidgetsConfig,
    pub tiles: TilesConfig,
    pub cache: CacheConfig,
}

pub struct HaConfig {
    pub url: url::Url,
    pub token: SecretString,              // redacted Debug
    pub allow_invalid_certs: bool,        // default false
    pub extra_entity_ids: Vec<EntityId>,  // additional subscriptions
    pub time_zone: Option<chrono_tz::Tz>, // fallback if get_config omits it
}

impl Config {
    /// Widget and tile entities plus `ha.extra_entity_ids`, deduplicated.
    pub fn subscribed_entity_ids(&self) -> Vec<EntityId>;
}

// events.rs
pub type EntityId = String;

pub enum HaCommand {
    CallService(ServiceCall),
    RefreshSnapshot,
    Shutdown,
}

pub enum HaEvent {
    Connected { ha_version: String, time_zone: chrono_tz::Tz },
    AuthFailed { message: String },
    Disconnected { reason: String },
    Snapshot(Vec<HaEntity>),
    EntitiesAdded(Vec<HaEntity>),
    EntitiesChanged(Vec<EntityChange>),
    EntitiesRemoved(Vec<EntityId>),
    ServiceResult { request_id: u64, result: Result<ServiceResponse, HaError> },
}

pub struct ServiceCall {
    pub request_id: u64,
    pub domain: String,
    pub service: String,
    pub target: Option<serde_json::Value>,
    pub data: serde_json::Value,
    pub return_response: bool,
}

pub enum PendingAction {
    ToggleTile { entity_id: EntityId },
    MediaTransport { entity_id: EntityId },
    MediaVolume { entity_id: EntityId },
}

pub enum AppEvent {
    Ha(HaEvent),
    Weather(Result<WeatherView, SourceError>),
    Media(Option<MediaView>),
    Departures(Result<DeparturesView, SourceError>),
    Calendar(Result<Vec<CalendarItem>, SourceError>),
    AlbumArt { entity_id: EntityId, image: slint::SharedPixelBuffer<slint::Rgba8Pixel> },
    ImageFailed { entity_id: EntityId, reason: String },
}

// state.rs
pub struct AppState {
    pub connection: ConnectionState,
    pub ha_version: Option<String>,
    pub time_zone: chrono_tz::Tz,
    pub entities: std::collections::HashMap<EntityId, HaEntity>,
    pub weather: Option<WeatherView>,
    pub media: Option<MediaView>,
    pub departures: Option<DeparturesView>,
    pub calendar: Vec<CalendarItem>,
    pub stale: bool,
    pub last_success: Option<chrono::DateTime<chrono::Utc>>,
    pub pending: std::collections::HashMap<u64, PendingAction>,
}

pub trait CacheStore {
    fn load(&self) -> Result<Option<AppState>, CacheError>;
    fn save(&self, state: &AppState) -> Result<(), CacheError>;
}

// app.rs
pub struct AppController {
    // owns AppState, the generated Slint window, and the HA command sender
}
impl AppController {
    pub fn new(
        window: AppWindow,
        config: Config,
        commands: tokio::sync::mpsc::Sender<HaCommand>,
    ) -> Self;
    pub fn apply(&mut self, event: AppEvent);
    pub fn next_page(&mut self);
    pub fn previous_page(&mut self);
}
```

Slint callbacks are wired in `ui/bridge.rs`; background tasks never touch a Slint handle directly. Images cross threads only as `SharedPixelBuffer`, never as `slint::Image`, because `Image` is not `Send`.

---

## 8. Waves

Each wave is an **epic**. Every checkbox below is a separate GitHub issue, including the final `Commit:` step; the generated mapping is [`docs/issues/index.md`](docs/issues/index.md). A wave is accepted only when its gate is demonstrated with commands or a written manual test record.

### Wave 0 — Scaffold, interfaces, and cross-compile spike

**Dependencies:** none. **Agents:** 1, sequential.

**Tasks**

- [ ] Create the Cargo package `ha-kiosk` at the repository root with `src/lib.rs` and `src/main.rs`.
- [ ] Add dependencies and pin Slint 1.19; set `rust-version = "1.95"` and edition 2024.
- [ ] Add `build.rs` with `slint_build::compile("ui/main.slint")` and a minimal window that compiles.
- [ ] Define the interfaces from section 7 as compiling stubs; add `#![deny(unsafe_code)]` unless a reviewed exception is needed.
- [ ] Add feature flags `desktop` (winit + software renderer) and `kiosk` (LinuxKMS + software renderer); verify both compile on the host where possible.
- [ ] Add `Cross.toml` using Slint cross images; pin image digests after verification. Add `pre-build` sysroot packages for fontconfig, libinput, libudev, libxkbcommon, and libseat. Document that `cross` requires Docker on macOS.
- [ ] Prove the highest-risk path: `cross build --release --target aarch64-unknown-linux-gnu --no-default-features --features kiosk` produces a binary. If it fails, document the exact error and choose the fallback (native build on the Pi) before Wave 1.
- [ ] Add `config.example.toml`, `.gitignore`, `scripts/run-dev.sh`, and a CI workflow skeleton that will pass once `Cargo.toml` exists.
- [ ] Write `AGENTS.md` with the language, GitHub identity, layout, and build rules.
- [ ] Commit: `chore: scaffold ha-kiosk and prove cross-build`.

**Gate:** `cargo build` and `cargo test` pass on macOS/Linux; the cross spike either produces an aarch64 binary or has a documented, accepted fallback; no UI or feature logic yet.

**Notes:** Cross-compilation is the largest schedule risk. Do it before writing features, not in Wave 4 for the first time. `cargo-zigbuild` is not a drop-in replacement because Slint needs native libraries such as fontconfig and xcb; prefer `cross` or a native build.

---

### Wave 1 — Core

**Dependencies:** Wave 0. **Agents:** 3, parallel after the interfaces freeze.

#### 1.1 Configuration

- [ ] `Config::load(path) -> Result<Config, ConfigError>` with `deny_unknown_fields`.
- [ ] Token resolution order: `HA_TOKEN` env, `token_file`, literal `token` (development only).
- [ ] Validate URL scheme, display dimensions/rotation, refresh intervals, entity ID syntax, file permissions, and a non-empty resolved subscription list.
- [ ] Redact the token in `Debug`; unit-test that no error or debug output contains it.
- [ ] Tests: valid config, missing required field, unknown key, bad URL, token file with mode `0644` rejected, token file with `0600` accepted.
- [ ] Commit: `feat: typed TOML configuration`.

#### 1.2 Home Assistant WebSocket client

- [ ] Implement connect, `auth_required` → `auth` → `auth_ok`/`auth_invalid`; derive the WebSocket URL from `ha.url` without dropping a path prefix; honor `allow_invalid_certs` for `wss`.
- [ ] Call `get_config` after authentication; emit `ha_version` and the IANA `time_zone`.
- [ ] Implement `subscribe_entities` with the configured entity IDs and decode `a`/`c`/`r` messages, including removed attributes.
- [ ] Implement `get_states` for diagnostics and tests; do not add a global `subscribe_events("state_changed")` subscription in v1.
- [ ] Implement `call_service` with `target`, `service_data`, optional `return_response`, and a UI-generated `request_id`.
- [ ] Implement a 30-second ping/pong heartbeat and a read loop that never blocks the command channel.
- [ ] Implement reconnect with exponential backoff and jitter, capped at 60 seconds; `auth_invalid` stops retries.
- [ ] Emit `HaEvent` values; never log the token, full credential URL, or image query strings.
- [ ] Tests with a mock WebSocket server: auth success/failure, `get_config` timezone, path-prefixed URL, snapshot, add/change/remove, service response correlation, disconnect, reconnect, heartbeat timeout.
- [ ] Commit: `feat: Home Assistant websocket client with reconnect`.

#### 1.3 State and cache

- [ ] Define `AppState`, connection state, stale flag, pending actions, Home Assistant version, timezone, and view models.
- [ ] Implement a versioned JSON cache with atomic write: write a temporary file in the same directory, `fsync` the file, rename, then `fsync` the directory.
- [ ] Store a configuration fingerprint (widget/tile/entity IDs and refresh settings) in the cache header; discard a cache whose schema or fingerprint does not match.
- [ ] Debounce writes (default 10 s) to limit SD-card wear; cache only the configured views and entity set; cap the cache size.
- [ ] Enforce cache directory `0700` and file `0600`.
- [ ] Load the cache before the first frame; reject incompatible schema versions with a warning, not a crash.
- [ ] Tests: round-trip, missing file, corrupt JSON, schema mismatch, fingerprint mismatch, atomic rename on failure, permissions, cache size cap.
- [ ] Commit: `feat: application state and offline cache`.

**Wave 1 gate:** `cargo test` is green; a mock Home Assistant server can drive a snapshot and a state change into `AppState`; no token appears in logs.

---

### Wave 2 — UI shell

**Dependencies:** Wave 1. **Agents:** 3, parallel.

#### 2.1 Slint shell and navigation

- [ ] `ui/main.slint`: window, page host, tab bar for Overview/Media/Agenda/Status.
- [ ] Horizontal swipe with axis lock and a threshold (at least 20% of width or 60 logical pixels); tab bar remains the reliable fallback.
- [ ] Page swipe starts only from non-interactive background/edge zones, never from a slider, scrollable list, button, or tile.
- [ ] Long lists use vertical `Flickable` (or equivalent); page swipe and list scrolling do not conflict.
- [ ] `AppController` applies `AppEvent` values and updates Slint models/properties on the UI thread.
- [ ] Coalesce state updates and cap redraws at four per second; the clock is a separate one-second timer.
- [ ] Set fullscreen through `slint::Window::set_fullscreen(true)`.
- [ ] Commit: `feat: Slint shell and page navigation`.

#### 2.2 Touch and focus states

- [ ] All interactive elements have a minimum 44 × 44 logical-pixel hit area.
- [ ] Pressed/active visual state; focus ring for keyboard/mouse development builds.
- [ ] Tap detection for tiles and long-press reserved for future use (do not ship an undocumented long-press action).
- [ ] Test with `i-slint-backend-testing`: tap changes page; swipe threshold; gesture from a slider does not change page; vertical scroll does not change page; a too-small target fails the layout check.
- [ ] Verify 800 × 480 and 1024 × 600 layouts; long text elides or wraps without pushing controls off-screen.
- [ ] Commit: `feat: touch targets and interaction states`.

#### 2.3 Theme tokens

- [ ] `ui/theme.slint` with color, spacing, radius, and type-scale tokens.
- [ ] Dark theme first; light theme structure prepared but not required to be polished.
- [ ] Bundled SVG icons for weather conditions; no runtime icon downloads.
- [ ] Verify contrast for text on background and on tiles.
- [ ] Commit: `feat: placeholder theme tokens`.

**Wave 2 gate:** The desktop build starts, shows four empty pages, swipe and tab navigation work, and the UI testing backend passes the shell tests.

**Notes:** Slint 1.19 has no `Window::set_cursor_visible` API. Cursor hiding is deployment-specific: `unclutter` on X11, compositor configuration on Wayland, and a touch-only LinuxKMS path where no pointer is needed. Verify the panel behaviour on the target device. Do not put a fake cursor API in `kiosk.rs`.

---

### Wave 3 — Feature widgets

**Dependencies:** Waves 1 and 2. **Agents:** 5, parallel, one widget owner each; task 3.6 runs after the widgets land.

#### 3.1 Clock and weather

- [ ] Clock and date from local time; one-second timer; 12/24-hour setting.
- [ ] Weather from a configured `weather.*` entity: condition, current temperature, unit, and attributes that exist.
- [ ] Forecast via `weather.get_forecasts` with `return_response: true`; configurable `hourly`/`daily` and count.
- [ ] Map Home Assistant condition strings to bundled icons; unknown condition gets a neutral icon.
- [ ] Refresh forecast every 15 minutes by default; retry with backoff; keep the last forecast on failure.
- [ ] Unit tests with recorded JSON fixtures; no live network in tests.
- [ ] Commit: `feat: clock and weather widgets`.

#### 3.2 Home Assistant tiles

- [ ] Configurable grid with `columns` and explicit tile entries (entity, label, optional icon).
- [ ] Bind state and supported attributes; show unavailable/unknown states honestly.
- [ ] Implement the action table from section 4.3; unsupported domains are read-only.
- [ ] Show a pending state during a service call and an error if it fails; ignore repeated taps while pending.
- [ ] Tests: mapping for each supported domain; unknown domain stays read-only; failed call surfaces an error; rapid taps produce at most one in-flight call.
- [ ] Commit: `feat: Home Assistant entity tiles`.

#### 3.3 Media player

- [ ] Parse `media_player` state, title, artist, album, `entity_picture`, volume, and supported features.
- [ ] Play/pause, previous/next, and volume slider; disable controls the entity does not support; debounce volume changes (at most one call per 100 ms, final value on release).
- [ ] Fetch album art over HTTP(S) only when needed; resolve relative URLs against the HA base URL; send the token only to the Home Assistant origin and never forward it across an origin redirect; honor `allow_invalid_certs`; decode off the UI thread into `SharedPixelBuffer`; reject images above a documented byte/pixel limit; cache by URL with a memory cap.
- [ ] Log only the image host and path, never the query string.
- [ ] Tests: parser fixtures, disabled controls, failed image fetch keeps the previous art.
- [ ] Commit: `feat: media player widget`.

#### 3.4 Departures

- [ ] Implement the provider-neutral contract in `docs/departures-entity-contract.md`: a configured sensor entity with a `departures` attribute.
- [ ] Parse line, destination, departure time, optional delay, platform, and cancelled flag; sort by departure time; drop past entries. Interpret a timestamp without an offset in the Home Assistant timezone.
- [ ] Refresh every 60 seconds by default, and immediately on reconnect; ignore stacked refreshes while one is in flight.
- [ ] Show “No departures” and “Data unavailable” as distinct states.
- [ ] Tests with fixtures, including malformed entries, timezone offsets, and a cancelled service.
- [ ] Commit: `feat: generic departures widget`.

**No provider-specific code, URLs, or stop IDs.** If a user’s integration exposes a different shape, they create a template sensor that follows the documented contract; the app does not grow a parser per provider.

#### 3.5 Calendar

- [ ] Call `calendar.get_events` for each configured calendar with a start/end range and `return_response: true`.
- [ ] Interpret timestamps without an offset in the Home Assistant timezone from `get_config`; convert to the device timezone for display.
- [ ] Merge events from multiple calendars, sort by start, and render today/tomorrow sections.
- [ ] Handle all-day events, overlapping events, and calendars that return errors independently.
- [ ] Refresh every 5 minutes and on reconnect.
- [ ] Tests with fixtures for multiple calendars, all-day events, and one failing calendar.
- [ ] Commit: `feat: calendar agenda widget`.

#### 3.6 Adopted design port (Overview)

The design system is adopted. This task ports the reference prototype to Slint and extends the light tile with the hero controls.

- [ ] Port `DESIGN.md` and `tokens.css` to `ui/theme.slint`; no hardcoded colours, spacing, or type sizes in components.
- [ ] Build the Overview top bar and hero light card: toggle, brightness, colour temperature, using `light.turn_on` with `brightness_pct`/`color_temp_kelvin` and debounced sliders.
- [ ] Implement scenes/quick actions and a configurable rooms/tile group from Home Assistant entities; never hardcode room, scene, or light names.
- [ ] Add compact layouts for 800 × 480 and 1024 × 600 plus the three-column 1280+ layout; no page-level horizontal scroll; secondary content uses paging or one contained scroll region.
- [ ] Fix the known design defects: 44 px switch hit area, AA contrast tokens, Inter bundle/fallback, and a software-renderer-friendly glow without CSS blur.
- [ ] Verify the five data states (Loading, Empty, Error, Populated, Edge) and capture screenshots at 800 × 480, 1024 × 600, 1280, 1440, and 2048.
- [ ] Keep Energy and Schedule post-v1; do not ship the prototype's mock metrics as production data.
- [ ] Commit: `feat: port adopted design to Slint Overview`.

**Wave 3 gate:** All widgets render with fixtures in the desktop build; each widget has unit tests; no widget requires a live network or a specific provider. The Overview matches the adopted tokens and passes the 800 × 480, 1024 × 600, and 1280+ layout checks.

---

### Wave 4 — Kiosk and deployment

**Dependencies:** Wave 3. **Agents:** 3, parallel.

#### 4.1 Release builds and cross-compilation

- [ ] Finalize `Cross.toml` with pinned image digests and required `pre-build` packages.
- [ ] `scripts/build-all.sh` builds `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`, and `x86_64-unknown-linux-gnu` with `--no-default-features --features kiosk`.
- [ ] Name artifacts with version and target; generate SHA-256 checksums; place them in `dist/`.
- [ ] Document the glibc 2.35+ requirement and the Bookworm-only support for cross-built binaries.
- [ ] Document the native-build fallback on the Pi.
- [ ] Commit: `chore: reproducible release builds`.

#### 4.2 Kiosk mode

- [ ] `src/kiosk.rs`: fullscreen, display rotation, platform detection, and clear errors when no display backend is available.
- [ ] LinuxKMS path: `SLINT_BACKEND=linuxkms-software`, `SLINT_KMS_ROTATION`, input through libinput/libseat. Document `SLINT_DRM_OUTPUT` and `SLINT_DRM_MODE` for multi-display or explicit resolution selection.
- [ ] X11 fallback: winit build, a minimal window manager (for example openbox or matchbox), cursor hiding with `unclutter` or equivalent, blanking off (`xset s off -dpms`). Without a window manager, fullscreen may not cover the screen.
- [ ] Install a font (`fonts-dejavu-core`) or bundle a licensed font; include the icon set’s license and attribution in `README.md`.
- [ ] Ensure time synchronization (`systemd-timesyncd` or chrony) and the device timezone are set; a Pi without an RTC otherwise shows the wrong clock after boot.
- [ ] Touch calibration and rotation notes for common 7-inch panels; do not hardcode a calibration matrix.
- [ ] Disable console blanking for the LinuxKMS path (`consoleblank=0` in the kernel command line) and document it.
- [ ] Commit: `feat: kiosk display setup`.

#### 4.3 systemd and install

- [ ] `deploy/ha-kiosk.service`: restart on failure, start after the seat/graphical target or after `seatd`, run as the `ha-kiosk` user, never as root. Use `RestartSec=5` and a sane `StartLimitBurst`/`StartLimitIntervalSec`. Set `Environment=RUST_LOG=info`, `Environment=SLINT_BACKEND=linuxkms-software`, and `Environment=LIBSEAT_BACKEND=seatd` on the default KMS path. Add baseline hardening (`NoNewPrivileges=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/ha-kiosk`); if seatd/logind socket access is blocked, relax to `ProtectSystem=full` and document why.
- [ ] Default seat model: **seatd + system service** with the service user in the `seat` group. Document the alternatives (systemd user service inside an autologin session, or a system service with `PAMName=login` and a TTY) and test the chosen default.
- [ ] `deploy/kiosk-setup.sh`: install/enable seatd, console blanking off, rotation, and a reboot notice for the chosen model.
- [ ] Grant only the required groups (`video`, `input`, `render` as needed); prefer libseat/seatd over root.
- [ ] `deploy/install.sh`: create user, install binary, create `/var/lib/ha-kiosk` (`0700`, owned by the service user), install config with `0600`, install the runtime packages (`libseat1`, `libinput10`, `libudev1`, `libxkbcommon0`, `libfontconfig1`, `fonts-dejavu-core`, `ca-certificates`, `seatd` on the KMS path), install unit, enable and start.
- [ ] `deploy/uninstall.sh`: stop, disable, remove unit and binary; keep config unless `--purge` is passed.
- [ ] README: install, upgrade, logs (`journalctl -u ha-kiosk`), display backends, and rollback.
- [ ] Commit: `chore: systemd service and installer`.

**Wave 4 gate:** A release binary runs on a Pi Zero 2 W and a Pi 3 with Bookworm; the service starts after reboot; the display has no cursor and does not blank; an intentionally stopped Home Assistant produces the offline badge; logs contain no token.

**Hardware note:** If a physical Pi is not available, mark the gate “not verified on hardware” in the issue and record which parts were tested on x86_64 only. Do not claim hardware verification without a test record.

---

### Wave 5 — Integration, performance, and release

**Dependencies:** Wave 4. **Agents:** 3, parallel.

#### 5.1 End-to-end tests

- [ ] Mock Home Assistant server covering config → connect → snapshot → UI state.
- [ ] Offline scenario: server stops, cached data appears with a stale badge; server returns, state resyncs.
- [ ] Authentication failure: invalid token shows an error and does not loop.
- [ ] Service failure: tile shows an error and does not fake success.
- [ ] Malformed messages and unknown domains do not crash the app.
- [ ] Add `cargo audit` (and optionally `cargo deny check licenses`) to CI; review findings before release.
- [ ] Commit: `test: end-to-end and failure scenarios`.

#### 5.2 Performance and stability

- [ ] Measure RSS, first-frame time, and idle CPU on the Pi Zero 2 W; record the method and numbers.
- [ ] Use `/usr/bin/time -v` or systemd memory accounting on the Pi; use `heaptrack` on x86_64 for allocation hotspots.
- [ ] Verify RSS < 150 MB (target < 100 MB), first frame < 3 s, idle CPU low enough that the device does not throttle.
- [ ] Run a 24-hour soak with a simulated state-change stream; check for leaks and unbounded caches.
- [ ] Tune release profile (`opt-level = "s"`, LTO, `strip`, `panic = "abort"`) and confirm behavior.
- [ ] Commit: `perf: verify Pi Zero 2 W budgets`.

#### 5.3 Documentation and release

- [ ] README: features, supported hardware, install, config reference, troubleshooting, security, license status.
- [ ] Commented `config.example.toml`; no real credentials or personal identifiers.
- [ ] Troubleshooting: cross-compile errors, glibc mismatch, no display backend, touch rotation, token permissions, Home Assistant version.
- [ ] Release checklist: tests, clippy, fmt, cross artifacts, checksums, changelog, known limitations.
- [ ] Commit: `docs: complete operator documentation`.

**Wave 5 gate:** All automated checks pass; the performance numbers are recorded; the 24-hour soak has no leak trend; documentation matches the shipped behavior; the license decision is recorded before any binary is distributed.

---

### Wave 6 — Community extension shop (post-v1)

**Dependencies:** Waves 1–3 for the HA client, UI shell, and widget rendering; start after Wave 5. **Agents:** 4, parallel after the extension interface is frozen. **This wave does not change the v1 release gate.**

#### 6.1 Catalog, signature, and store

- [ ] Define the signed catalog format (`index.json` + `index.json.minisig`) and JSON schemas for catalog entries and manifests.
- [ ] Fetch the catalog over HTTPS, verify Minisign signatures against configured public keys, cache the last verified index, and enforce a catalog size cap.
- [ ] Enforce HTTPS, size caps, SHA-256 checksums, and `min_app_version`/`api_version` compatibility. Unsigned catalogs and HTTP URLs are accepted only when `allow_unsigned` and `dev_mode` are both explicitly enabled.
- [ ] Download `.hkext` packages (gzipped tar) to a temp directory; verify size and checksum before extraction.
- [ ] Reject absolute paths, `..`, symlinks, hardlinks, device files, and decompressed-size bombs during extraction.
- [ ] Install atomically under `/var/lib/ha-kiosk/extensions/<id>/<version>/`; keep one previous version for rollback.
- [ ] Maintain `installed.json` (id, version, enabled, manifest hash, source catalog) and enforce `0600`/`0700` permissions.
- [ ] Tests: valid/invalid/tampered signature, key rotation, checksum mismatch, oversized package, path traversal, install/update/rollback/remove, offline cache.
- [ ] Commit: `feat: signed extension catalog and store`.

#### 6.2 Sandboxed extension host

- [ ] Add a Rhai engine with no filesystem, process, environment, or module access.
- [ ] Register only `http_get_json`, `ha_state`, `now`, and `log`; enforce the manifest's host and entity allowlists.
- [ ] `http_get_json`: GET only, HTTPS, 10 s timeout, 1 MB cap, no credentials, no cross-host redirects.
- [ ] `ha_state`: read-only snapshot, allowlisted entity IDs, no service calls, no HA token exposure.
- [ ] Set operation, string, array, map, call-depth, and wall-clock limits; run the script off the UI thread.
- [ ] Validate the `departures`, `list`, or `key_value` output shape; cap at 100 items; reject malformed output.
- [ ] Keep last good data on failure; mark repeated failures degraded; show the error in the shop/status page.
- [ ] Tests: forbidden host/entity, redirect escape, HTTP rejection, infinite loop/operation limit, oversized allocation, malformed output, timeout.
- [ ] Commit: `feat: sandboxed extension host`.

#### 6.3 Shop UI and configuration

- [ ] Add a Shop page (fifth page, only when `extensions.enabled = true`) with installed/available lists.
- [ ] Show name, version, author, license, description, permissions, configuration status, and errors.
- [ ] Implement install, update, remove, enable/disable with a first-install trust confirmation.
- [ ] Show configuration keys and the exact TOML/secret file paths; no on-screen keyboard in Wave 6.
- [ ] Add extension errors to the Status page; keep the page usable while the catalog is offline.
- [ ] Tests with the Slint testing backend: install/update/remove flows, permission display, offline state, error state.
- [ ] Commit: `feat: extension shop page`.

#### 6.4 Companion catalog repository and documentation

- [ ] Create the separate `home-assistant-kiosk-extensions` repository (visibility decided in section 12) with `index.json`, `index.json.minisig`, schemas, and extension sources.
- [ ] Add a validated example extension, `org.example.bus-departures`, using a configurable stop ID and placeholder API host; no real provider URL or personal ID.
- [ ] Document the author API, manifest, permissions, config schema, output schema, packaging, versioning, and security rules.
- [ ] Document the release/signing flow: CI validates; a maintainer signs the index offline with Minisign; the private key never enters CI or either repository.
- [ ] Document community review: license, permissions, no secrets, no service calls, no arbitrary UI, tests, removal process.
- [ ] Commit: `docs: extension authoring and catalog guide`.

**Wave 6 gate:** A signed catalog installs, updates, rolls back, and removes the example bus-departures extension; a tampered index/package and a sandbox violation are rejected; the extension works offline after install; RSS stays within the Pi budget; the shop UI is usable at 800 × 480; the catalog repository and signing documentation are complete.

**Security note:** Community extensions are untrusted third-party code. The signed catalog establishes provenance, not trustworthiness. The shop must show a clear warning and permissions before installation. Any extension that needs native code, arbitrary UI, service calls, or filesystem access is out of scope and must be rejected.

---

## 9. Verification matrix

| Requirement | Verified by | Wave |
| --- | --- | --- |
| Config parsing, validation, token redaction | `tests/config_test.rs` | 1 |
| HA auth, get_config timezone, path-prefixed URL, subscribe_entities, commands, reconnect | `tests/ha_client_test.rs` with mock WS | 1 |
| Cache round-trip, corruption, fingerprint mismatch, atomic write, permissions | `tests/cache_test.rs` | 1 |
| Page navigation, swipe threshold, gesture arbitration, touch sizes, responsive layouts | `tests/ui_smoke_test.rs`, testing backend | 2 |
| Weather and forecast parsing, timezone conversion | fixture unit tests | 3 |
| Tile action mapping, pending state, rapid taps | fixture/unit tests | 3 |
| Media parser, disabled controls, volume debounce, same-origin token | fixture/unit tests | 3 |
| Departures contract, timezone, malformed data | fixture/unit tests | 3 |
| Calendar merge, timezone, all-day events, partial failures | fixture/unit tests | 3 |
| Adopted design tokens, hero controls, breakpoints, contrast | token test + screenshots at 800×480, 1024×600, 1280+ | 3 |
| Offline and reconnect behaviour | end-to-end mock server | 5 |
| Dependency vulnerabilities | `cargo audit` in CI | 5 |
| RSS, startup, idle CPU, 24 h soak | Pi test record | 5 |
| Cross-build artifacts | `scripts/build-all.sh`, CI | 4 |
| systemd install and reboot | Pi manual test record | 4 |
| Catalog signature, checksum, package extraction | extension store tests | 6 |
| Sandbox permissions, limits, output validation | extension host tests | 6 |
| Shop install/update/remove, offline, errors | UI tests + Pi test record | 6 |

---

## 10. Review focus

These are the failure modes most likely to hurt a real user and are not fully covered by ordinary unit tests:

1. **Entity subscription correctness.** Removed entities and removed attributes must disappear from the UI; a stale attribute must not survive a reconnect.
2. **Bad credentials and TLS failures.** A wrong token must stop retrying; a self-signed certificate must produce an actionable error, not a silent offline state.
3. **Pi Zero 2 W resource use.** Album art decoding, JSON parsing of large snapshots, and software rendering can push RSS or CPU over budget. Measure, do not assume.
4. **Kiosk display access.** LinuxKMS needs DRM/input permissions, a seat manager or group access, and correct rotation; X11 needs DISPLAY/XAUTHORITY and cursor/blanking settings. A service that starts but cannot open the display must fail visibly.
5. **Honest stale data.** Cached values must never look live. The badge and timestamp are part of the feature, not decoration.
6. **Secret handling.** Token in logs, token in a crash message, world-readable config, token sent to a third-party image host, or token in a screenshot are all release blockers.
7. **Timezone correctness.** A kiosk in a different timezone from Home Assistant must still show calendar and departure times correctly; naive timestamps use the `get_config` timezone.
8. **Extension trust boundary.** A signed catalog proves provenance, not safety. A community script must not reach a host or entity outside its declared permissions, must not call HA services, and must not exhaust memory or CPU on a Pi.
9. **Design fidelity vs Pi performance.** The adopted prototype is a browser reference. The Slint port must keep the hierarchy and tokens while dropping blur-heavy effects and fitting 800 × 480.

Each focus item has an owning task above; the end-to-end tests in Wave 5 pin items 1, 2, and 5, and the Pi test record pins items 3 and 4. Timezone and image-token tests live with the widget and WebSocket tasks; the extension tests live in Wave 6.

---

## 11. Known risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| LinuxKMS backend is marked experimental by Slint | Kiosk may not start on some panels | Keep the X11/winit fallback; test on the actual panel; pin Slint. |
| Cross-compiled glibc mismatch | Binary fails on Bullseye | Require Bookworm for cross-built binaries; document native builds. |
| Cross images change or disappear | Build breaks | Pin image digests; keep a native-build fallback. |
| Pi Zero 2 W CPU too slow for smooth swipes | Poor UX | No continuous animations; coalesce redraws; test with a 800×480 panel. |
| Web prototype does not fit the Pi display or software renderer | Design looks wrong or runs slowly | Compact 800×480 layout, no CSS blur/color-mix, simplified glow, screenshot checks at every breakpoint. |
| Home Assistant instance has hundreds of entities | Memory and traffic blow up | Subscribe only to configured entity IDs; never cache the full state set. |
| Home Assistant behind a path-prefixed reverse proxy | WebSocket URL built at the root fails | Derive `<prefix>/api/websocket` from `ha.url`; test a path-prefixed URL. |
| Kiosk timezone differs from Home Assistant | Calendar/departure times are wrong | Use `get_config.time_zone` for naive timestamps; convert for display. |
| SD-card wear from cache writes | Card failure | Debounce writes (10 s), write atomically, keep the cache small. |
| Self-signed TLS on LAN | Connection fails | Document `allow_invalid_certs` as a last resort; prefer installing the CA. |
| Slint licensing for embedded distribution | Legal exposure | Decide GPL-3.0-only or commercial before distributing binaries. |
| Malicious or compromised community extension | Data leak, HA control, crash, or memory exhaustion | Signed index, checksum, Rhai sandbox, host/entity allowlists, no service calls, resource limits, permissions shown before install. |
| Rhai script allocates or loops too much | OOM or UI stall on a Pi | Operation/string/array/map limits, wall-clock timeout, run off the UI thread, one script at a time, measure RSS. |
| Compromised catalog signing key | Malicious catalog accepted | Offline key, key rotation list, revocation note, review of index changes; no private key in CI. |
| Extension API drift | Installed extensions break after updates | `api_version` and `min_app_version`; keep v1 API stable; test the example extension in CI. |
| No physical Pi available for a gate | False “done” claim | Record manual tests; mark unverified items explicitly. |

---

## 12. Open decisions

1. **License.** Slint is GPL-3.0-only, royalty-free for non-embedded use, or commercial. A single-purpose kiosk device may be classified as an embedded system. Choose GPL-3.0-only for the project (source must be available when binaries are distributed) or obtain a commercial license before distributing proprietary binaries. Do not add a license file until this is decided.
2. **Direct provider APIs.** v1 is Home Assistant-only. Weather, transit, and calendar providers stay behind Home Assistant integrations. Revisit only if a concrete provider cannot be represented by the generic entity contracts.
3. **Departures contract.** v1 uses a sensor attribute contract. If that proves too rigid, add a second adapter for a configurable JSON endpoint in v2; do not hardcode a provider.
4. **Kiosk backend support.** LinuxKMS is primary and X11 is the fallback. Wayland is supported by winit but not a release gate.
5. **UI localization.** English in v1. Keep strings centralized; add a locale file only when there is a second language.
6. **Hardware verification.** The release gate requires a Pi Zero 2 W and a Pi 3. If they are unavailable, the release is marked “x86_64 verified only”.
7. **Catalog repository visibility.** Decided: create `ABOYrbx/home-assistant-kiosk-extensions` as a public repository from the start because community use requires a public catalog. `ABOYrbx` is the designated production-key custodian and will generate the Minisign private key on encrypted offline storage before the first real release; it never enters CI or either repository. No production key exists yet. The initial empty index uses an ephemeral bootstrap key whose secret key is destroyed after signing.
8. **Extension service access.** v1 extensions are read-only for HA entities and cannot call services. Revisit only with a separate permission and review model.
9. **Extension runtime.** Rhai is chosen for size and simplicity. Revisit WASM only if a real extension cannot be expressed safely; do not add both.

---

## 13. Future extensions (not v1)

These are deliberately outside the v1 gate. They are recorded so the architecture keeps a place for them, not as commitments.

- **Richer tile controls:** cover position, climate setpoint and mode, fan speed, and a light colour wheel beyond the v1 brightness/colour-temperature controls.
- **Energy and Schedule cards:** from the adopted prototype; needs energy/automation data sources and is not part of the v1 gate.
- **Protocol optimization:** use Home Assistant's `coalesce_messages` supported feature when available to reduce per-message overhead on a Pi; measure before enabling.
- **Design polish:** final typography, iconography, tile density, and dark/light polish after the Slint port of the adopted design.
- **Camera and image widgets:** a snapshot or stream tile, with a strict memory and network budget.
- **Auto page rotation:** cycle pages after a configurable idle interval; useful for a wall display.
- **Idle screen and burn-in protection:** dim or blank after inactivity, with a tap to wake; optional presence-based wake and scheduled night dimming via Home Assistant.
- **Local audio and announcements:** text-to-speech or notification sounds; needs an audio stack and is not part of the low-memory target.
- **Direct provider adapters:** delivered through the Wave 6 extension shop where possible; never hardcode personal URLs or IDs in the kiosk.
- **Extension UI:** arbitrary Slint UI from an extension remains out of scope; new presentation types need a core release.
- **Localization:** German or other languages via a locale file; English remains the source language.
- **Multiple dashboards/profiles:** per-room configuration without duplicating the binary.
- **Signed updates/OTA:** verified artifact download and rollback; not needed for a manually installed kiosk.
- **Accessibility:** screen-reader support through Slint’s accessibility feature; requires a larger dependency budget.

Each extension needs its own issue, a clear memory/CPU budget, and an update to this plan before implementation.

---

## 14. Execution

**Recommended:** subagent-driven development. Each task is small enough for a fresh implementer, and the interfaces are frozen in Wave 0. A reviewer checks each wave against its gate before the next wave starts.

Workflow:

1. Open the subtask issue in [`docs/issues/index.md`](docs/issues/index.md); the wave issue is its parent epic.
2. Create a branch named `wave-N/<short-description>`.
3. Implement the subtask, keeping commits small and conventional (`feat:`, `fix:`, `test:`, `docs:`, `chore:`).
4. Run the applicable gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `python3 scripts/check-docs.py`.
5. Open a pull request that closes the subtask issue, links the parent epic, and records the gate evidence.
6. Merge only after review. Close the epic only when every subtask and the wave gate are complete.
7. Do not push with any GitHub account other than `ABOYrbx`.

GitHub issues:

- [Wave 0 — Scaffold, interfaces, cross-compile spike](https://github.com/ABOYrbx/home-assistant-kiosk/issues/5)
- [Wave 1 — Core: config, Home Assistant client, state/cache](https://github.com/ABOYrbx/home-assistant-kiosk/issues/3)
- [Wave 2 — UI shell: navigation, touch, theme](https://github.com/ABOYrbx/home-assistant-kiosk/issues/2)
- [Wave 3 — Feature widgets: clock/weather, tiles, media, departures, calendar](https://github.com/ABOYrbx/home-assistant-kiosk/issues/6)
- [Wave 4 — Kiosk and deployment: release builds, display, systemd](https://github.com/ABOYrbx/home-assistant-kiosk/issues/1)
- [Wave 5 — Integration, performance, documentation](https://github.com/ABOYrbx/home-assistant-kiosk/issues/4)
- [Wave 6 — Community extension shop (post-v1)](https://github.com/ABOYrbx/home-assistant-kiosk/issues/7)

The full subtask-to-issue mapping is generated in [`docs/issues/index.md`](docs/issues/index.md); wave issues are epics, not the implementation units.

**Next step:** Review this plan, resolve the license decision, then start Wave 0.
