# Validated Bus Departures Example Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Make the documented `org.example.bus-departures` example a validated contract of the kiosk's real manifest parser and Rhai sandbox.

**Architecture:** Keep the copyable manifest, script, README, and license in `docs/extensions.md`, marked with stable HTML comments. Add a Rust integration test that extracts those files, builds and safely extracts a `.hkext`, validates both manifest representations, and executes the script with an in-memory HTTP fixture. No extension package or catalog data is added to the kiosk repository.

**Tech Stack:** Rust, Rhai 1.27, serde/TOML/JSON, existing Wave 6 extension host and runner.

**Spec:** `docs/superpowers/specs/2026-11-25-validated-bus-departures-example-design.md`

## Global Constraints

- English only.
- No real provider URL, stop ID, entity ID, token, address, or personal data.
- Placeholder API host only: `api.example.com`.
- Extension ID: `org.example.bus-departures`.
- Required `stop_id`; optional `limit` with default `6`, minimum `1`, maximum `20`.
- Output kind: `departures`.
- Tests use recorded in-memory fixtures and never access the network.
- Extension source and catalog data remain outside the kiosk repository.

## Review Focus

- A stop ID containing query delimiters must not alter the requested URL; the example rejects it before fetching.
- Optional provider fields may be absent or null and must map to the output defaults.
- A provider response with more rows than `limit` must be truncated by the script.
- An empty provider list must produce a valid empty departures view.
- An HTTP failure must propagate while `ExtensionRunner` retains the previous good output.

---

### Task 1: Extract and validate the canonical example

**Files:**

- Create: `tests/extension_example_test.rs`
- Modify: `docs/extensions.md`

**Interfaces:**

- Consumes: `ha_kiosk::extensions::{ExtensionManifest, Manifest}` and the marked TOML/Rhai fences in `docs/extensions.md`.
- Produces: `example_manifest() -> Manifest`, `example_script() -> &'static str`, and `documented_example_is_valid()`.

- [x] **Step 1: Write the failing extraction and schema test**

Add `documented_example_is_valid()` to `tests/extension_example_test.rs`. Extract the fenced blocks between `<!-- example:manifest -->` and `<!-- /example:manifest -->`, and between `<!-- example:script -->` and `<!-- /example:script -->`. Assert the ID, version, output kind, exact HTTP allowlist, empty HA allowlist, required `stop_id`, and bounded defaulted `limit`. Parse the TOML with both `Manifest::parse` and `ExtensionManifest::parse_toml`, then validate the latter against version `0.6.0`.

- [x] **Step 2: Run the test and verify it fails**

Run: `cargo test --test extension_example_test documented_example_is_valid`
Expected: FAIL because the documentation markers do not exist.

- [x] **Step 3: Mark and harden the documented example**

Add the markers around the existing manifest fence. Update the manifest help text to state the safe stop-ID character set. Replace the script fence with a script that trims and validates `stop_id`, requests `https://api.example.com/departures?stop=<id>&limit=<limit>`, maps required fields, applies optional defaults, and stops after `config.limit` rows.

- [x] **Step 4: Run the extraction and schema test**

Run: `cargo test --test extension_example_test documented_example_is_valid`
Expected: PASS.

### Task 2: Execute the example against fixtures

**Files:**

- Modify: `tests/extension_example_test.rs`

**Interfaces:**

- Consumes: `ExtensionHost`, `HostConfig`, `HttpFetcher`, `ExtensionOutput`, and the extracted example.
- Produces: fixture-backed tests for package layout, populated, empty, limited, invalid-stop-ID, and provider-error behavior.

- [x] **Step 1: Write failing fixture execution tests**

Add a recording fake fetcher and tests asserting:

- The four marked files build a `.hkext` with `manifest.toml`, `main.rhai`, `README.md`, and `LICENSE` at its root.
- `stop_id = "REPLACE_ME"` requests exactly `https://api.example.com/departures?stop=REPLACE_ME&limit=6`.
- Two fixture rows map line, destination, departure, delay, platform, cancelled, and remarks; absent optional fields use defaults.
- `limit = 1` returns one row from a two-row response.
- `departures: []` returns an empty departures view.
- `stop_id = "bad&host=evil.example"` fails before the fetcher is called.
- A fetcher error is returned by `ExtensionHost::refresh`.

- [x] **Step 2: Run the fixture tests and verify they fail**

Run: `cargo test --test extension_example_test`
Expected: FAIL until the documented script implements the tested behavior.

- [x] **Step 3: Adjust the example script until the fixture tests pass**

Keep the script limited to the four registered host functions and ordinary Rhai language features. Do not add host APIs solely for the example.

- [x] **Step 4: Run the fixture tests**

Run: `cargo test --test extension_example_test`
Expected: PASS.

### Task 3: Prove failure retention and document validation

**Files:**

- Modify: `tests/extension_example_test.rs`
- Modify: `docs/extensions.md`
- Modify: `CHANGELOG.md`

**Interfaces:**

- Consumes: `ExtensionRunner` and a switchable fake fetcher.
- Produces: `provider_error_keeps_last_good_example_output()` and author-guide validation notes.

- [x] **Step 1: Write the failing runner test**

Use a fetcher that succeeds once and then fails. Refresh an `ExtensionRunner`, assert the first output is retained after the failure, and assert one consecutive failure is recorded without degradation.

- [x] **Step 2: Run the runner test and verify it fails**

Run: `cargo test --test extension_example_test provider_error_keeps_last_good_example_output`
Expected: FAIL until the fixture fetcher supports switching to an error.

- [x] **Step 3: Implement the switchable fixture and update documentation**

Add the fetcher behavior. State in `docs/extensions.md` that CI extracts and executes this exact example, list the safe stop-ID characters, and explain empty/error behavior. Add an Unreleased changelog entry.

- [x] **Step 4: Run the focused suite**

Run: `cargo test --test extension_example_test`
Expected: PASS.

### Task 4: Run the repository gate and commit

**Files:**

- Modify: `README.md`
- Review all changes from Tasks 1-3.

- [x] **Step 1: Format and lint**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [x] **Step 2: Run tests and documentation checks**

Run: `cargo test && python3 scripts/check-docs.py && python3 -m unittest discover -s tests -p 'test_*.py'`
Expected: PASS.

- [x] **Step 3: Review the diff for repository-boundary and privacy violations**

Confirm no `.hkext`, catalog index, signing key, real provider marker, or personal identifier was added.

- [x] **Step 4: Commit**

Run: `git add docs/extensions.md docs/superpowers CHANGELOG.md tests/extension_example_test.rs && git commit -m "test: validate bus departures extension example"`
Expected: one Conventional Commit on the issue branch.

## Execution note

The full `cargo clippy --all-targets -- -D warnings` and `cargo test` commands were run, but `main` already contains unrelated UI-test compilation errors. The changed library and extension targets pass `cargo clippy --lib --test extension_example_test -- -D warnings` and `cargo test --lib --test extension_example_test --test host_test --test extensions_test --test shop_view_test --test shop_ui_test`. Documentation and Python checks pass.
