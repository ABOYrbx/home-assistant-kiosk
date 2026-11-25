# Extension Shop Metadata Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show complete, truthful extension metadata, configuration status, and errors in the Wave 6 Shop, including offline installed extensions.

**Architecture:** Keep `ExtensionManager` as the background source of installed manifest metadata and validated configuration state. Pass complete `ShopEntry` snapshots through `ShopView` to the Slint bridge, and render explicit status/version/error labels with the existing design tokens.

**Tech Stack:** Rust, Slint 1.19, serde/TOML, existing extension manager/store/manifest APIs, `i-slint-backend-testing`.

**Spec:** `docs/superpowers/specs/2026-11-25-extension-shop-metadata-design.md`

## Global Constraints

- English only; provider-neutral examples only.
- No secrets or personal identifiers in code, tests, UI, logs, or documentation.
- File and network I/O stay on the background shop worker.
- Use existing design tokens and 44 × 44 logical-pixel touch targets.
- Do not change the v1 page set or extension lifecycle semantics.

## Review Focus

- An installed extension with no cached catalog still shows its real name, author, license, description, and permissions.
- A missing required secret reports `Configuration required`, while a complete settings/secret set reports `Configured`.
- An extension with no configuration schema reports `No configuration required`, not a false missing-configuration warning.
- A missing/corrupt installed manifest shows an error and never claims that no permissions are required.
- Long metadata and version strings do not overflow the Shop at the kiosk breakpoints.

---

### Task 1: Model configuration and metadata state

**Files:**
- Modify: `src/extensions/shop.rs`
- Modify: `src/ui/bridge.rs`
- Modify: `ui/pages/shop.slint`
- Test: `tests/shop_view_test.rs`

**Interfaces:**
- Consumes: existing `ShopEntry`, `ShopView`, and `shop_entry_view`.
- Produces: `ShopEntry.metadata_available`, `ShopEntry::configuration_status()`, and `ShopEntryView.config_status` / `installed_version`.

- [ ] Add failing view-model assertions for all four configuration states, unavailable permissions, and installed-version bridge output.
- [ ] Run `cargo test --test shop_view_test` and confirm the new assertions fail.
- [ ] Add the metadata flag/status method and bridge fields; update `ShopEntryView` and the card labels/fallbacks.
- [ ] Run `cargo test --test shop_view_test` and confirm it passes.

### Task 2: Load installed metadata and validate configuration in the manager

**Files:**
- Modify: `src/extensions/manager.rs`
- Modify: `src/events.rs`
- Modify: `src/app.rs`
- Test: `tests/extensions_test.rs`
- Test: `tests/shop_view_test.rs`

**Interfaces:**
- Consumes: `ExtensionStore`, package `load_manifest`, host `Manifest::build_config`, `ManagerOptions`, and `ShopEvent`.
- Produces: complete installed-only `ShopEntry` values, accurate `configured` values, and offline snapshots that retain installed entries.

- [ ] Add failing manager tests for installed-only metadata and required settings/secrets, plus an offline-snapshot view-model test.
- [ ] Run `cargo test --test extensions_test --test shop_view_test` and confirm the new tests fail.
- [ ] Build installed-only entries from `manifest.toml`; compute configuration with the installed manifest and declared settings/secrets; surface manifest-read errors without inventing metadata.
- [ ] Extend `ShopEvent::CatalogOffline` with the current entries and apply them before marking the view offline.
- [ ] Run `cargo test --test extensions_test --test shop_view_test` and confirm it passes.

### Task 3: Verify Slint rendering and update documentation

**Files:**
- Modify: `tests/shop_ui_test.rs`
- Modify: `docs/extensions.md`
- Modify: `config.example.toml`

**Interfaces:**
- Consumes: the completed `ShopEntryView` contract.
- Produces: UI regression coverage and operator documentation matching the displayed states.

- [ ] Extend the Slint test fixture with configuration status and installed version; assert metadata/error cards remain usable offline.
- [ ] Add Shop geometry checks at 800 × 480, 1024 × 600, 1280 × 800, 1440 × 900, and 2048 × 1200 with long metadata.
- [ ] Update the Shop documentation and example configuration comments to describe the explicit configuration states and offline metadata.
- [ ] Run `cargo test --test shop_ui_test` and confirm it passes.

### Task 4: Full verification and commit

**Files:**
- Modify: `.openchamber/logs/summary-build.md`

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test`.
- [ ] Run `python3 scripts/check-docs.py`.
- [ ] Run `python3 -m unittest discover -s tests -p 'test_*.py'`.
- [ ] Review `git diff`, commit with a Conventional Commit message, and write the required 3–6 bullet summary.
