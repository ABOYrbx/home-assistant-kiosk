# Plan Review — Fact-Check and Corrections

**Review date:** 2026-11-22
**Scope:** `PLAN.md` revision 1 (German) and the technical claims behind it.
**Method:** Checked the plan against the current Slint 1.19 documentation, the Home Assistant WebSocket API documentation, the Slint build/cross-compilation guide, the Home Assistant `calendar.get_events` and `weather.get_forecasts` action documentation, and the Slint licensing page. The plan was then rewritten in English as revision 2.

## Summary

The original plan had a sound product direction and a reasonable wave structure. It also had several technical contradictions and two design decisions that would have caused avoidable work:

1. It mixed `std::sync::Mutex` and `tokio::sync::RwLock` for `AppState` without defining which thread owns state, which conflicts with Slint’s single-threaded event loop.
2. It subscribed to every `state_changed` event even though Home Assistant provides `subscribe_entities` for an explicit entity list, which is the correct choice for a 512 MB device.
3. It hardcoded a transit provider (EFA/VMV) and a personal stop, which violates the requirement that the project work for every Home Assistant user without personal links or IDs. The revised plan is provider-neutral and Home Assistant-first.
4. It assumed Slint can hide the cursor through a window API. Slint 1.19 has no such API; cursor hiding is deployment-specific.
5. It treated cross-compilation as a Wave 4 task, even though it is the largest technical risk and should be proven in Wave 0.

The revised plan fixes these points, narrows v1 to Home Assistant as the single data source, and records the remaining legal and hardware decisions as open items.

## Blocking corrections

| # | Original claim or implication | Finding | Correction in revision 2 |
| --- | --- | --- | --- |
| 1 | `AppState` uses `Arc<Mutex<...>>` **and** `tokio::sync::RwLock` | Contradictory lock model; Slint’s UI state is not `Send`, and touching Slint from a Tokio task is wrong. | The UI thread owns `AppState`. Background tasks send `AppEvent` values over an `mpsc` channel; the UI thread applies them. No shared state lock. |
| 2 | `subscribe_events("state_changed")` for live updates | This delivers every state change in the entire Home Assistant instance and wastes CPU, memory, and network on a Pi Zero 2 W. Home Assistant documents `subscribe_entities` with an explicit `entity_ids` list as the efficient path used by the frontend. | Use `subscribe_entities` with the configured entity IDs and decode the compact `a`/`c`/`r` format. Keep `subscribe_events` only as an opt-in fallback. |
| 3 | EFA/VMV departure API and a specific stop | Provider-specific and personal. It would not work for other users and would put a real stop/link into the repository. | v1 reads departures from a configured Home Assistant sensor following `docs/departures-entity-contract.md`. No provider URL, stop ID, or personal data is compiled in. |
| 4 | “Weather: Open-Meteo or configurable” | Vague, duplicates a data source that Home Assistant already normalizes, and adds another network client and API surface. | Use a configured `weather.*` entity plus `weather.get_forecasts` with `return_response: true`. No weather API key. |
| 5 | “Calendar: iCal feed or HA calendar entity” | An iCal parser is extra code, extra dependencies, and another failure mode. Home Assistant already exposes calendar events. | Use `calendar.get_events` for configured `calendar.*` entities. |
| 6 | “Cursor hide (Slint/Window API)” | Slint 1.19 `Window` exposes fullscreen, size, position, show/hide, and redraw, but no cursor-visibility method. | Cursor hiding is a deployment concern: `unclutter` on X11, compositor configuration on Wayland, no cursor under LinuxKMS software rendering. |
| 7 | “`renderer-software` feature” is enough for the Pi | The software renderer still needs a backend. A bare Pi without X/Wayland needs `backend-linuxkms` (plus libinput) and `SLINT_BACKEND=linuxkms-software`. | Defined separate `desktop` (winit) and `kiosk` (LinuxKMS) feature sets with exact Slint features. |
| 8 | Cross-compile from macOS is a Wave 4 task | It is the highest-risk item; discovering a linker/sysroot problem after five waves is expensive. Slint’s cross images target an Ubuntu 22.04 sysroot with glibc 2.35. | Wave 0 includes a mandatory cross-compile spike. Wave 4 only finalizes artifacts. |
| 9 | Implied support for any Raspberry Pi OS | Cross-built binaries need glibc 2.35+. Raspberry Pi OS Bullseye has glibc 2.31 and will fail. | Bookworm (glibc 2.36) is required for cross-built binaries; native builds are the documented fallback for older systems. Pi Zero v1/ARMv6 is out of scope. |
| 10 | Album art via “HTTP → Slint-Image” | `slint::Image` is not `Send`, and `Image::load_from_path` needs a file. Network bytes must be decoded into `SharedPixelBuffer` or loaded with `Image::load_from_data` on the UI thread. | Fetch and decode off the UI thread with the `image` crate; send `SharedPixelBuffer` to the UI thread. Cache by URL with a memory cap. |
| 11 | systemd service plus “cursor off” and X11 autologin, without a display-access model | A system service cannot open X11 or DRM without the right session, display, and device permissions. Running as root is not acceptable. | LinuxKMS is primary with libseat/libinput, a dedicated `ha-kiosk` user, and a seat manager or explicit groups. X11 is a documented fallback with `DISPLAY`/`XAUTHORITY`. The unit never runs as root. |
| 12 | “Token format validation” | A Home Assistant long-lived token is an opaque credential; local format checks give false confidence and can reject valid tokens. | Validate non-empty/trimmed and verify by the WebSocket `auth_ok` response. Startup fails loudly on `auth_invalid`. |
| 13 | “Settings page” in the UI | The plan had no on-screen keyboard or editing model; a settings page that cannot edit anything is misleading. | Rename it to a read-only Status page (connection, last update, version, errors). Configuration stays in TOML. |
| 14 | “Save cache on every state update, debounced 2 s” | Frequent writes on an SD card wear it out and add I/O on a slow device. | Debounce writes to 10 s by default, write atomically (temp file + rename), and cache only configured views/entities. |
| 15 | “Start < 3 s” without defining what starts | Network connection cannot be part of a startup budget. | Define “first frame < 3 s with cached data”; the Home Assistant connection is asynchronous. |
| 16 | “Frame rate measure” without a target | Software rendering on a Pi Zero 2 W will not hit desktop frame rates, and promising 60 fps is unrealistic. | No continuous animations; 1 Hz clock; responsive swipe/scroll; measure idle CPU and record the number instead of a generic fps promise. |

