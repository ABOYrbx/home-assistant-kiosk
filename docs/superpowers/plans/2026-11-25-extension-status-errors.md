# Extension Status Errors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show extension and catalog failures on the Status page without letting offline catalog access or a long error list make the page unusable.

**Architecture:** Reuse the `ShopView` state already pushed into `MainWindow`; pass the existing catalog online/error properties into `StatusPage` and render a distinct offline warning. Keep the Status title fixed and place all diagnostics in a vertically scrollable, clipped region.

**Tech Stack:** Rust, Slint 1.19, `i-slint-backend-testing`.

**Spec:** `PLAN.md` sections 4.6 and 8 (Wave 6), issue #138.

## Global Constraints

- English code, tests, documentation, and UI strings.
- No secrets, personal data, real provider URLs, or provider-specific IDs.
- Use only existing `Theme` tokens; no new colors, spacing, or type values.
- Catalog and extension work stays off the UI thread.
- Offline catalog state must not hide installed extensions or block page navigation.

## Review Focus

- An offline catalog with no per-extension errors still produces a visible Status diagnostic.
- A later successful catalog refresh removes the stale offline diagnostic.
- Many or long errors remain contained and scrollable at 800 × 480.
- A vertical drag inside the error list scrolls the list rather than changing pages.
- Extension-specific failures remain visible alongside a catalog-offline warning.

---

### Task 1: Surface catalog-offline diagnostics on Status

**Files:**
- Modify: `ui/main.slint`
- Modify: `ui/pages/status.slint`
- Create: `tests/status_ui_test.rs`

**Interfaces:**
- Consumes: existing `MainWindow::shop_catalog_online` and `MainWindow::shop_catalog_error` properties.
- Produces: `StatusPage::catalog_online` and `StatusPage::catalog_error` input properties.

- [ ] **Step 1: Write the failing UI test**

Create `status_page_shows_catalog_offline_error_and_clears_it_after_recovery`: select Status, set catalog offline with `network unreachable`, assert both `Catalog offline` and the reason render, then set catalog online with an empty error and assert both disappear.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test status_ui_test status_page_shows_catalog_offline_error_and_clears_it_after_recovery`
Expected: FAIL because the Status page does not render the catalog state.

- [ ] **Step 3: Pass catalog state into Status and render the warning**

Bind `shop-catalog-online` and `shop-catalog-error` through `MainWindow`; render an offline warning only while offline and the reason is non-empty. Keep per-extension errors independent.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --test status_ui_test status_page_shows_catalog_offline_error_and_clears_it_after_recovery`
Expected: PASS.

### Task 2: Keep long extension error lists usable

**Files:**
- Modify: `ui/pages/status.slint`
- Modify: `tests/status_ui_test.rs`

**Interfaces:**
- Consumes: `StatusPage::extension_errors` and the catalog warning from Task 1.
- Produces: addressable `StatusPage::status-list` Flickable and `StatusPage::status-error-message` rows.

- [ ] **Step 1: Write the failing layout/scroll test**

Create `status_page_scrolls_long_errors_without_leaving_the_page`: at 800 × 480, provide one catalog error and many long extension errors, assert the list and tabs are in bounds, drag the list upward, assert the final error moves while `current-page` remains 3, then repeat containment checks at 1024 × 600, 1280 × 800, and 2048 × 1200.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --test status_ui_test status_page_scrolls_long_errors_without_leaving_the_page`
Expected: FAIL because `StatusPage::status-list` does not exist.

- [ ] **Step 3: Add a clipped Flickable around the diagnostics**

Keep the Status title outside the scroll region. Give the Flickable the full remaining height, word-wrapped error rows, mouse-drag panning, and content sized to its preferred height.

- [ ] **Step 4: Run both Status tests to verify they pass**

Run: `cargo test --test status_ui_test`
Expected: PASS.

### Task 3: Document and verify the behavior

**Files:**
- Modify: `docs/extensions.md`

- [ ] **Step 1: Document Status diagnostics**

State that Status shows extension failures and catalog-offline reasons, that a later refresh clears the offline reason, and that long lists scroll without blocking navigation.

- [ ] **Step 2: Run focused Wave 6 checks**

Run: `cargo test --lib --test extensions_test --test host_test --test shop_view_test --test shop_ui_test --test status_ui_test`
Expected: PASS.

- [ ] **Step 3: Run formatting, lint, documentation, and build gates**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && python3 scripts/check-docs.py && cargo build`
Expected: PASS, except any separately recorded pre-existing stale-test failures that prevent the all-target lint/test suite from compiling.

- [ ] **Step 4: Commit**

```bash
git add ui/main.slint ui/pages/status.slint tests/status_ui_test.rs docs/extensions.md docs/superpowers/plans/2026-11-25-extension-status-errors.md
git commit -m "fix: keep extension status usable offline"
```
