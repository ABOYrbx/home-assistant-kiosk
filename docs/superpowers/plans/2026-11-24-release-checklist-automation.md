# Release Checklist Automation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Wave 5.3.4 release checklist executable and regression-tested, including the Rust gates, cross-artifact metadata, checksum verification, changelog, and known limitations.

**Architecture:** Keep `docs/release-checklist.md` as the operator procedure and add a fail-fast `scripts/check-release.sh` entry point for its automated gates. Harden the existing artifact builder and documentation checker, and cover both with dependency-free Python tests that use stubs instead of Docker or a Rust toolchain.

**Tech Stack:** Bash, Python 3 standard library, Cargo, Cross, GitHub Actions.

**Spec:** GitHub issue #117 and `PLAN.md` section 8, Wave 5.3.4.

## Global Constraints

- English only; no secrets, personal identifiers, or provider-specific URLs/IDs.
- Do not open a pull request or push; commit with the configured `ABOYrbx` identity.
- Preserve the kiosk feature contract and the three supported target triples.
- Do not claim hardware verification, a successful cross build, or a clean Rust gate without command evidence.
- Keep the Slint license decision open and do not publish binaries.
- Use the existing worktree; do not reconstruct unrelated merged UI code.

## Review Focus

- A missing or failing release command must stop preflight before later commands run.
- Artifact/version input must not escape `dist/` or silently produce an empty release.
- Checksums must describe the final artifact bytes and be verified before publication.
- Documentation validation must actually run release-profile and performance checks after merge cleanup.
- Release notes must carry hardware evidence status, checksums, and known limitations without implying unavailable evidence exists.

---

### Task 1: Repair and test the documentation gate

**Files:**
- Modify: `scripts/check-docs.py`
- Modify: `README.md`
- Create: `tests/test_check_docs.py`

**Interfaces:**
- Consumes: existing `load_toml(path: Path)`, `config_paths(value, prefix)`, and module-level `errors` list.
- Produces: one definition each of the release constants/check functions; `main()` runs every documentation gate.

- [ ] Add tests that load `scripts/check-docs.py`, assert top-level constants/functions are unique, verify the exact release profile, reject profile drift, and verify performance-budget text is checked.
- [ ] Add a test that `config_paths` includes nested page and extension keys so README drift is detected.
- [ ] Run `python3 -m unittest tests.test_check_docs -v` and confirm the new tests fail against the merged checker.
- [ ] Remove duplicate constants/functions, restore `RELEASE_PROFILE`, and call `check_cargo_profile()` plus `check_performance_doc()` from `main()`.
- [ ] Add the missing configuration-reference rows and remove duplicated README sections introduced by consolidation.
- [ ] Run the focused tests and `python3 scripts/check-docs.py`; both must pass.
- [ ] Commit: `fix: restore release documentation gates`.

### Task 2: Add an executable release preflight

**Files:**
- Create: `scripts/check-release.sh`
- Create: `tests/test_check_release.py`
- Modify: `docs/release-checklist.md`

**Interfaces:**
- Consumes: `cargo`, `python3`, `bash`, and the existing build/checksum scripts.
- Produces: `scripts/check-release.sh [--with-artifacts]`; exit status is the first failing command's status.

- [ ] Write stub-based tests for default command order, fail-fast behavior, unknown options, and `--with-artifacts`.
- [ ] Run the focused tests and confirm they fail because the script is absent.
- [ ] Implement the script with `set -euo pipefail`, explicit command logging, exact fmt/clippy/test/audit/docs/Python/deployment gates, and optional artifact build plus `SHA256SUMS` verification.
- [ ] Document that preflight does not replace hardware, soak, license, or publication sign-off.
- [ ] Run focused tests, `bash -n scripts/check-release.sh`, and the default preflight with stubs where local tooling is unavailable.
- [ ] Commit: `feat: add release preflight checks`.

### Task 3: Harden artifact and checksum generation

**Files:**
- Modify: `scripts/build-all.sh`
- Modify: `tests/test_build_all.py`
- Modify: `docs/release-builds.md`

**Interfaces:**
- Consumes: existing `VERSION`, `TARGETS`, `DIST_DIR`, `CARGO_TARGET_DIR`, and `CROSS` environment variables.
- Produces: validated version/target names, versioned artifacts, and a verified `SHA256SUMS`.

- [ ] Add failing tests for unsafe version/target names and for checksum verification output.
- [ ] Run the focused build-tooling tests and confirm the new validation tests fail.
- [ ] Validate semver-shaped versions and target-triple characters before creating or deleting artifacts.
- [ ] Verify the generated checksum file with `sha256sum -c` and print a clear success message.
- [ ] Update the release-build documentation with validation and automatic verification behavior.
- [ ] Run all release-tooling Python tests and confirm the artifact bytes, ordering, permissions, reproducibility, cleanup, and checksums still pass.
- [ ] Commit: `fix: validate and verify release artifacts`.

### Task 4: Complete release documentation and verify the branch

**Files:**
- Modify: `docs/release-checklist.md`
- Modify: `CHANGELOG.md`
- Modify: `README.md`
- Modify: `docs/superpowers/plans/2026-11-24-release-checklist-automation.md`

**Interfaces:**
- Consumes: Task 1–3 scripts and tests.
- Produces: an enforceable checklist, changelog entry, and recorded verification evidence.

- [ ] Add tests that pin every required checklist command, target triple, checksum step, changelog requirement, known-limitations category, and sign-off row.
- [ ] Run the focused checklist test and confirm it passes only when the documentation contains the required release contract.
- [ ] Update the checklist to use preflight and verified artifact generation; clarify that manual/hardware evidence remains mandatory.
- [ ] Add the release tooling changes and current known limitations to `CHANGELOG.md`.
- [ ] Mark completed plan steps and record command evidence plus environment limits (no Docker, no cargo-audit, pre-existing Rust test/API mismatch).
- [ ] Run `cargo fmt --check`, `cargo clippy --all-targets --features desktop -- -D warnings`, `cargo test --features desktop`, a desktop release build, `python3 scripts/check-docs.py`, all Python tests, and deployment-script tests. Record exact failures without bypassing them.
- [ ] Review the diff for secrets, personal markers, unrelated edits, and English-only content.
- [ ] Commit: `docs: complete release checklist verification`.
- [ ] Write the required 3–6 bullet summary to `.openchamber/logs/summary-build.md`.