## Clarifications and smaller fixes

| # | Topic | Correction |
| --- | --- | --- |
| 17 | Repository layout | The original tree nested everything under `kiosk-dashboard/` while the repository itself is the project. The revised layout uses the repository root as the Cargo package. |
| 18 | “Workspace structure” | The original plan said “workspace” but described a single crate. v1 is one crate with `lib.rs` + `main.rs`; no workspace. This keeps cross-compilation and tests simple. |
| 19 | Touch targets and focus | Slint does not have a CSS `:focus-visible`. The plan now asks for a visual pressed/active state, a development-build focus ring, and tested 44 × 44 logical-pixel targets. |
| 20 | Swipe navigation | Defined an axis lock and a threshold (20% of width or 60 logical pixels), with the tab bar as the reliable fallback. |
| 21 | Config permissions vs. service user | A root-owned `0600` config cannot be read by a non-root service. The plan now requires the config and token file to be owned by the service user with mode `0600`. |
| 22 | Secrets in process/logs | Added a custom redacted token type, a rule against logging the token or full URL, and a startup check for world-readable token files. |
| 23 | TLS on a LAN | `reqwest`/`tokio-tungstenite` use rustls. `allow_invalid_certs` is off by default and documented as a last resort; installing the CA is preferred. |
| 24 | Home Assistant version floor | `subscribe_entities` and service calls with `return_response` require a modern Home Assistant. The plan sets 2023.8 as the minimum and degrades gracefully for unsupported actions. |
| 25 | Reconnect behaviour | Added re-authentication, re-subscription, fresh-snapshot resync, 30-second ping/pong, exponential backoff with jitter, and a hard stop on `auth_invalid`. |
| 26 | Service-call failures | A failed action must show an error and must not leave the tile in a fake “on” state. Pending state is shown until the server confirms. |
| 27 | Cache contents | Cache only the configured views/entities, add a schema version, reject incompatible caches, and cap the file. Do not store the full Home Assistant state database. |
| 28 | Departure contract | Added `docs/departures-entity-contract.md` with a provider-neutral JSON shape, timezone handling, malformed-entry rules, and a template-sensor example using placeholders. |
| 29 | Weather icons | Bundled SVG icons mapped from Home Assistant condition strings; unknown conditions get a neutral icon; no runtime icon downloads. |
| 30 | CI | Added the gate commands (`fmt`, `clippy`, `test`) and a note that UI tests use `i-slint-backend-testing`, not a real display. |
| 31 | Slint licensing | Slint’s royalty-free license excludes embedded systems; a single-purpose kiosk may qualify as embedded. GPL-3.0-only or a commercial license is required before distributing binaries. Recorded as an open decision. |
| 32 | Hardware gates | A gate must include hardware evidence or be explicitly marked “not verified on hardware”. CI alone cannot prove the Pi memory budget or display path. |

