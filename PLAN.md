# Home Assistant Kiosk Dashboard — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans`. Track implementation with the checkbox syntax (`- [ ]`) and with the GitHub issues linked at the end of this document.

**Goal:** A small, native kiosk dashboard written in Rust and Slint. It runs on a Raspberry Pi 3, a Pi Zero 2 W, and ordinary Linux systems (x86_64/ARM), talks to Home Assistant over its WebSocket API, and shows clock, weather, Home Assistant tiles, media, departures, and calendar events. It is touch-first, works as a full-screen kiosk, and keeps the last known state when Home Assistant is offline.

**Architecture:** One native Rust binary with a Slint UI. The Slint event loop and the application state live on the main thread. A Tokio runtime runs in the background for the Home Assistant WebSocket connection, service calls, HTTP image fetches, and cache writes. Background tasks send typed events to the UI thread; the UI thread applies them to Slint models and properties. All data is Home Assistant-first and configuration-driven: no provider-specific URLs, stop IDs, entity IDs, or personal data are compiled into the program.

**Tech stack:** Rust 1.95+ (edition 2024), Slint 1.19 (software renderer; winit for desktop development, LinuxKMS for the kiosk), tokio 1.54, tokio-tungstenite 0.30, serde/serde_json, toml 1.2, reqwest 0.13 with rustls, chrono, tracing, thiserror/anyhow, secrecy.

**Spec:** This file is the source of truth. The fact-check and change log is in [`docs/plan-review.md`](docs/plan-review.md). The generic departures entity contract is in [`docs/departures-entity-contract.md`](docs/departures-entity-contract.md). Repository rules for agents are in [`AGENTS.md`](AGENTS.md).

**Status:** Planning complete; no application code yet. Revision 2, 2026-11-22.

---

## 1. Product definition

### In scope for v1

- Clock and date, updated once per second.
- Current weather and a short forecast from a Home Assistant `weather` entity.
- A configurable grid of Home Assistant tiles (switch, light, climate, cover, scene, script, and read-only sensors).
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
- Direct weather, transit, or calendar provider APIs. Providers stay behind Home Assistant integrations in v1.
- User accounts, multi-user profiles, or cloud services.
- Video playback, camera streams, or audio output on the kiosk itself.
- Localization. The UI is English in v1; all user-facing strings live in one place so a later translation is possible.

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

---

## 3. Global constraints

- **Memory:** Raspberry Pi Zero 2 W has 512 MB RAM. Hard gate: resident set size below 150 MB. Target: below 100 MB at idle with cached data.
- **Startup:** First frame within 3 seconds using cached data. Home Assistant connection happens asynchronously and must not block the first frame.
- **Rendering:** No continuous animations. The clock updates once per second. Swipe/scroll should stay responsive; do not promise 60 fps on a Pi Zero 2 W software renderer.
- **Slint version:** Pin `slint = "1.19"` and `slint-build = "1.19"`. Do not upgrade a minor version inside a wave without a separate review.
- **Rust version:** `rust-version = "1.95"` in `Cargo.toml`. Slint's current build documentation requires at least Rust 1.95.
- **Slint features:** `default-features = false`; base features `compat-1-18`, `std`, `renderer-software`. Desktop adds `backend-winit`; kiosk adds `backend-linuxkms`, `backend-linuxkms-libinput`, and `backend-linuxkms-libseat`. PNG/JPEG and SVG support come with `std`; `image-default-formats` is optional if more formats are needed. Do not build with `--all-features` on macOS: the LinuxKMS backend is Linux-only.
- **Home Assistant minimum:** 2023.8, because `subscribe_entities` and service calls with `return_response` are required. The UI must degrade gracefully if an action is not supported.
- **Security:** The token is never logged, never included in error messages, and never committed. The config file must be mode `0600` and owned by the service user. TLS certificate validation stays on by default.
- **No personal data:** Examples use placeholders such as `light.example`, `weather.home`, `calendar.family`, and `sensor.departures`. Do not add a real address, stop ID, token, internal hostname, or person’s name to the repository.
- **Timeouts:** Every network operation has a timeout. WebSocket reconnect uses exponential backoff with jitter, capped at 60 seconds.
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
│ SchedulerTask: clock, weather forecast, calendar, departures refresh        │
│ ImageTask: fetch and decode album art into SharedPixelBuffer                │
│ CacheTask: debounced atomic snapshot writes                                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

