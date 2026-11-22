# Wave 6 — Community extension shop (post-v1)

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), sections 4.6 and 8 (Wave 6); [author guide](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/docs/extensions.md); [catalog spec](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/docs/extension-catalog.md).
**Depends on:** Waves 1–3 for the HA client, UI shell, and widget rendering; start after Wave 5.
**Blocks:** nothing in the v1 gate. This is a post-v1 feature.
**Labels:** `implementation`, `wave-6`.

## Context

Build a community extension shop backed by a separate, public GitHub catalog. Extensions add data widgets such as bus departures without a new kiosk binary. The trust boundary is deliberately small: signed catalog, SHA-256 packages, sandboxed Rhai data scripts, declared permissions, declarative `departures`/`list`/`key_value` output, and hard resource limits. No native code, arbitrary Slint UI, filesystem access, or Home Assistant service calls.

## Checklist

### Catalog, signature, and store

- [ ] Define the signed catalog format (`index.json` + `index.json.minisig`) and schemas for catalog entries and manifests.
- [ ] Fetch the catalog over HTTPS; verify Minisign signatures against configured public keys; cache the last verified index; enforce a catalog size cap.
- [ ] Enforce HTTPS, size caps, SHA-256 checksums, `api_version`, and `min_app_version`. Unsigned catalogs and HTTP URLs require both `allow_unsigned` and `dev_mode`.
- [ ] Download `.hkext` packages to a temp directory; verify size and checksum before extraction.
- [ ] Reject absolute paths, `..`, symlinks, hardlinks, device files, and decompression bombs.
- [ ] Install atomically under `/var/lib/ha-kiosk/extensions/<id>/<version>/`; keep one previous version for rollback.
- [ ] Maintain `installed.json`; enforce `0600`/`0700` permissions.
- [ ] Tests: valid/invalid/tampered signature, key rotation, checksum mismatch, oversized package, path traversal, install/update/rollback/remove, offline cache.

### Sandboxed extension host

- [ ] Rhai engine with no filesystem, process, environment, or module access.
- [ ] Register only `http_get_json`, `ha_state`, `now`, and `log`; enforce manifest host/entity allowlists.
- [ ] `http_get_json`: GET only, HTTPS, 10 s timeout, 1 MB cap, no credentials, no cross-host redirects.
- [ ] `ha_state`: read-only snapshot, allowlisted entities, no service calls, no token exposure.
- [ ] Operation, string, array, map, call-depth, wall-clock, and item-count limits; run off the UI thread.
- [ ] Validate `departures`/`list`/`key_value` output; cap at 100 items; reject malformed output.
- [ ] Keep last good data on failure; mark repeated failures degraded; show errors in shop/status.
- [ ] Tests: forbidden host/entity, redirect escape, HTTP rejection, infinite loop, oversized allocation, malformed output, timeout.

### Shop UI and configuration

- [ ] Add a Shop page (fifth page, only when `extensions.enabled = true`).
- [ ] Show installed/available entries, version, author, license, description, permissions, config status, errors.
- [ ] Install, update, remove, enable/disable with a first-install trust confirmation.
- [ ] Show config keys and exact TOML/secret file paths; no on-screen keyboard in Wave 6.
- [ ] Show extension errors on the Status page; keep the shop usable offline.
- [ ] UI tests with `i-slint-backend-testing`: install/update/remove, permission display, offline, error state.

### Companion catalog repository and docs

- [ ] Create the separate `home-assistant-kiosk-extensions` repository (visibility recorded in `PLAN.md` section 12).
- [ ] Add `index.json`, `index.json.minisig`, schemas, extension sources, and validation CI.
- [ ] Add the provider-neutral `org.example.bus-departures` example using a configurable stop ID and placeholder API host.
- [ ] Document author API, manifest, permissions, config, output, packaging, versioning, security, and review.
- [ ] Document the offline Minisign signing flow; private key never enters CI or either repository.

## Acceptance gate

- A signed catalog installs, updates, rolls back, and removes the example bus-departures extension.
- A tampered index/package and a sandbox violation are rejected.
- The extension works offline after install.
- RSS stays within the Pi Zero 2 W budget.
- The shop is usable at 800 × 480.
- The catalog repository and signing documentation are complete.
- No personal stop ID, real provider URL, token, or address is present in the repository or examples.

## Out of scope

- Native code, WASM, shared libraries, or arbitrary Slint UI from extensions.
- Home Assistant service calls from extensions.
- Filesystem, process, or environment access.
- An on-screen keyboard or in-app extension configuration editor.
- Enabling the shop in the v1 release.

## Notes

- A signed catalog proves provenance, not safety. The install confirmation must say so.
- Rhai is chosen over WASM for size and Pi Zero 2 W practicality. Do not add a second runtime.
- The catalog is a separate public repository; do not put the signing private key in either repository.
