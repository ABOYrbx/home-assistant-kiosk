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

### 2.1 Repository governance

- The repository is private and **`ABOYrbx` is the only collaborator**. Do not invite users, teams, bots, or deploy keys.
- `main` is the only long-lived branch. Use short-lived `wave-N/...`, `docs/...`, or `fix/...` branches.
- The policy is pull-request-only, squash-merge-only, linear history, no force-push, no deletion of `main`, and a passing `docs` check. Use the pull request template and CODEOWNERS.
- GitHub Free does **not** support branch protection or rulesets for private repositories. Until the repository is public or on GitHub Pro, these are process rules, not a technical block. Never treat the missing block as permission to push directly. The remaining technical step is tracked in issue #10.
- Apply the available settings with `scripts/apply-github-settings.sh`; the full policy is in [`docs/github-rules.md`](docs/github-rules.md).
- Releases and `v*` tags are made by `ABOYrbx` from `main`, with checksums and a changelog.
- Workflows run with read-only `GITHUB_TOKEN` permissions by default and may not approve pull requests. Prefer GitHub-owned or verified actions.
- Do not bypass a failing check or a review rule silently. Record an emergency exception in the issue or pull request and restore the rule immediately.
- Security reports go through [`SECURITY.md`](SECURITY.md), not a public issue. Conduct is covered by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).
- Local tool state is ignored and must not be force-added: `node_modules/`, `.design-studio/`, `.design-studio.json`, `.openchamber/screenshots/`, `.openchamber/tmp/`, and `.opencode/`. Source-like design files (`DESIGN.md`, `tokens.css`, `styles.css`, `index.html`, `package.json`, `package-lock.json`) stay visible so the owner can decide whether to commit them.
- Extension source and catalog data never enter this repository; use the separate catalog repository.

## 3. What this project is

A native Home Assistant kiosk dashboard:

- Rust 1.95+, edition 2024.
- Slint 1.19 for the UI.
- Tokio for background I/O.
- One Home Assistant WebSocket connection for state and service calls.
- TOML configuration; no provider-specific code.
- Targets: Raspberry Pi Zero 2 W, Raspberry Pi 3, generic Linux x86_64/ARM.
- Touch-first, full-screen, offline-tolerant, low memory.
- Post-v1 (Wave 6): a community extension shop fed by a separate, signed catalog repository. Extensions are sandboxed data scripts, not native code.

Read [`PLAN.md`](PLAN.md) for the full specification. This file is the short operating manual; `PLAN.md` wins if the two disagree.

## 4. Repository layout

```text
.
├── .github/                     # workflows, issue/PR templates, CODEOWNERS
├── AGENTS.md                    # this file
├── CODE_OF_CONDUCT.md
├── CONTRIBUTING.md
├── PLAN.md                      # implementation plan and source of truth
├── README.md                    # operator-facing overview
├── SECURITY.md
├── herdr.sh                     # interactive Herdr agent dispatcher
├── DESIGN.md                    # adopted design system (authoritative)
├── tokens.css                   # design tokens
├── index.html                   # Vite design reference prototype
├── styles.css                   # prototype layout and components
├── package.json                 # prototype only; not the kiosk runtime
├── package-lock.json
├── Cargo.toml                   # package `ha-kiosk`
├── Cargo.lock
├── Cross.toml                   # `cross` targets and Slint images
├── build.rs                     # compiles ui/main.slint
├── config.example.toml          # safe placeholders only
├── docs/
│   ├── plan-review.md           # fact-check findings and corrections
│   ├── departures-entity-contract.md
│   ├── extensions.md            # Wave 6 author guide
│   ├── extension-catalog.md     # Wave 6 catalog/signing spec
│   ├── github-rules.md          # repository and branch policy
│   ├── herdr-agent.md           # Herdr dispatch guide
│   ├── performance.md           # Pi Zero 2 W budgets and measurement record
│   ├── agent-prompts/
│   │   └── implement-issue.md   # implementation prompt template
│   └── issues/                  # subtask index, mapping, and wave epic drafts
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
│   ├── extensions/              # Wave 6: catalog, store, Rhai host
│   └── ui/                      # bridge, view models, image decoding
├── ui/
│   ├── main.slint
│   ├── theme.slint
│   ├── pages/                   # overview, media, agenda, shop (Wave 6), status
│   └── widgets/                 # clock, weather, tile, media, departures, calendar
├── deploy/                       # systemd unit and install scripts
├── scripts/                      # build, dev, docs, and performance helpers
└── tests/                        # integration and UI smoke tests (extensions_test.rs in Wave 6)
```