Rules:

- `AppState` is owned by the main thread. It is not wrapped in a mutex and is not shared with Tokio tasks.
- Background tasks send `AppEvent` values over `tokio::sync::mpsc`. The UI thread drains them and updates Slint from `slint::invoke_from_event_loop`.
- Bursts of Home Assistant state changes are coalesced before a redraw. Update at most four times per second; the clock is a separate one-second timer.
- Blocking work (file I/O, image decoding) never runs on the UI thread.
- A panic in a background task is logged and the task is restarted by its supervisor, except for invalid configuration or an invalid token, which stop the retry loop and show an error state.

### 4.2 Home Assistant connection

1. Connect to `ws(s)://<host>:8123/api/websocket`.
2. Wait for `auth_required`, send `auth` with the access token.
3. Require `auth_ok`. On `auth_invalid`, stop reconnecting and show “Authentication failed”; do not hammer the server with a bad token.
4. Subscribe with `subscribe_entities` and the explicit list of entity IDs needed by the configured widgets. Do **not** subscribe to every `state_changed` event; that traffic is unnecessary on a Pi.
5. Apply the initial snapshot from `subscribe_entities`, then apply `a` (added), `c` (changed), and `r` (removed) events. Handle removed attributes as well as changed values.
6. `get_states` remains available for diagnostics and tests. Do not implement a global `subscribe_events("state_changed")` fallback in v1: Home Assistant 2023.8 and newer support `subscribe_entities`, which is the declared minimum.
7. Send `ping` every 30 seconds and expect `pong`. A missed heartbeat or a closed socket triggers reconnect with exponential backoff and jitter (1 s, 2 s, 4 s, … 60 s).
8. After a reconnect, authenticate again, re-subscribe, and replace the local entity map with the fresh snapshot. Never assume subscriptions survive a disconnect.
9. Service calls use `call_service` with `target` and `service_data`. Calls that return data (weather forecast, calendar events) set `return_response: true`.

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
| `light` | `light.toggle` | Optional brightness later; v1 toggles only. |
| `scene`, `script` | `scene.turn_on`, `script.turn_on` | Momentary action; no state toggle. |
| `cover` | `cover.open_cover` / `cover.close_cover` | Choose by current state. |
| `climate` | Read-only in v1 | Temperature display; setpoint control is a later feature. |
| `sensor`, `binary_sensor`, `weather` | Read-only | No tap action. |
| Anything else | Read-only | Render state and attributes that fit; never guess an action. |

Unknown or unsupported domains must render as read-only tiles. A failed service call shows a short error on the tile and in the status page; it never leaves the UI in a fake “on” state.

Media controls use `media_player.media_play_pause`, `media_player.media_next_track`, `media_player.media_previous_track`, and `media_player.volume_set` with `volume_level` in the range 0.0–1.0. Check `supported_features` and disable controls the entity does not advertise.

Calendar calls use `calendar.get_events` with `start_date_time` and `end_date_time` (ISO 8601, Home Assistant local timezone) or a `duration`, and `return_response: true`. Weather calls use `weather.get_forecasts` with `type = "daily"` or `"hourly"`, and `return_response: true`.

### 4.4 Offline and stale data

- On startup, load the cache before opening the window; the first frame shows cached views.
- Every view carries `last_success: Option<DateTime<Utc>>` and a `stale: bool`.
- If Home Assistant is unreachable, keep the last values, set `stale = true`, and show an “Offline — last update HH:MM” badge.
- Do not show old values as if they were live. Clock and date are always local and always live.
- On reconnect, replace stale values with the fresh snapshot and clear the badge.
- The cache stores only the views needed by the configured widgets, not the full Home Assistant state database. Include a schema version and reject incompatible caches.

---

## 5. Configuration

The default path is `/etc/ha-kiosk/config.toml`. A `--config <path>` argument overrides it. The example file is [`config.example.toml`](config.example.toml).

Design rules:

