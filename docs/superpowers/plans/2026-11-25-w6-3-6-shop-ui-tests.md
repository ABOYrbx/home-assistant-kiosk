# W6.3.6 Shop UI Testing Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add focused Slint testing-backend coverage for the Wave 6 shop install, update, remove, permission, offline, and error flows.

**Architecture:** Exercise the real `MainWindow`, `ShopView`, and UI bridge without a display or network. Drive the UI with testing-backend pointer events and assert rendered text through Slint's accessible labels.

**Tech Stack:** Rust, Slint 1.19, `i-slint-backend-testing` 1.19, existing `ShopView` state machine.

**Spec:** [`PLAN.md`](../../../PLAN.md), section 8, Wave 6, task 6.3.6; [`docs/issues/wave-6.md`](../../issues/wave-6.md).

## Global Constraints

- English only; provider-neutral placeholder IDs and hosts only.
- No live Home Assistant, catalog, package store, display, or network in tests.
- Keep Slint and testing-backend versions pinned to 1.19.
- Preserve honest state: no optimistic success before a store result.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

## Review Focus

- A first install must show the trust warning and exact declared permissions before emitting the install callback.
- Cancelling or failing an action must not move an entry into an installed or updated state.
- Offline mode must hide install/update offers while keeping installed remove/enable actions and refresh usable.
- Entry errors must be visible as text and remain associated with the correct card.
- State changes must repopulate the real Slint models so buttons appear and disappear after install, update, and remove.

---

### Task 1: Install confirmation and permission rendering

**Files:**
- Modify: `tests/shop_ui_test.rs`

**Interfaces:**
- Consumes: `ha_kiosk::MainWindow`, `ha_kiosk::extensions::shop::ShopView`, `ha_kiosk::ui::bridge`.
- Produces: reusable test helpers for rendering a `ShopView`, clicking controls, and asserting accessible text.

- [x] Replace the broad smoke test with a focused install-flow test using a real catalog `ShopEntry`.
- [x] Assert the card renders its name, declared permissions, configuration keys, and exact paths.
- [x] Assert Install opens the trust confirmation without emitting `shop-install`.
- [x] Assert Cancel closes the confirmation without emitting `shop-install`.
- [x] Assert Confirm emits the exact extension ID and closes the confirmation.
- [x] Run `cargo test --test shop_ui_test` and verify the install-flow test passes.

### Task 2: Installed actions and state transitions

**Files:**
- Modify: `tests/shop_ui_test.rs`

**Interfaces:**
- Consumes: helpers from Task 1 and `ShopView::{request, complete, apply_catalog}`.

- [x] Add a lifecycle test that confirms install, applies a successful store result, and observes the installed card.
- [x] Apply a newer catalog version and assert Update is offered; click it and assert the exact callback ID.
- [x] Apply a successful update result and assert the card shows the new version with no Update button.
- [x] Click Remove, assert the exact callback ID, apply a successful remove result, and assert the card returns to Available with Install offered.
- [x] Assert enable/disable emits the correct ID and boolean value.
- [x] Run `cargo test --test shop_ui_test` and verify the lifecycle test passes.

### Task 3: Offline and error rendering

**Files:**
- Modify: `tests/shop_ui_test.rs`

**Interfaces:**
- Consumes: helpers from Task 1 and `ShopView::mark_catalog_offline` / `ShopView::complete`.

- [x] Add an offline test using an installed entry with a pending catalog update.
- [x] Assert the offline banner and catalog error are rendered.
- [x] Assert Update and Install are absent while Remove, enable/disable, and Refresh remain usable.
- [x] Add an error test that completes an install with a fixture error.
- [x] Assert the card renders status Error, the exact error text, and an Install retry action without claiming installation.
- [x] Run `cargo test --test shop_ui_test` and verify both tests pass.

### Task 4: Full verification and commit

**Files:**
- Modify: `tests/shop_ui_test.rs`
- Create: `.openchamber/logs/summary-build.md`

- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo clippy --all-targets -- -D warnings`.
- [ ] Run `cargo test`.
- [ ] Run `python3 scripts/check-docs.py`.
- [ ] Review the diff for scope, provider neutrality, and accidental secrets.
- [ ] Commit with a Conventional Commit message.
- [ ] Write the required 3–6 bullet summary to `.openchamber/logs/summary-build.md`.
