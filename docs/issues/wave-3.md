# Wave 3 — Feature widgets: clock/weather, tiles, media, departures, calendar

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 3, and [the departures entity contract](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/docs/departures-entity-contract.md).
**Depends on:** Waves 1 and 2.
**Blocks:** Wave 4.
**Labels:** `implementation`, `wave-3`.

## Context

Implement the five data widgets. Each widget is independently testable with fixtures and must not require a live Home Assistant instance or a provider-specific integration.

## Checklist

### Clock and weather

- [ ] Local clock/date with a one-second timer; 12/24-hour setting.
- [ ] Weather from a configured `weather.*` entity: condition, temperature, unit, available attributes.
- [ ] Forecast via `weather.get_forecasts` with `return_response: true`; hourly/daily and count from config.
- [ ] Map Home Assistant condition strings to bundled SVG icons; neutral icon for unknown conditions.
- [ ] Refresh every 15 minutes by default; retry with backoff; keep last forecast on failure.
- [ ] Fixture tests; no live network.

### Home Assistant tiles

- [ ] Configurable grid (`columns`, explicit tile entries).
- [ ] Bind state and supported attributes; show unavailable/unknown honestly.
- [ ] Implement the action table in `PLAN.md` section 4.3; unsupported domains are read-only.
- [ ] Pending state during a service call; error state on failure; ignore repeated taps while pending.
- [ ] Tests for each supported domain; unknown domain read-only; failed call surfaces an error; rapid taps produce at most one in-flight call.

### Media player

- [ ] Parse `media_player` state, title, artist, album, `entity_picture`, volume, supported features.
- [ ] Play/pause, previous/next, volume slider; disable unsupported controls; debounce volume (max one call per 100 ms, final on release).
- [ ] Fetch album art over HTTP(S) only when needed; resolve relative URLs against the HA base URL; send the token only to the Home Assistant origin and never forward it across an origin redirect; honor `allow_invalid_certs`; decode off the UI thread into `SharedPixelBuffer`; reject images above a documented byte/pixel limit; cache by URL with a memory cap.
- [ ] Log only the image host and path, never the query string.
- [ ] Tests: parser fixtures, disabled controls, failed image fetch keeps previous art.

### Departures

- [ ] Implement the contract in `docs/departures-entity-contract.md`.
- [ ] Parse line, destination, departure, delay, platform, cancelled, remarks; sort; drop past entries; interpret timestamps without an offset in the HA timezone.
- [ ] Refresh every 60 seconds by default and immediately on reconnect; ignore stacked refreshes.
- [ ] Distinct “No departures” and “Data unavailable” states.
- [ ] Tests: malformed entries, timezone offsets, cancelled service, empty list.

### Calendar

- [ ] Call `calendar.get_events` for each configured calendar with `start_date_time`/`end_date_time` (or `duration`) and `return_response: true`.
- [ ] Interpret timestamps without an offset in the HA timezone from `get_config`; convert to device time for display.
- [ ] Merge multiple calendars, sort, render today/tomorrow.
- [ ] Handle all-day and overlapping events; one failing calendar does not hide the others.
- [ ] Refresh every 5 minutes and on reconnect.
- [ ] Fixture tests for multiple calendars, all-day events, partial failure.

## Acceptance gate

- All widgets render with fixtures on desktop.
- Each widget has unit tests, including timezone conversion, debouncing, and same-origin image tokens.
- No widget depends on a live network or a specific provider.
- No personal entity ID, stop ID, address, or provider URL is present in code or fixtures.

## Out of scope

- Tile brightness/color controls and climate setpoints (future).
- Provider-specific transit APIs.
- Final visual design.

## Notes

- The departures widget is deliberately generic. If an integration exposes another shape, the user maps it to the contract with a template sensor; do not add a parser per provider.
- Album art and icons are bounded caches; unbounded caches are a release blocker on a 512 MB device.

---

## Subtask issues

The wave issue is the epic. Every checkbox above, including the commit step, is a separate issue:

### 3.1 Clock and weather