- `ha.token_file` is the normal source for a service. For development, the `HA_TOKEN` environment variable overrides it. A literal `ha.token` is accepted for development but is discouraged.
- The token file must be readable only by the service user (`0600`). Startup fails with a clear error if permissions are too open.
- `ha.url` must be `http://` or `https://`; `ws://`/`wss://` are derived internally.
- Entity IDs are validated syntactically (`domain.object_id`) but not checked against the server at startup. Missing entities are reported per widget, not as a fatal error.
- At least one subscribed entity is required after resolving widget, tile, and extra IDs; an empty subscription list is a configuration error.
- Unknown keys are rejected (serde `deny_unknown_fields`) to catch typos.
- Durations are in seconds and have documented minimums.
- `display.rotation` accepts `0`, `90`, `180`, `270`. On LinuxKMS it maps to `SLINT_KMS_ROTATION`; touch calibration is a deployment concern, documented in the README.
- No default entity IDs are shipped. A widget whose required entity is absent is hidden or shows a configuration hint.

---

## 6. Repository layout

The repository root **is** the Cargo package. There is no nested project folder and no multi-crate workspace in v1. A single crate with `lib.rs` and `main.rs` keeps the cross-build simple and the interfaces testable.

```text
home-assistant-kiosk/
├── AGENTS.md
├── PLAN.md
├── README.md
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
│   └── issues/
│       ├── wave-0.md … wave-5.md
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
│   └── ui/
│       ├── mod.rs
│       ├── bridge.rs      # AppState -> Slint properties/models
│       ├── view_models.rs # UI-facing structs, no Slint types
│       └── images.rs      # background fetch/decode, SharedPixelBuffer
├── ui/
│   ├── main.slint
│   ├── theme.slint
│   ├── pages/
│   │   ├── overview.slint
│   │   ├── media.slint
│   │   ├── agenda.slint
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
    └── ui_smoke_test.rs
```

`build.rs` calls `slint_build::compile("ui/main.slint")`. Slint files are compiled at build time; there is no runtime `.slint` loading.

---

## 7. Interfaces between waves

These names and shapes are frozen in Wave 0. Later waves may add fields but must not silently rename or change the meaning of these items.

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
    Connected { ha_version: String },
    AuthFailed { message: String },
    Disconnected { reason: String },
    Snapshot(Vec<HaEntity>),
    EntitiesAdded(Vec<HaEntity>),
    EntitiesChanged(Vec<EntityChange>),
    EntitiesRemoved(Vec<EntityId>),
    ServiceResult { request_id: u64, result: Result<ServiceResponse, HaError> },
}

