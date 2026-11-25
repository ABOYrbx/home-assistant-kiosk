# Extension Catalog Repository Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the public `ABOYrbx/home-assistant-kiosk-extensions` companion repository with a signed empty catalog, schemas, source layout, packaging/validation tooling, and CI, then record the visibility and key-custody decision in the kiosk repository.

**Architecture:** Catalog data remains in a sibling Git repository and is never committed to the kiosk repository. The kiosk repository stores only the decision, canonical catalog URL, and tests that prevent configuration drift. The companion repository uses Python validation tests plus the official Minisign CLI in CI; its initial signature uses an ephemeral bootstrap key whose secret key is destroyed after signing.

**Tech Stack:** JSON Schema 2020-12, Python 3.11+, `jsonschema`, `tomllib`, POSIX shell, tar/gzip, SHA-256, Minisign, GitHub Actions.

**Spec:** `PLAN.md` sections 4.6, 8 (Wave 6), and 12; `docs/extension-catalog.md`; `src/extensions/catalog.rs`; `src/extensions/manifest.rs`.

## Global Constraints

- English only in code, comments, documentation, tests, UI strings, commits, and issues.
- Extension source and catalog data never enter `ABOYrbx/home-assistant-kiosk`.
- The companion catalog is public for community use.
- `ABOYrbx` is the only publisher and holds the future production Minisign private key offline.
- No private key, token, real provider URL, stop ID, entity ID, address, or personal data enters either repository or CI.
- The shop remains disabled in `config.example.toml`; `public_keys` remains empty until a production key is published.
- Catalog schema version and extension API version are both `1`.
- Package URLs are HTTPS; packages are gzipped tar archives with safe relative paths.
- Initial catalog contents are empty because the example extension belongs to issue #141.
- Do not open a pull request. Commit the kiosk-repository changes in this worktree and publish the new companion repository with `ABOYrbx`.

## Review Focus

- Duplicate JSON keys must fail validation instead of being silently accepted.
- Manifest/index permission and config drift must fail validation before signing.
- Unsafe package paths, links, missing license/readme files, and oversized output must fail packaging.
- The committed signature must verify against the committed public key and fail after index tampering.
- CI must not request, read, or store a signing secret.

---

### Task 1: Record the public repository decision and canonical URL

**Files:**
- Modify: `PLAN.md`
- Modify: `docs/extension-catalog.md`
- Modify: `docs/github-rules.md`
- Modify: `README.md`
- Modify: `CONTRIBUTING.md`
- Modify: `config.example.toml`
- Create: `tests/test_extension_catalog_repository.py`

**Interfaces:**
- Consumes: existing configuration and documentation checks.
- Produces: canonical URL `https://raw.githubusercontent.com/ABOYrbx/home-assistant-kiosk-extensions/main/index.json` and a recorded public/offline-key decision.

- [ ] Write failing tests for the canonical catalog URL, public visibility decision, offline `ABOYrbx` key custody, empty production key list, and absence of tracked secret-key files.
- [ ] Run `python3 -m unittest tests/test_extension_catalog_repository.py` and verify the new tests fail for the missing decision/URL.
- [ ] Update the documentation and example configuration with the decision and canonical URL.
- [ ] Run the focused tests and `python3 scripts/check-docs.py`; verify both pass.
- [ ] Commit `docs: record public extension catalog repository`.

### Task 2: Build and test the companion repository validator

**Files:**
- Create in sibling repo: `scripts/validate.py`
- Create in sibling repo: `scripts/package.sh`
- Create in sibling repo: `tests/test_validate.py`
- Create in sibling repo: `tests/test_package.py`
- Create in sibling repo: `requirements-dev.txt`

**Interfaces:**
- Consumes: `schema/index.schema.json`, `schema/manifest.schema.json`, `index.json`, `extensions/<id>/manifest.toml`.
- Produces: `python3 scripts/validate.py` exits nonzero on schema/semantic/package-layout failures; `scripts/package.sh <id> <version>` prints SHA-256 and byte size for a safe `.hkext`.

- [ ] Write failing validator tests for valid empty index, duplicate JSON keys, unknown fields, duplicate IDs, insecure package URLs, manifest/index permission drift, unsafe entry paths, and secret defaults.
- [ ] Run the validator tests and verify they fail because `scripts/validate.py` is absent.
- [ ] Implement the validator with duplicate-key rejection, JSON Schema validation, semantic checks, manifest/index comparison, and source-tree safety checks.
- [ ] Write failing packaging tests for a valid package, missing required files, version mismatch, symlinks, and unsafe paths.
- [ ] Run the packaging tests and verify they fail because `scripts/package.sh` is absent.
- [ ] Implement deterministic safe packaging and SHA-256/size output.
- [ ] Run all companion-repository Python tests and verify they pass.
- [ ] Commit `test: validate extension catalog and packages` in the companion repository.

### Task 3: Add schemas, signed bootstrap index, source layout, and CI

**Files:**
- Create in sibling repo: `README.md`
- Create in sibling repo: `index.json`
- Create in sibling repo: `index.json.minisig`
- Create in sibling repo: `catalog.pub`
- Create in sibling repo: `schema/index.schema.json`
- Create in sibling repo: `schema/manifest.schema.json`
- Create in sibling repo: `extensions/README.md`
- Create in sibling repo: `.gitignore`
- Create in sibling repo: `.github/workflows/validate.yml`

**Interfaces:**
- Consumes: Task 2 validator and packager.
- Produces: a public repository whose empty index verifies with `catalog.pub`; no private key is retained.

- [ ] Write a failing integration test that checks the required repository files and rejects a missing or mismatched signature.
- [ ] Run the integration test and verify it fails before the scaffold exists.
- [ ] Create schemas matching the kiosk parser, empty index, source-layout README, ignore rules, and validation workflow.
- [ ] Generate an ephemeral Minisign key pair outside the repository, sign the exact index bytes, copy only the public key/signature into the repository, verify them, and destroy the secret key.
- [ ] Run `scripts/validate.sh`, the full companion test suite, Minisign verification, and a tamper check; verify all pass.
- [ ] Create the public GitHub repository `ABOYrbx/home-assistant-kiosk-extensions`, verify `gh auth status`, identity, and remote, then push the initial commit with `ABOYrbx`.
- [ ] Record the repository URL and verification evidence in the kiosk documentation and commit any follow-up changes.

### Task 4: Verify and summarize

**Files:**
- Modify: `CHANGELOG.md`
- Create: `.openchamber/logs/summary-build.md`

**Interfaces:**
- Consumes: completed Tasks 1–3.
- Produces: final gate evidence and a short build summary.

- [ ] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, focused Python tests, `python3 scripts/check-docs.py`, and the companion repository gates.
- [ ] Run `cargo test`; record the known pre-existing UI-test compile failures separately if they remain.
- [ ] Inspect both repositories for secrets, personal data, provider-specific URLs/IDs, and accidental catalog data in the kiosk repository.
- [ ] Update the changelog and write `.openchamber/logs/summary-build.md` with 3–6 bullets.
- [ ] Commit the final kiosk-repository changes and report exact verification results.
