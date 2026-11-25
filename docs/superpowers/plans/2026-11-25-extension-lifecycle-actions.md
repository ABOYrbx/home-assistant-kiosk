# Extension Lifecycle Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Harden and verify the Wave 6 extension install, update, remove, and enable/disable flows, including the first-install trust confirmation.

**Architecture:** Keep the Slint shop as a thin view over `ShopView`; validate lifecycle transitions and semver ordering in Rust before work is queued. The existing background `ExtensionManager` and `ExtensionStore` remain the only components that mutate installed packages, while UI controls reflect and enforce the single pending action.

**Tech Stack:** Rust 1.95, Slint 1.19, semver, the existing signed catalog/package store, and the Slint testing backend.

**Spec:** `PLAN.md`, sections 4.6 and 8 (Wave 6), especially tasks 6.1 and 6.3.

## Global Constraints

- English only for code, tests, comments, documentation, commit messages, and UI strings.
- Extensions remain untrusted third-party code; the trust warning and declared permissions must appear before install.
- No provider-specific URL, entity ID, stop ID, secret, or personal data may be added.
- No blocking package, filesystem, or network work on the Slint UI thread.
- Installed extensions remain usable offline; removal retains settings unless explicitly purged.
- Use existing design tokens and 44 x 44 touch targets; do not add continuous animation.
- Do not open a pull request or push; commit locally with the configured `ABOYrbx` identity.

## Review Focus

- A catalog version older than or equal to the installed version must never be offered as an update.
- Updating a disabled extension must not silently enable it.
- Removing an extension must not erase its retained configuration state.
- A stale or duplicate action result must not mutate a different pending lifecycle transition.
- Repeated taps while a package operation is pending must not queue duplicate work or replace the visible error with a busy error.

---

### Task 1: Harden lifecycle transitions in `ShopView`

**Files:**
- Modify: `src/extensions/shop.rs`
- Modify: `tests/shop_view_test.rs`

**Interfaces:**
- Consumes: `ShopEntry`, `ShopAction`, `ShopView`, and `semver::Version`.
- Produces: `ShopEntry::has_update() -> bool`; `ShopError::NoPending`; stricter `ShopView::request` and `ShopView::complete` behavior.

- [x] Add failing tests for older/equal catalog versions, disabled-extension updates, retained settings after removal, and completion without a matching pending request.
- [x] Run `cargo test --test shop_view_test` and confirm the new tests fail for the expected reasons.
- [x] Implement semver-based update detection, preserve `enabled` on update, preserve `configured` on remove, and require a matching pending request before applying a result.
- [x] Run `cargo test --test shop_view_test` and confirm all tests pass.

### Task 2: Enforce the pending action in the shop UI

**Files:**
- Modify: `ui/pages/shop.slint`
- Modify: `tests/shop_ui_test.rs`

**Interfaces:**
- Consumes: `MainWindow::shop_pending_id` and the existing shop callbacks.
- Produces: disabled action controls while `pending-id` is non-empty, with accessible button semantics and labels.

- [x] Add a failing UI test proving install/update/remove/toggle taps are ignored while an action is pending and that controls expose disabled accessibility state.
- [x] Run `cargo test --test shop_ui_test` and confirm the new test fails.
- [x] Add an enabled state to `ShopButton`, pass the page pending state into each card, and expose button role/label/disabled state without changing the visual token system.
- [x] Run `cargo test --test shop_ui_test` and confirm all tests pass.

### Task 3: Verify manager/store lifecycle behavior

**Files:**
- Modify: `src/extensions/manager.rs`
- Create: `tests/extension_manager_test.rs`

**Interfaces:**
- Consumes: existing `CatalogFetcher`, `ExtensionManager`, `ManagerOptions`, and package fixtures.
- Produces: manager-level regression coverage for install, update, disable/enable, remove, and downgrade rejection.

- [x] Add failing manager tests using fake fetchers and a temporary store; include a downgrade attempt and retained settings behavior at the store boundary.
- [x] Run `cargo test --test extension_manager_test` and confirm the downgrade test fails.
- [x] Require the catalog version to be newer before `ExtensionManager::update` installs it.
- [x] Run `cargo test --test extension_manager_test` and confirm all tests pass.

### Task 4: Document and verify the completed issue

**Files:**
- Modify: `docs/extensions.md`
- Modify: `README.md`
- Modify: `CHANGELOG.md`
- Modify: `config.example.toml` only if the operator-facing wording needs clarification.

**Interfaces:**
- Consumes: the verified behavior from Tasks 1-3.
- Produces: operator documentation matching lifecycle semantics and a local Conventional Commit.

- [x] Update the shop documentation to state newer-version-only updates, preserved disabled state, retained settings on removal, and pending-input behavior.
- [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `python3 scripts/check-docs.py`, and `python3 -m unittest discover -s tests -p 'test_*.py'`.
- [x] Review the diff for secrets, provider-specific values, token violations, and unintended files.
- [x] Commit with `fix: harden extension lifecycle actions`.