pub enum AppEvent {
    Ha(HaEvent),
    Tick(chrono::DateTime<chrono::Local>),
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
    pub entities: std::collections::HashMap<EntityId, HaEntity>,
    pub weather: Option<WeatherView>,
    pub media: Option<MediaView>,
    pub departures: Option<DeparturesView>,
    pub calendar: Vec<CalendarItem>,
    pub stale: bool,
    pub last_success: Option<chrono::DateTime<chrono::Utc>>,
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

Each wave has a GitHub issue with a checklist derived from this section. A wave is accepted only when its gate is demonstrated with commands or a written manual test record.

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

- [ ] Implement connect, `auth_required` → `auth` → `auth_ok`/`auth_invalid`.
- [ ] Implement `subscribe_entities` with the configured entity IDs and decode `a`/`c`/`r` messages, including removed attributes.
- [ ] Implement `get_states` for diagnostics and tests; do not add a global `subscribe_events("state_changed")` subscription in v1.
- [ ] Implement `call_service` with `target`, `service_data`, and optional `return_response`.
- [ ] Implement a 30-second ping/pong heartbeat and a read loop that never blocks the command channel.
- [ ] Implement reconnect with exponential backoff and jitter, capped at 60 seconds; `auth_invalid` stops retries.
- [ ] Emit `HaEvent` values; never log the token or full WebSocket URL with credentials.
- [ ] Tests with a mock WebSocket server: auth success/failure, snapshot, add/change/remove, service response, disconnect, reconnect, heartbeat timeout.
- [ ] Commit: `feat: Home Assistant websocket client with reconnect`.

#### 1.3 State and cache

- [ ] Define `AppState`, connection state, stale flag, and view models.
- [ ] Implement a versioned JSON cache with atomic write (temporary file in the same directory, then rename).
- [ ] Debounce writes (default 10 s) to limit SD-card wear; cap the cache to the configured views and entity set.
- [ ] Load the cache before the first frame; reject incompatible schema versions with a warning, not a crash.
- [ ] Tests: round-trip, missing file, corrupt JSON, schema mismatch, atomic rename on failure, cache size cap.
- [ ] Commit: `feat: application state and offline cache`.

**Wave 1 gate:** `cargo test` is green; a mock Home Assistant server can drive a snapshot and a state change into `AppState`; no token appears in logs.

---

### Wave 2 — UI shell

**Dependencies:** Wave 1. **Agents:** 3, parallel.

#### 2.1 Slint shell and navigation

- [ ] `ui/main.slint`: window, page host, tab bar for Overview/Media/Agenda/Status.
- [ ] Horizontal swipe with axis lock and a threshold (at least 20% of width or 60 logical pixels); tab bar remains the reliable fallback.
- [ ] `AppController` applies `AppEvent` values and updates Slint models/properties on the UI thread.
- [ ] Coalesce state updates and cap redraws at four per second; the clock is a separate one-second timer.
- [ ] Set fullscreen through `slint::Window::set_fullscreen(true)`.
- [ ] Commit: `feat: Slint shell and page navigation`.

#### 2.2 Touch and focus states

- [ ] All interactive elements have a minimum 44 × 44 logical-pixel hit area.
- [ ] Pressed/active visual state; focus ring for keyboard/mouse development builds.
- [ ] Tap detection for tiles and long-press reserved for future use (do not ship an undocumented long-press action).
- [ ] Test with `i-slint-backend-testing`: tap changes page; swipe threshold; a too-small target fails the layout check.
- [ ] Commit: `feat: touch targets and interaction states`.

#### 2.3 Theme tokens

- [ ] `ui/theme.slint` with color, spacing, radius, and type-scale tokens.
- [ ] Dark theme first; light theme structure prepared but not required to be polished.
- [ ] Bundled SVG icons for weather conditions; no runtime icon downloads.
- [ ] Verify contrast for text on background and on tiles.
- [ ] Commit: `feat: placeholder theme tokens`.

**Wave 2 gate:** The desktop build starts, shows four empty pages, swipe and tab navigation work, and the UI testing backend passes the shell tests.

**Notes:** Slint 1.19 has no `Window::set_cursor_visible` API. Cursor hiding is deployment-specific: `unclutter` on X11, compositor configuration on Wayland, and no cursor under LinuxKMS software rendering. Do not put a fake cursor API in `kiosk.rs`.

---

### Wave 3 — Feature widgets

**Dependencies:** Waves 1 and 2. **Agents:** 5, parallel, one widget owner each.

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
- [ ] Show a pending state during a service call and an error if it fails.
- [ ] Tests: mapping for each supported domain; unknown domain stays read-only; failed call surfaces an error.
- [ ] Commit: `feat: Home Assistant entity tiles`.

#### 3.3 Media player

- [ ] Parse `media_player` state, title, artist, album, `entity_picture`, volume, and supported features.
- [ ] Play/pause, previous/next, and volume slider; disable controls the entity does not support.
- [ ] Fetch album art over HTTP(S) with the token only when required; decode off the UI thread into `SharedPixelBuffer`; cache by URL and cap memory.
- [ ] Handle relative `entity_picture` URLs by resolving them against the Home Assistant base URL.
- [ ] Tests: parser fixtures, disabled controls, failed image fetch keeps the previous art.
- [ ] Commit: `feat: media player widget`.

#### 3.4 Departures

- [ ] Implement the provider-neutral contract in `docs/departures-entity-contract.md`: a configured sensor entity with a `departures` attribute.
- [ ] Parse line, destination, departure time, optional delay, platform, and cancelled flag; sort by departure time; drop past entries.
- [ ] Refresh every 60 seconds by default, and immediately on reconnect.
- [ ] Show “No departures” and “Data unavailable” as distinct states.
- [ ] Tests with fixtures, including malformed entries, timezone offsets, and a cancelled service.
- [ ] Commit: `feat: generic departures widget`.

**No provider-specific code, URLs, or stop IDs.** If a user’s integration exposes a different shape, they create a template sensor that follows the documented contract; the app does not grow a parser per provider.

#### 3.5 Calendar

- [ ] Call `calendar.get_events` for each configured calendar with a start/end range and `return_response: true`.
- [ ] Merge events from multiple calendars, sort by start, and render today/tomorrow sections.
- [ ] Handle all-day events, overlapping events, and calendars that return errors independently.
- [ ] Refresh every 5 minutes and on reconnect.
- [ ] Tests with fixtures for multiple calendars, all-day events, and one failing calendar.
- [ ] Commit: `feat: calendar agenda widget`.

**Wave 3 gate:** All widgets render with fixtures in the desktop build; each widget has unit tests; no widget requires a live network or a specific provider.

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
- [ ] LinuxKMS path: `SLINT_BACKEND=linuxkms-software`, `SLINT_KMS_ROTATION`, input through libinput/libseat.
- [ ] X11 fallback: winit build, `unclutter` or equivalent for the cursor, `xset s off -dpms` for blanking.
- [ ] Touch calibration and rotation notes for common 7-inch panels; do not hardcode a calibration matrix.
- [ ] Disable console blanking for the LinuxKMS path (`consoleblank=0` in the kernel command line) and document it.
- [ ] Commit: `feat: kiosk display setup`.

#### 4.3 systemd and install

- [ ] `deploy/ha-kiosk.service`: restart on failure, start after the seat/graphical target or after `seatd`, run as the `ha-kiosk` user, never as root. Add baseline hardening (`NoNewPrivileges=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/ha-kiosk`).
- [ ] Choose and document the seat model: (a) a systemd user service inside an autologin session, (b) a system service with `PAMName=login` and a TTY, or (c) `seatd` with the service user in the `seat` group. Test the chosen path.
- [ ] `deploy/kiosk-setup.sh`: console/autologin setup for the chosen model, console blanking off, rotation, and a reboot notice.
- [ ] Grant only the required groups (`video`, `input`, `render` as needed); prefer libseat/logind or seatd over root.
- [ ] `deploy/install.sh`: create user, install binary, install config with `0600`, install unit, enable and start.
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
- [ ] Malformed messages and unknown entity domains do not crash the app.
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

## 9. Verification matrix

| Requirement | Verified by | Wave |
| --- | --- | --- |
| Config parsing, validation, token redaction | `tests/config_test.rs` | 1 |
| HA auth, subscribe_entities, commands, reconnect | `tests/ha_client_test.rs` with mock WS | 1 |
| Cache round-trip, corruption, atomic write | `tests/cache_test.rs` | 1 |
| Page navigation, swipe threshold, touch sizes | `tests/ui_smoke_test.rs`, testing backend | 2 |
| Weather and forecast parsing | fixture unit tests | 3 |
| Tile action mapping and errors | fixture/unit tests | 3 |
| Media parser and disabled controls | fixture/unit tests | 3 |
| Departures contract and malformed data | fixture/unit tests | 3 |
| Calendar merge and partial failures | fixture/unit tests | 3 |
| Offline and reconnect behaviour | end-to-end mock server | 5 |
| RSS, startup, idle CPU, 24 h soak | Pi test record | 5 |
| Cross-build artifacts | `scripts/build-all.sh`, CI | 4 |
| systemd install and reboot | Pi manual test record | 4 |

---

## 10. Review focus

These are the failure modes most likely to hurt a real user and are not fully covered by ordinary unit tests:

1. **Entity subscription correctness.** Removed entities and removed attributes must disappear from the UI; a stale attribute must not survive a reconnect.
2. **Bad credentials and TLS failures.** A wrong token must stop retrying; a self-signed certificate must produce an actionable error, not a silent offline state.
3. **Pi Zero 2 W resource use.** Album art decoding, JSON parsing of large snapshots, and software rendering can push RSS or CPU over budget. Measure, do not assume.
4. **Kiosk display access.** LinuxKMS needs DRM/input permissions, a seat manager or group access, and correct rotation; X11 needs DISPLAY/XAUTHORITY and cursor/blanking settings. A service that starts but cannot open the display must fail visibly.
5. **Honest stale data.** Cached values must never look live. The badge and timestamp are part of the feature, not decoration.
6. **Secret handling.** Token in logs, token in a crash message, world-readable config, or token in a screenshot are all release blockers.

Each focus item has an owning task above; the end-to-end tests in Wave 5 pin items 1, 2, and 5, and the Pi test record pins items 3 and 4.

---

## 11. Known risks and mitigations

| Risk | Impact | Mitigation |
| --- | --- | --- |
| LinuxKMS backend is marked experimental by Slint | Kiosk may not start on some panels | Keep the X11/winit fallback; test on the actual panel; pin Slint. |
| Cross-compiled glibc mismatch | Binary fails on Bullseye | Require Bookworm for cross-built binaries; document native builds. |
| Cross images change or disappear | Build breaks | Pin image digests; keep a native-build fallback. |
| Pi Zero 2 W CPU too slow for smooth swipes | Poor UX | No continuous animations; coalesce redraws; test with a 800×480 panel. |
| Home Assistant instance has hundreds of entities | Memory and traffic blow up | Subscribe only to configured entity IDs; never cache the full state set. |
| SD-card wear from cache writes | Card failure | Debounce writes (10 s), write atomically, keep the cache small. |
| Self-signed TLS on LAN | Connection fails | Document `allow_invalid_certs` as a last resort; prefer installing the CA. |
| Slint licensing for embedded distribution | Legal exposure | Decide GPL-3.0-only or commercial before distributing binaries. |
| No physical Pi available for a gate | False “done” claim | Record manual tests; mark unverified items explicitly. |

---

## 12. Open decisions

1. **License.** Slint is GPL-3.0-only, royalty-free for non-embedded use, or commercial. A single-purpose kiosk device may be classified as an embedded system. Choose GPL-3.0-only for the project (source must be available when binaries are distributed) or obtain a commercial license before distributing proprietary binaries. Do not add a license file until this is decided.
2. **Direct provider APIs.** v1 is Home Assistant-only. Weather, transit, and calendar providers stay behind Home Assistant integrations. Revisit only if a concrete provider cannot be represented by the generic entity contracts.
3. **Departures contract.** v1 uses a sensor attribute contract. If that proves too rigid, add a second adapter for a configurable JSON endpoint in v2; do not hardcode a provider.
4. **Kiosk backend support.** LinuxKMS is primary and X11 is the fallback. Wayland is supported by winit but not a release gate.
5. **UI localization.** English in v1. Keep strings centralized; add a locale file only when there is a second language.
6. **Hardware verification.** The release gate requires a Pi Zero 2 W and a Pi 3. If they are unavailable, the release is marked “x86_64 verified only”.

---

## 13. Execution

**Recommended:** subagent-driven development. Each task is small enough for a fresh implementer, and the interfaces are frozen in Wave 0. A reviewer checks each wave against its gate before the next wave starts.

Workflow:

1. Open the GitHub issue for the wave (links below).
2. Create a branch named `wave-N/<short-description>`.
3. Implement the checklist, keeping commits small and conventional (`feat:`, `fix:`, `test:`, `docs:`, `chore:`).
4. Run the local gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
5. Open a pull request that links the issue and records the gate evidence (commands and results).
6. Merge only after review. Do not push with any GitHub account other than `ABOYrbx`.

GitHub issues:

- [Wave 0 — Scaffold, interfaces, cross-compile spike](https://github.com/ABOYrbx/home-assistant-kiosk/issues/5)
- [Wave 1 — Core: config, Home Assistant client, state/cache](https://github.com/ABOYrbx/home-assistant-kiosk/issues/3)
- [Wave 2 — UI shell: navigation, touch, theme](https://github.com/ABOYrbx/home-assistant-kiosk/issues/2)
- [Wave 3 — Feature widgets: clock/weather, tiles, media, departures, calendar](https://github.com/ABOYrbx/home-assistant-kiosk/issues/6)
- [Wave 4 — Kiosk and deployment: release builds, display, systemd](https://github.com/ABOYrbx/home-assistant-kiosk/issues/1)
- [Wave 5 — Integration, performance, documentation](https://github.com/ABOYrbx/home-assistant-kiosk/issues/4)

**Next step:** Review this plan, resolve the license decision, then start Wave 0.
