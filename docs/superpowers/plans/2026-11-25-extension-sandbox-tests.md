# Extension Sandbox Boundary Tests Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Wave 6.2.8 coverage for extension host permissions, redirects, HTTP rejection, resource limits, malformed output, and timeouts.

**Architecture:** Extend the existing `tests/host_test.rs` integration suite so every denial is observed at the public `ExtensionHost` boundary. Use the existing fake fetchers for deterministic permission checks and loopback HTTP servers for the production reqwest transport. Fix only behavior exposed by a failing test.

**Tech Stack:** Rust, Rhai, reqwest blocking client, serde_json, standard-library TCP test servers.

**Spec:** `PLAN.md`, sections 4.6 and 8 (Wave 6.2 and the Wave 6 gate).

## Global Constraints

- Tests must never call a live third-party service.
- Use placeholder hosts/entity IDs and loopback addresses only.
- Do not add secrets or provider-specific URLs/IDs.
- Resource limits come from `Limits`; do not weaken defaults to make tests pass.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.

## Review Focus

- A URL denied by policy must never reach the fetcher.
- A declared HTTP host must not permit a different port, credentials, or plain HTTP in production.
- An entity snapshot present in `HostConfig` but absent from the manifest must remain inaccessible.
- A redirect must not cause a request to its destination, even when both servers are local test fixtures.
- A fetch that returns after the wall-clock deadline must fail the run rather than produce output.
- Resource-limit tests must terminate quickly and assert the specific limit that fired.

---

### Task 1: Prove the wall-clock deadline covers blocking host calls

**Files:**
- Modify: `tests/host_test.rs`
- Modify: `src/extensions/host.rs` only if the new test exposes a gap

**Interfaces:**
- Consumes: `HttpFetcher`, `HostConfig`, `ExtensionHost::refresh`, `ExtensionError::Timeout`.
- Produces: no new public interface.

- [x] **Step 1: Add a delayed fetcher and a failing host-level timeout test**

Add a test-only fetcher that records its call count, sleeps beyond a deliberately short host deadline, and returns valid JSON. Assert `ExtensionError::Timeout` and exactly one fetcher call.

- [x] **Step 2: Run the test and verify it fails for the missing deadline check**

Run: `cargo test --test host_test wall_clock_deadline_covers_a_slow_fetch -- --exact --nocapture`
Expected: FAIL because the current host returns successful output after the delayed fetch.

- [x] **Step 3: Enforce the deadline after the blocking fetch**

Check the existing run deadline immediately after `HttpFetcher::get_json` returns, before converting or returning the value.

- [x] **Step 4: Run the test and verify it passes**

Run: `cargo test --test host_test wall_clock_deadline_covers_a_slow_fetch -- --exact`
Expected: PASS.

### Task 2: Close permission and redirect observation gaps

**Files:**
- Modify: `tests/host_test.rs`

**Interfaces:**
- Consumes: `HttpFetcher`, `HttpPolicy`, `Manifest`, `EntitySnapshot`, `ReqwestFetcher`.
- Produces: no new public interface.

- [x] **Step 1: Add host-level HTTP rejection tests**

Assert production mode rejects loopback HTTP and development mode rejects non-loopback HTTP, with a panic-on-call fetcher proving no transport side effect. Assert URL credentials are denied before the fetcher runs.

- [x] **Step 2: Strengthen forbidden-entity coverage**

Provide a snapshot for an undeclared entity in `HostConfig`; assert `ha_state` still rejects it.

- [x] **Step 3: Make redirect escape observable**

Run a loopback source server that redirects to a second loopback server. Assert the host rejects the redirect and the destination server records zero requests.

- [x] **Step 4: Run the focused tests**

Run: `cargo test --test host_test`
Expected: PASS, including the pre-existing operation, allocation, malformed-output, and script-timeout tests.

### Task 3: Verify the complete change and record evidence

**Files:**
- Modify: `tests/host_test.rs`
- Modify: `src/extensions/host.rs`
- Modify: `docs/extensions.md`
- Create: `.openchamber/logs/summary-build.md`

**Interfaces:**
- Consumes: all Task 1 and Task 2 tests.
- Produces: a committed, reviewed test package and a short build summary.

- [x] **Step 1: Run formatting, lint, tests, and documentation checks**

Run the repository gates and record each result.

- [ ] **Step 2: Review the complete diff and commit**

Check for secrets, provider-specific values, brittle timing, and accidental scope expansion. Commit with a Conventional Commit message.

- [ ] **Step 3: Write the requested build summary**

Write 3–6 English bullets to `.openchamber/logs/summary-build.md`.
