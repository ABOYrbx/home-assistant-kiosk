# Wave 3 Feature Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore and complete the Wave 3 widget runtime and UI so clock/weather, tiles, media, departures, and calendar work from Home Assistant fixtures without provider-specific code.

**Architecture:** Keep `AppState` and widget controllers on the Slint thread. Run the existing Home Assistant client, scheduler, cache, and album-art worker in Tokio; send typed `AppEvent` messages back to the controller. Reuse the existing parsers, widget components, design tokens, and tests, rebuilding only the consolidation-broken shell and bridge.

**Tech Stack:** Rust 1.95, Slint 1.19, Tokio, chrono/chrono-tz, reqwest, image, serde.

**Spec:** `PLAN.md` sections 4 and 8 (Wave 3), `docs/departures-entity-contract.md`, `DESIGN.md`, `tokens.css`, GitHub issue #6.

## Global Constraints

- English-only code, UI strings, fixtures, documentation, and commits.
- No live Home Assistant or provider-specific dependency in tests.
- No real entity IDs, URLs, stop IDs, addresses, tokens, or personal data.
- UI thread owns state; network, file I/O, and image decoding stay off it.
- Images cross threads only as `SharedPixelBuffer`; caches are bounded.
- Home Assistant token goes only to the exact HA origin and is never logged.
- Timestamps without offsets use the Home Assistant timezone; display uses device time.
- Use existing design tokens and 44 px touch targets; verify 800×480, 1024×600, 1280, 1440, and 2048 layouts.

## Review Focus

- Redirects and relative image URLs must not leak the HA token; tests pin same-origin behavior and query-free logging.
- Forecast/calendar failures retain useful prior data and surface errors without blanking other widgets.
- Reconnect triggers fresh subscriptions plus immediate weather/calendar/departures refresh without stacked requests.
- Rapid tile/media interactions produce at most one valid in-flight action and honest pending/error transitions.
- Very long, missing, unavailable, malformed, and empty data must render all five states without overflow.

---

### Task 1: Restore the consolidated widget UI

**Files:**
- Restore from `f3c6e4c^`: `ui/theme.slint`, `ui/pages/{overview,media,agenda,status}.slint`, `ui/widgets/{clock,weather,ha_tile,hero_light,scenes,media_player,departures,calendar}.slint`, `src/sources/lights.rs`.
- Rebuild: `ui/main.slint`, `src/ui/bridge.rs`.

- [x] Restore the complete pre-consolidation widget components and tokens.
- [x] Define one `MainWindow` with the models, properties, callbacks, page host, responsive Overview mode, Shop page, and test hooks expected by existing tests.
- [x] Implement bridge functions for clock/connection, weather, tiles, hero, media/art, departures, and calendar models.
- [x] Run `cargo test --test ui_smoke_test --test ui_layout_test --test overview_hero_test` and fix compile/layout failures.

### Task 2: Rebuild the UI-thread controller and application startup

**Files:**
- Modify: `src/app.rs`, `src/main.rs`, `src/events.rs`, `src/state.rs`, `src/lib.rs`.
- Test: `tests/{hero_controller,tiles_controller,media_ui,departures_ui,calendar_ui,ui_state,app_coalesce}_test.rs`.

**Interfaces:**
- Preserve `AppController::new(MainWindow, Config, mpsc::Sender<HaCommand>)`, `apply(AppEvent)`, `window()`, `next_page()`, and `previous_page()`.
- Produce typed widget models through `src/ui/bridge.rs`; service callbacks send `HaCommand::CallService`.

- [x] Add failing controller tests for event routing, widget refresh after HA events, pending completion/failure, and reconnect behavior where coverage is missing.
- [x] Implement one controller core owning `AppState`, `TileBoard`, hero/media/departures/calendar state, redraw coalescing, and callback dispatch.
- [x] Start the HA client, cache restore/save, scheduler, and album-art task from `main.rs`; drain `AppEvent` on the Slint thread.
- [x] Update local clock/date every second and publish initial/loading/offline state before the first frame.
- [x] Run controller and UI-state tests.

### Task 3: Complete weather and calendar refresh lifecycles

**Files:**
- Modify: `src/sources/weather.rs`, `src/sources/calendar.rs`, `src/app.rs`, `src/main.rs`.
- Test: weather/calendar source and integration tests.

- [x] Add tests for forecast request shape/count, backoff/retention, reconnect refresh, and stacked-refresh suppression.
- [x] Issue `weather.get_forecasts` and per-calendar `calendar.get_events` with `return_response: true`.
- [x] Route responses by request ID; preserve last good forecast on failure and isolate calendar failures.
- [x] Refresh weather every 900 s, calendar every 300 s, and departures every 60 s by default, with immediate reconnect refresh.
- [x] Run weather/calendar/departures tests.

### Task 4: Complete media, tile, and album-art interaction/security

**Files:**
- Modify: `src/app.rs`, `src/ui/images.rs`, `src/ui/bridge.rs`, `src/sources/media.rs` if tests expose gaps.
- Test: `tests/{media_test,media_ui_test,album_art_test,tiles_test,tiles_controller_test}.rs`.

- [x] Verify feature-gated transport/volume controls, 100 ms volume throttling, final release, and one in-flight tile action.
- [x] Request album art only when the resolved URL changes; keep previous art on failure and enforce byte/pixel/cache limits.
- [x] Verify token attachment only for exact HA origin, redirect confinement, invalid-cert setting, and query-free logs.
- [x] Run media/tile/album-art tests.

### Task 5: Repair test consolidation and close acceptance gaps

**Files:**
- Modify: `tests/ui_smoke_test.rs` only to remove duplicated merge blocks/restore missing imports and helpers.
- Modify: tests and documentation only where behavior requires it.

- [x] Run the full suite to identify remaining behavioral failures; add focused regression tests before fixes.
- [x] Ensure fixtures cover loading, empty, error, populated, unavailable, long text, all-day/overlap, malformed departures, partial calendar failure, and image-failure retention.
- [x] Update `config.example.toml`, `README.md`, and relevant docs for runtime behavior and image limits.
- [x] Run formatting, clippy, tests, documentation checks, and desktop build.
- [x] Capture/verify widget layouts at required widths using the Slint testing backend or desktop screenshots; record unavailable visual checks honestly.
- [x] Review the complete diff for secrets, provider-specific values, token logging, and unintended changes.

### Task 6: Commit and hand off

- [x] Commit the completed wave with a Conventional Commit message.
- [x] Write `.openchamber/logs/summary-build.md` with 3–6 bullets.
- [x] Do not open a pull request.