## Verified as correct

These points from the original plan were checked and kept:

- Rust edition 2024 is stable and appropriate.
- Slint’s software renderer is a real, supported path for devices without a GPU.
- The feature name `renderer-software` is correct, and `backend-linuxkms` exists for direct DRM/KMS rendering without X11/Wayland.
- Slint’s LinuxKMS backend supports DRM dumb buffers for software rendering and falls back to LinuxFB.
- The Home Assistant WebSocket flow `auth_required` → `auth` → `auth_ok`/`auth_invalid` is correct.
- `call_service` supports `target`, `service_data`, and `return_response: true`.
- `calendar.get_events` and `weather.get_forecasts` both return their result in a response object, so they must be called with `return_response: true` over the WebSocket API.
- The proposed target triples `aarch64-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`, and `x86_64-unknown-linux-gnu` are valid.
- `cross` with Docker is the documented Slint approach for Rust cross-compilation, and Slint publishes cross images for ARM targets.
- The Pi Zero 2 W has 512 MB RAM and is a realistic lower bound for this design.
- A 150 MB RSS hard limit is plausible for a native Slint app with a software renderer, though it must be measured; the plan targets below 100 MB.
- `slint::invoke_from_event_loop` is the correct bridge from background tasks to the UI thread.
- `Window::set_fullscreen(true)` exists in Slint 1.19.
- `i-slint-backend-testing` exists for UI tests and does not require a physical display.

## Facts that changed the design

1. **`subscribe_entities` instead of `subscribe_events`.** Home Assistant’s own WebSocket documentation says `subscribe_entities` is how the frontend follows states and is more efficient. The compact format is more work to decode but is the right trade-off on a Pi.
2. **LinuxKMS is marked experimental.** The plan keeps X11 as a fallback and requires a real-panel test. Do not claim KMS support on untested hardware.
3. **glibc 2.35 is a hard floor for Slint’s cross images.** This makes Bookworm the practical target and turns “cross-compile from macOS” into a compatibility requirement, not just a build convenience.
4. **Slint licensing is not “free for everything”.** The royalty-free plan excludes embedded systems, and embedded royalties apply to commercial projects. This must be decided before binaries leave the machine.
5. **Slint has no cursor API.** The original plan implied one; deployment scripts must own this.

## Remaining open questions

1. **License:** GPL-3.0-only for the project, or a commercial Slint license if proprietary distribution is intended? This is blocking for binary distribution, not for development.
2. **Hardware availability:** Is a Pi Zero 2 W and a Pi 3 available for the Wave 4/5 gates? If not, the release must be marked “x86_64 verified only”.
3. **Kiosk backend:** Is LinuxKMS acceptable as primary, with X11 only as a fallback? The plan recommends yes.
4. **Departures contract:** Is a Home Assistant sensor attribute acceptable for v1, or is a configurable generic HTTP JSON adapter required? The plan recommends the sensor contract for v1.
5. **UI design:** The plan intentionally uses functional placeholders. A visual design pass is a separate effort after the widgets work.
6. **Localization:** English is the project and UI language in v1. A second language is future work.

## Sources

- Slint 1.19 crate features: <https://docs.rs/crate/slint/latest/features>
- Slint LinuxKMS backend: <https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/>
- Slint build and cross-compilation guide: <https://github.com/slint-ui/slint/blob/master/docs/building.md>
- Slint `Window` API: <https://docs.rs/slint/latest/slint/struct.Window.html>
- Slint `Image` API: <https://docs.rs/slint/latest/slint/struct.Image.html>
- Slint testing backend: <https://docs.rs/i-slint-backend-testing/latest/i_slint_backend_testing/>
- Slint pricing and licensing: <https://slint.dev/pricing>
- Home Assistant WebSocket API: <https://developers.home-assistant.io/docs/api/websocket/>
- Home Assistant `calendar.get_events`: <https://www.home-assistant.io/actions/calendar.get_events/>
- Home Assistant `weather.get_forecasts`: <https://www.home-assistant.io/actions/weather.get_forecasts/>
- Debian Bookworm glibc 2.36: <https://launchpad.net/debian/bookworm/+source/glibc>
