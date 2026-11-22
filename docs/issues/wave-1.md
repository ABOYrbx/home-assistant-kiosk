# Wave 1 — Core: config, Home Assistant client, state and cache

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 1.
**Depends on:** Wave 0 (frozen interfaces).
**Blocks:** Waves 2–5.
**Labels:** `implementation`, `wave-1`.

## Context

Build the non-visual core: typed configuration, a resilient Home Assistant WebSocket client, and the state/cache model. This wave must be testable without a display or a real Home Assistant instance.

## Checklist

### Configuration

- [ ] `Config::load(path) -> Result<Config, ConfigError>` with `deny_unknown_fields`.
- [ ] Token resolution order: `HA_TOKEN`, `ha.token_file`, `ha.token` (development only).
- [ ] Validate URL scheme, display dimensions/rotation, refresh intervals, entity ID syntax, file permissions, and a non-empty resolved subscription list.
- [ ] Redact the token in `Debug`; test that no error or debug output contains it.
- [ ] Tests: valid config, missing field, unknown key, bad URL, token file `0644` rejected, `0600` accepted.

### Home Assistant WebSocket client

- [ ] Connect and complete `auth_required` → `auth` → `auth_ok`/`auth_invalid`; derive the WebSocket URL from `ha.url` without dropping a path prefix; honor `allow_invalid_certs` for `wss`.
- [ ] Call `get_config` after auth; emit the Home Assistant version (from `auth_ok`) and IANA timezone (from `get_config`).
- [ ] `subscribe_entities` with the configured entity IDs; decode `a`/`c`/`r`, including removed attributes.
- [ ] `get_states` for diagnostics and tests; no global `subscribe_events("state_changed")` subscription in v1.
- [ ] `call_service` with `target`, `service_data`, optional `return_response`, and a UI-generated `request_id`.
- [ ] 30-second ping/pong heartbeat; a read loop that does not block commands.
- [ ] Reconnect with exponential backoff and jitter, capped at 60 seconds; `auth_invalid` stops retries.
- [ ] Emit `HaEvent`; never log the token, full credential URL, or image query strings.
- [ ] Tests with a mock WebSocket server: auth success/failure, `get_config` timezone, path-prefixed URL, snapshot, add/change/remove, service response correlation, disconnect/reconnect, heartbeat timeout.

### State and cache

- [ ] Define `AppState`, connection state, stale flag, pending actions, Home Assistant version, timezone, and view models.
- [ ] Versioned JSON cache with atomic write: temporary file in the same directory, `fsync` the file, rename, `fsync` the directory.
- [ ] Cache header includes a configuration fingerprint; discard schema or fingerprint mismatches.
- [ ] Debounce writes (default 10 s); cache only configured views/entities; cap file size.
- [ ] Cache directory `0700`, cache file `0600`.
- [ ] Load the cache before the first frame; incompatible schema is a warning, not a crash.
- [ ] Tests: round-trip, missing file, corrupt JSON, schema mismatch, fingerprint mismatch, failed rename, permissions, size cap.

## Acceptance gate

- `cargo test` is green.
- A mock Home Assistant server drives a snapshot and a state change into `AppState`.
- `auth_ok` supplies the version and `get_config` the timezone; a path-prefixed `ha.url` reaches `<prefix>/api/websocket`.
- A reconnect re-authenticates, re-subscribes, and replaces state from a fresh snapshot.
- The cache rejects a mismatched fingerprint and is written `0600`.
- No token appears in logs, errors, or test output.

## Out of scope

- Slint pages and widgets.
- Deployment and display backends.

## Notes

- Use `subscribe_entities`; v1 has no global `state_changed` subscription.
- The UI thread owns `AppState`; this wave provides the event types that a later UI bridge consumes.