- [ ] #57 — [W3.1.1] Clock and date from local time; one-second timer; 12/24-hour setting.
- [ ] #58 — [W3.1.2] Weather from a configured weather.* entity: condition, current temperature, unit, and a...
- [ ] #59 — [W3.1.3] Forecast via weather.get_forecasts with return_response: true; configurable hourly/dail...
- [ ] #60 — [W3.1.4] Map Home Assistant condition strings to bundled icons; unknown condition gets a neutral...
- [ ] #61 — [W3.1.5] Refresh forecast every 15 minutes by default; retry with backoff; keep the last forecas...
- [ ] #62 — [W3.1.6] Unit tests with recorded JSON fixtures; no live network in tests.
- [ ] #152 — [W3.1.7] Commit: feat: clock and weather widgets.

### 3.2 Home Assistant tiles

- [ ] #63 — [W3.2.1] Configurable grid with columns and explicit tile entries (entity, label, optional icon).
- [ ] #64 — [W3.2.2] Bind state and supported attributes; show unavailable/unknown states honestly.
- [ ] #65 — [W3.2.3] Implement the action table from section 4.3; unsupported domains are read-only.
- [ ] #66 — [W3.2.4] Show a pending state during a service call and an error if it fails; ignore repeated ta...
- [ ] #67 — [W3.2.5] Tests: mapping for each supported domain; unknown domain stays read-only; failed call s...
- [ ] #153 — [W3.2.6] Commit: feat: Home Assistant entity tiles.

### 3.3 Media player

- [ ] #68 — [W3.3.1] Parse media_player state, title, artist, album, entity_picture, volume, and supported f...
- [ ] #69 — [W3.3.2] Play/pause, previous/next, and volume slider; disable controls the entity does not supp...
- [ ] #70 — [W3.3.3] Fetch album art over HTTP(S) only when needed; resolve relative URLs against the HA bas...
- [ ] #71 — [W3.3.4] Log only the image host and path, never the query string.
- [ ] #72 — [W3.3.5] Tests: parser fixtures, disabled controls, failed image fetch keeps the previous art.
- [ ] #154 — [W3.3.6] Commit: feat: media player widget.

### 3.4 Departures

- [ ] #73 — [W3.4.1] Implement the provider-neutral contract in docs/departures-entity-contract.md: a config...
- [ ] #74 — [W3.4.2] Parse line, destination, departure time, optional delay, platform, and cancelled flag;...
- [ ] #75 — [W3.4.3] Refresh every 60 seconds by default, and immediately on reconnect; ignore stacked refre...
- [ ] #76 — [W3.4.4] Show “No departures” and “Data unavailable” as distinct states.
- [ ] #77 — [W3.4.5] Tests with fixtures, including malformed entries, timezone offsets, and a cancelled ser...
- [ ] #155 — [W3.4.6] Commit: feat: generic departures widget.

### 3.5 Calendar

- [ ] #78 — [W3.5.1] Call calendar.get_events for each configured calendar with a start/end range and return...
- [ ] #79 — [W3.5.2] Interpret timestamps without an offset in the Home Assistant timezone from get_config;...
- [ ] #80 — [W3.5.3] Merge events from multiple calendars, sort by start, and render today/tomorrow sections.
- [ ] #81 — [W3.5.4] Handle all-day events, overlapping events, and calendars that return errors independently.
- [ ] #82 — [W3.5.5] Refresh every 5 minutes and on reconnect.
- [ ] #83 — [W3.5.6] Tests with fixtures for multiple calendars, all-day events, and one failing calendar.
- [ ] #156 — [W3.5.7] Commit: feat: calendar agenda widget.

### 3.6 Adopted design port (Overview)

- [ ] #167 — [W3.6.1] Port DESIGN.md and tokens.css to ui/theme.slint; no hardcoded colours, spacing, or type...
- [ ] #168 — [W3.6.2] Build the Overview top bar and hero light card: toggle, brightness, colour temperature,...
- [ ] #169 — [W3.6.3] Implement scenes/quick actions and a configurable rooms/tile group from Home Assistant...
- [ ] #170 — [W3.6.4] Add compact layouts for 800 × 480 and 1024 × 600 plus the three-column 1280+ layout; no...
- [ ] #171 — [W3.6.5] Fix the known design defects: 44 px switch hit area, AA contrast tokens, Inter bundle/f...
- [ ] #172 — [W3.6.6] Verify the five data states (Loading, Empty, Error, Populated, Edge) and capture screen...
- [ ] #173 — [W3.6.7] Keep Energy and Schedule post-v1; do not ship the prototype's mock metrics as productio...
- [ ] #174 — [W3.6.8] Commit: feat: port adopted design to Slint Overview.