There is no nested `kiosk-dashboard/` directory and no multi-crate workspace in v1. The repository root is the Cargo package.

## 5. Build and test commands

```bash
# Formatting, linting, tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Dependency vulnerability check (Wave 5 and before release)
cargo audit

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

# Documentation, configuration, and performance record checks
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p 'test_*.py'
```

- `Cargo.lock` is committed; this is an application, not a library.
- Keep the `rust-version` field at `1.95` unless a Slint upgrade requires newer.
- Do not add a dependency without checking its license, maintenance status, and target support.
- Do not enable Slint's default features on the kiosk build. The software renderer is mandatory for the Pi Zero 2 W.
- Release profile: `opt-level = "s"`, LTO, `strip`, `panic = "abort"`. Measure before changing these.
- Performance records live in [`docs/performance.md`](docs/performance.md). Check a device record with `python3 scripts/perf-budget.py <record.json>`; never replace a measured value with an estimate.

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
11. **Timezone correctness.** Call `get_config` after authentication and use its `time_zone` for timestamps without an offset. Never assume the kiosk and Home Assistant share a timezone.
12. **Interaction arbitration.** Page swipe starts only from non-interactive background/edge zones. Sliders and vertical lists own their gestures. Debounce volume changes and ignore taps while an action is pending.
13. **Cache identity.** The cache header carries a schema version and a configuration fingerprint; a mismatch discards the cache. The cache is `0600` and its directory `0700`.

## 7. Slint rules

- Pin Slint to `1.19.x`; do not mix versions of `slint`, `slint-build`, and `i-slint-backend-testing`.
- Use the exact dependency/feature contract in `PLAN.md` section 7.1. Do not guess Slint feature names; `svg` and `image-decoders` are not `slint` crate features in 1.19.
- Base features: `compat-1-18`, `std`, `renderer-software`.
- Desktop feature: `backend-winit`.
- Kiosk feature: `backend-linuxkms`, `backend-linuxkms-libinput`, `backend-linuxkms-libseat`.
- PNG/JPEG and SVG icons are compiled at build time via `@image-url`; runtime album art is decoded with the `image` crate. Do not rely on `slint::Image::load_from_path` unless the runtime decoder feature is verified.
- Do not build with `--all-features` on macOS or in a non-Linux CI job: the LinuxKMS backend is Linux-only.
- LinuxKMS is primary for the kiosk; X11 + winit is the fallback. `SLINT_BACKEND=linuxkms-software` selects the direct-to-DRM path.
- Slint 1.19 does not expose a cursor-visibility API. Cursor hiding belongs to deployment: `unclutter` on X11, compositor settings on Wayland, and a touch-only LinuxKMS path with no pointer needed. Verify the panel behaviour on the target device.
- Use `slint::Window::set_fullscreen(true)` for fullscreen. Rotation is a deployment/backend concern (`SLINT_KMS_ROTATION`).
- UI tests use `i-slint-backend-testing`; do not require a real display in CI.
- Do not add continuous animations. A 1 Hz clock is the expected baseline.

### Design system rules

- `DESIGN.md` and `tokens.css` are authoritative. The Vite prototype (`index.html`/`styles.css`) is a reference; the production UI is Slint.
- No hardcoded colour, spacing, type size, radius, or motion value when a token exists. Port tokens to `ui/theme.slint` in Wave 3.6.
- The Overview follows the adopted prototype: top bar, hero light with brightness/colour temperature, scenes/quick actions, and a configurable rooms/tile group. Energy and Schedule are post-v1.
- Verify 800 × 480, 1024 × 600, 1280+, 1440, and 2048; no horizontal page scroll; 44 px touch targets; AA contrast; five data states.
- Bundle Inter with its OFL licence or use the system sans fallback; never fetch a font at runtime.
- Do not port CSS `color-mix`, blur, or heavy shadows to Slint. Use flat token colours and simple gradients; simplify the lamp glow for the software renderer.
- Never ship the prototype's mock values as production data. Rooms, scenes, lights, and metrics come from Home Assistant or configuration.
- A design change updates `DESIGN.md`, `tokens.css`, the prototype, and the Slint implementation together.

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
- Send the token only to the Home Assistant origin. Never send it to a third-party image host. Do not log image query strings.
- The config file is mode `0600` and owned by the service user. The systemd unit must not run as root.
- The cache is mode `0600`; its directory is `0700`. It may reveal presence, temperatures, and media habits.
- No secrets in test fixtures, screenshots, issue bodies, commit messages, or logs.
- Release artifacts never bundle a config file or a token.
- Fonts and icons need a documented, compatible license; record attribution in `README.md`.

## 10. Issues, branches, and pull requests

