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

---

## Subtask issues

The wave issue is the epic. Every checkbox above, except the commit step, is a separate issue:

### 1.1 Configuration

- [ ] #20 — [W1.1.1] Config::load(path) -> Result<Config, ConfigError> with deny_unknown_fields.
- [ ] #21 — [W1.1.2] Token resolution order: HA_TOKEN env, token_file, literal token (development only).
- [ ] #22 — [W1.1.3] Validate URL scheme, display dimensions/rotation, refresh intervals, entity ID syntax,...
- [ ] #23 — [W1.1.4] Redact the token in Debug; unit-test that no error or debug output contains it.
- [ ] #24 — [W1.1.5] Tests: valid config, missing required field, unknown key, bad URL, token file with mode...

### 1.2 Home Assistant WebSocket client

- [ ] #25 — [W1.2.1] Implement connect, auth_required → auth → auth_ok/auth_invalid; derive the WebSocket UR...
- [ ] #26 — [W1.2.2] Call get_config after authentication; emit ha_version and the IANA time_zone.
- [ ] #27 — [W1.2.3] Implement subscribe_entities with the configured entity IDs and decode a/c/r messages,...
- [ ] #28 — [W1.2.4] Implement get_states for diagnostics and tests; do not add a global subscribe_events("s...
- [ ] #29 — [W1.2.5] Implement call_service with target, service_data, optional return_response, and a UI-ge...
- [ ] #30 — [W1.2.6] Implement a 30-second ping/pong heartbeat and a read loop that never blocks the command...
- [ ] #31 — [W1.2.7] Implement reconnect with exponential backoff and jitter, capped at 60 seconds; auth_inv...
- [ ] #32 — [W1.2.8] Emit HaEvent values; never log the token, full credential URL, or image query strings.
- [ ] #33 — [W1.2.9] Tests with a mock WebSocket server: auth success/failure, get_config timezone, path-pre...

### 1.3 State and cache

- [ ] #34 — [W1.3.1] Define AppState, connection state, stale flag, pending actions, Home Assistant version,...
- [ ] #35 — [W1.3.2] Implement a versioned JSON cache with atomic write: write a temporary file in the same...
- [ ] #36 — [W1.3.3] Store a configuration fingerprint (widget/tile/entity IDs and refresh settings) in the...
- [ ] #37 — [W1.3.4] Debounce writes (default 10 s) to limit SD-card wear; cache only the configured views a...
- [ ] #38 — [W1.3.5] Enforce cache directory 0700 and file 0600.
- [ ] #39 — [W1.3.6] Load the cache before the first frame; reject incompatible schema versions with a warni...
- [ ] #40 — [W1.3.7] Tests: round-trip, missing file, corrupt JSON, schema mismatch, fingerprint mismatch, a...
