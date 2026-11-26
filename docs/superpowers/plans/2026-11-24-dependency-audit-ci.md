# Dependency Audit CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Rust dependency vulnerability scanning reproducible and mandatory on pull requests, `main`, and a weekly schedule, and record a clean review of the current lockfile.

**Architecture:** Keep the existing standalone `security` workflow, explicitly install Rust, pin `cargo-audit`, and run it against the committed lockfile. Vulnerabilities fail the gate; unmaintained warnings remain visible and are reviewed before release. Add dependency-free Python regression tests for the workflow contract. Do not add `cargo-deny` license enforcement while the project license decision remains unresolved.

**Tech Stack:** GitHub Actions, cargo-audit 0.23.0, Python standard-library `unittest`, existing documentation checks.

**Spec:** [`PLAN.md`](../../../PLAN.md), section 8, Wave 5.1, plus GitHub issue #108.

## Global Constraints

- English only.
- Workflow permissions remain `contents: read`.
- Prefer GitHub-owned or verified actions.
- Rust setup uses shell-based rustup; repository policy rejects `dtolnay/rust-toolchain`.
- No credentials, personal identifiers, provider-specific URLs, or real Home Assistant IDs.
- The project license remains undecided; do not introduce a license file or a blocking license policy.
- The committed `Cargo.lock` is authoritative.
- Heavy Rust gates run on the repository CI runner; local verification is limited to fast checks such as `cargo fmt --check`, documentation tests, and `cargo audit`.

## Review Focus

- An unpinned audit tool silently changes behavior or fails to compile on the hosted toolchain.
- The workflow stops running for pull requests or stops detecting newly published advisories.
- An audit changes the committed lockfile or hides warning output needed for release review.
- A failing advisory has no documented release-review path.
- License enforcement is added before the owner records the Slint licensing decision.

---

### Task 1: Enforce a reproducible dependency audit

**Files:**
- Create: `tests/test_security_workflow.py`
- Modify: `.github/workflows/security.yml`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: existing workflow triggers, read-only permissions, `Cargo.lock`, cargo-audit CLI.
- Produces: a pinned weekly/PR audit command whose contract is covered by Python tests.

- [x] **Step 1: Write failing workflow contract tests**

Create dependency-free tests that read `.github/workflows/security.yml` and assert:

- `pull_request:` and the weekly cron `0 6 * * 1` remain present.
- top-level permissions contain `contents: read`.
- Rust is explicitly set up with shell-based rustup before installing cargo-audit.
- the existing CI workflow uses the same policy-compliant Rust setup and installs rustfmt/clippy.
- the install command contains `cargo install cargo-audit --locked --version 0.23.0`.
- the audit command is exactly `cargo audit --file Cargo.lock`.

- [x] **Step 2: Run the tests and verify they fail**

Run: `python3 -m unittest discover -s tests -p 'test_security_workflow.py' -v`
Expected: FAIL because the current workflow does not set up Rust or pin cargo-audit.

- [x] **Step 3: Harden the workflow**

Add a shell-based rustup setup step to both workflows, install rustfmt/clippy in `ci.yml`, pin cargo-audit to `0.23.0`, and run `cargo audit --file Cargo.lock`. Preserve the existing triggers, read-only permissions, concurrency, and timeout.

- [x] **Step 4: Run the workflow tests**

Run: `python3 -m unittest discover -s tests -p 'test_security_workflow.py' -v`
Expected: PASS with all workflow contract tests green.

- [x] **Step 5: Review the current dependency findings**

Run: `cargo audit --file Cargo.lock`
Expected: no vulnerabilities. Record every warning in `docs/dependency-audit.md` with its dependency path and release disposition. If a vulnerability is reported, identify the minimal compatible dependency update, apply it, and repeat the audit plus the Rust gates rather than adding an unexplained ignore.

- [x] **Step 6: Commit**

```bash
git add .github/workflows/ci.yml .github/workflows/security.yml tests/test_security_workflow.py
git commit -m "ci: enforce reproducible dependency audit"
```

### Task 2: Document the audit gate and release review

**Files:**
- Create: `docs/dependency-audit.md`
- Modify: `docs/release-checklist.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the enforced workflow command and reviewed findings from Task 1.
- Produces: operator guidance that matches CI and records the security change and current warning dispositions for release notes.

- [x] **Step 1: Record the reviewed audit findings**

Create `docs/dependency-audit.md` with the scan date, cargo-audit version, lockfile dependency count, vulnerability result, and a table of all current warnings. For each warning, record the crate version, advisory ID, dependency path, and release disposition. State that warnings are reviewed but do not fail CI, while vulnerabilities do.

- [x] **Step 2: Update the release checklist**

In `docs/release-checklist.md`, state that the `security` workflow runs `cargo audit --file Cargo.lock` on pull requests, pushes to `main`, and weekly. Require review of a failing scheduled run and every reported warning before release, link the findings record, and state that accepted exceptions need a recorded advisory and reason. Note that cargo-deny license enforcement is deferred until the project license decision is recorded.

- [x] **Step 3: Update the changelog**

Add an Unreleased Security bullet describing the pinned audit gate, weekly advisory review, and current no-vulnerability result.

- [x] **Step 4: Verify documentation and tooling tests**

Run: `python3 scripts/check-docs.py && python3 -m unittest discover -s tests -p 'test_*.py' -v`
Expected: documentation check passes and all Python tests pass.

- [x] **Step 5: Commit**

```bash
git add docs/dependency-audit.md docs/release-checklist.md CHANGELOG.md
git commit -m "docs: document dependency audit release gate"
```

### Task 3: Run the Wave 5 automated gates and final review

**Files:**
- No source changes expected unless a gate exposes a defect.

**Interfaces:**
- Consumes: completed workflow, tests, lockfile, and release documentation.
- Produces: recorded evidence for the issue and final build summary.

- [x] **Step 1: Run fast local gates and dispatch the Rust gates**

Run locally:

```bash
cargo fmt --check
cargo audit --file Cargo.lock
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p 'test_*.py' -v
bash tests/deploy_scripts_test.sh
```

After committing, verify the `ABOYrbx` identity, push the feature branch without opening a pull request, and manually dispatch `ci.yml` and `security.yml` on that branch. Record the GitHub Actions results for fmt, clippy, tests, the kiosk build, docs, deployment scripts, and the dependency audit.

- [x] **Step 2: Review the complete branch diff**

Use the requesting-code-review workflow with a fresh reviewer. Resolve Critical and Important findings through a test-first fix pass; record deferred minors and rulings.

- [ ] **Step 3: Write the requested build summary**

Write 3–6 English bullet points to `.openchamber/logs/summary-build.md`, including the audit result and verification evidence.