- Work one **subtask issue** per checkbox. The wave issue is the **epic**; `PLAN.md` is the specification and [`docs/issues/index.md`](docs/issues/index.md) maps every subtask to its issue.
- Branch naming: `wave-0/scaffold`, `wave-1/ha-client`, `wave-3/media`, and so on. Add the subtask id when it helps: `wave-3/media-W3.3.2`.
- One pull request per subtask or coherent group unless the wave is split by an explicit agreement. A pull request records:
  - the subtask issue it closes and the parent epic,
  - the commands run and their result,
  - any manual/hardware test evidence,
  - known limitations.
- Close a subtask issue only when its definition of done is met. Close the epic only when every subtask and the wave gate are complete.
- Do not mark a wave complete while a gate is unverified. If hardware is unavailable, write “not verified on hardware” and list what was tested instead.
- Reviewers check the gate, not only the diff. A green unit test suite does not prove the Pi memory budget or the display path.

### Herdr dispatch

- Use `./herdr.sh` inside a Herdr pane. It selects 1–4 issues, creates a new workspace, and launches one agent per issue.
- One subtask issue per worktree and branch; each agent gets its own tab titled with the issue and its own PR.
- The script creates the worktrees/tabs, starts the agents, runs gates, and opens PRs. The agents must not push or merge.
- Review every PR before merging; no script merges.
- If an agent is blocked, answer it in Herdr; do not let the script guess.
- The implementation prompt lives in [`docs/agent-prompts/implement-issue.md`](docs/agent-prompts/implement-issue.md); update the template, not inline shell text.
- See [`docs/herdr-agent.md`](docs/herdr-agent.md).

## 11. Definition of done

A task is done when:

- [ ] The code builds with the declared feature set.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass; `cargo audit` is clean before a release.
- [ ] New behavior has tests or a recorded manual test.
- [ ] Timezone, gesture, pending, offline, empty, and unavailable states are handled where relevant.
- [ ] No secret, personal identifier, or provider-specific URL was added.
- [ ] Documentation and `config.example.toml` match the behavior.
- [ ] The commit messages and issue/PR text are in English.
- [ ] The push used the `ABOYrbx` account.

## 12. Extension rules (Wave 6, post-v1)

The community extension shop is a post-v1 feature. Until Wave 6 starts, keep the v1 core free of extension code and do not enable a shop.

- **Trust:** extensions are untrusted third-party code. A signed catalog proves provenance, not safety. Show a warning and the declared permissions before installation.
- **Provider neutrality:** provider URLs, stop IDs, and API hosts live only in extension manifests and settings in the separate catalog/configuration, never in the kiosk source or examples.
- **No native code:** extensions are Rhai scripts plus a manifest and optional assets. No shared libraries, executables, WASM, or arbitrary Slint files.
- **No arbitrary UI:** an extension returns only `departures`, `list`, or `key_value` data. The built-in widgets render it.
- **No host escape:** do not register filesystem, process, environment, module, or unrestricted network functions in Rhai.
- **Read-only HA access:** `ha_state` accepts only the entity IDs declared in the manifest, returns a snapshot, and never calls a service or exposes the token.
- **Network allowlist:** `http_get_json` is GET-only, HTTPS-only, host-allowlisted, credential-free, timeout-bound, size-capped, and must not follow cross-host redirects.
- **Limits:** set operation, string, array, map, call-depth, wall-clock, package-size, decompressed-size, and item-count limits. Run scripts off the UI thread.
- **Signatures:** the catalog index is Minisign-signed. Verify it against configured public keys; verify the package SHA-256 and size. Unsigned mode and HTTP catalog URLs are development-only and must be visibly disabled in production.
- **Packages:** reject absolute paths, `..`, symlinks, hardlinks, device files, and decompression bombs during extraction. Install atomically and keep one version for rollback.
- **Secrets:** per-extension secrets live in a separate `0600` file; never in the manifest, catalog, logs, or screenshots.
- **Catalog:** the catalog is a separate repository. The signing private key stays offline and never enters CI or either repository. The catalog must be public for community use.
- **Review:** check license, permissions, no secrets, read-only HA access, no service calls, no arbitrary UI, tests, and removal process.
- **Tests:** tampered index/package, key rotation, path traversal, forbidden host/entity, redirect escape, operation/memory limits, malformed output, offline install, update/rollback/remove.

## 13. License note

Slint is available under GPL-3.0-only, a royalty-free license that excludes embedded systems, or a commercial license. A single-purpose kiosk device may be classified as an embedded system. Do not add a project license or distribute binaries until the owner records the license decision in `PLAN.md` section 12. This is a legal decision, not an implementation detail.
