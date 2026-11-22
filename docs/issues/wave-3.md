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
