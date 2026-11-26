# Dependency Audit Record

This document records the Rust dependency audit reviewed for the Wave 5 release gate. The [`security` workflow](../.github/workflows/security.yml) runs the same command on pull requests, pushes to `main`, and weekly.

## Reproduce

```bash
cargo install cargo-audit --locked --version 0.23.0
cargo audit --file Cargo.lock
```

Vulnerabilities fail the command and the CI job. Maintenance warnings are printed for review but do not fail CI. Every warning must be reviewed again before a release; a newly reported vulnerability blocks the release until it is fixed or explicitly accepted with a recorded reason.

## Latest review

- Date: 2026-11-26
- cargo-audit: 0.23.0
- Advisory database: 1,568 advisories
- Lockfile packages scanned: 630
- Known vulnerabilities: 0
- Maintenance warnings: 9, all `unmaintained`
- Result: pass, with nine warnings recorded below

No warning has a fixed upgrade in the advisory database. The two direct time dependencies need an explicit re-review before each release; the remaining warnings arrive through Slint or Rhai and require upstream replacement.

| Crate | Version | Advisory | Dependency path | Release disposition |
| --- | --- | --- | --- | --- |
| `bincode` | 2.0.1 | [RUSTSEC-2025-0141](https://rustsec.org/advisories/RUSTSEC-2025-0141) | `i-slint-compiler` → `typed-index-collections` (build/macro path) | Accepted transitive warning for v1; Slint controls the replacement. |
| `chrono` | 0.4.45 | [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543) | Direct dependency; also used by `i-slint-core` | Accepted for v1 because no fixed upgrade exists. Reassess a maintained date/time replacement before every release. |
| `chrono-tz` | 0.10.4 | [RUSTSEC-2026-0555](https://rustsec.org/advisories/RUSTSEC-2026-0555) | Direct dependency | Accepted for v1 because no fixed upgrade exists. Reassess together with `chrono` before every release. |
| `pin-utils` | 0.1.1 | [RUSTSEC-2026-0441](https://rustsec.org/advisories/RUSTSEC-2026-0441) | `spin_on` → `slint-build` / `slint-macros` | Accepted transitive warning for v1; Slint controls the replacement. |
| `smartstring` | 1.0.1 | [RUSTSEC-2026-0249](https://rustsec.org/advisories/RUSTSEC-2026-0249) | `rhai` | Accepted transitive warning for v1; Rhai controls the replacement. |
| `ttf-parser` | 0.25.1 | [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192) | `owned_ttf_parser` → `ab_glyph` → `sctk-adwaita` → `winit` → `i-slint-backend-winit` | Accepted transitive warning for v1; Slint/winit control the replacement. |
| `zbus_names` | 4.3.5 | [RUSTSEC-2026-0371](https://rustsec.org/advisories/RUSTSEC-2026-0371) | `zbus` → `i-slint-backend-winit` | Accepted transitive warning for v1; Slint/zbus control the replacement. |
| `zvariant` | 5.15.1 | [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370) | `zbus` / `zbus_macros` / `zbus_names` → `i-slint-backend-winit` | Accepted transitive warning for v1; Slint/zbus control the replacement. |
| `zvariant_derive` | 5.15.1 | [RUSTSEC-2026-0372](https://rustsec.org/advisories/RUSTSEC-2026-0372) | `zvariant` → `zbus` → `i-slint-backend-winit` | Accepted transitive warning for v1; Slint/zbus control the replacement. |

## License checks

`cargo deny check licenses` is intentionally not enabled yet. The project license is still `UNSPECIFIED`, and the Slint license decision in [`PLAN.md`](../PLAN.md) section 12 is a release prerequisite. A blocking license policy must be added after that decision so it reflects the owner's chosen terms rather than an unresolved legal question.
