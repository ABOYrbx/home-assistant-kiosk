# Wave 0 — Scaffold, interfaces, and cross-compile spike

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 0.
**Depends on:** nothing.
**Blocks:** every other wave.
**Labels:** `implementation`, `wave-0`.

## Context

Create the package, freeze the interfaces between waves, and prove the riskiest technical path before feature work starts. Cross-compiling Slint from macOS to a Raspberry Pi is the main schedule risk; it must be demonstrated here, not in Wave 4.

## Checklist

- [ ] Create the Cargo package `ha-kiosk` at the repository root with `src/lib.rs` and `src/main.rs`.
- [ ] Add dependencies and pin Slint 1.19; set `edition = "2024"` and `rust-version = "1.95"`. Use the exact dependency/feature contract in `PLAN.md` section 7.1.
- [ ] Add `build.rs` with `slint_build::compile("ui/main.slint")` and a minimal window that compiles.
- [ ] Implement the interfaces from `PLAN.md` section 7 as compiling stubs; add `#![deny(unsafe_code)]`.
- [ ] Add feature flags `desktop` (winit + software renderer) and `kiosk` (LinuxKMS + software renderer); verify both compile.
- [ ] Add `Cross.toml` using Slint cross images; pin image digests after verification. Add `pre-build` sysroot packages for fontconfig, libinput, libudev, libxkbcommon, and libseat. Document that `cross` requires Docker on macOS.
- [ ] Prove `cross build --release --target aarch64-unknown-linux-gnu --no-default-features --features kiosk` produces a binary. If it fails, document the error and choose the native-build fallback before Wave 1.
- [ ] Add `config.example.toml`, `.gitignore`, `scripts/run-dev.sh`, and a CI workflow skeleton that passes once `Cargo.toml` exists.
- [ ] Add `AGENTS.md` with the English-only rule, the `ABOYrbx`-only push rule, layout, and build/test commands.
- [ ] Commit: `chore: scaffold ha-kiosk and prove cross-build`.

## Files

- `Cargo.toml`, `Cargo.lock`, `build.rs`, `Cross.toml`
- `src/main.rs`, `src/lib.rs`, module stubs
- `ui/main.slint`
- `config.example.toml`, `.gitignore`
- `.github/workflows/ci.yml`
- `scripts/run-dev.sh`

## Acceptance gate

- `cargo build` and `cargo test` pass on macOS/Linux.
- The cross spike produces an aarch64 binary, or the failure is documented with an accepted fallback.
- No feature or widget logic is implemented yet.

## Out of scope

- Home Assistant connection.
- UI design.
- Deployment scripts beyond the build spike.

## Notes

- `cross` requires Docker on macOS.
- `cargo-zigbuild` is not a drop-in replacement: Slint needs native libraries such as fontconfig and xcb.
- Do not claim a Raspberry Pi build works without the actual command output.

---

## Subtask issues

The wave issue is the epic. Every checkbox above, including the commit step, is a separate issue:

### 0.1 Scaffold and interface contract

- [ ] #11 — [W0.1.1] Create the Cargo package ha-kiosk at the repository root with src/lib.rs and src/main.rs.
- [ ] #12 — [W0.1.2] Add dependencies and pin Slint 1.19; set rust-version = "1.95" and edition 2024.
- [ ] #13 — [W0.1.3] Add build.rs with slint_build::compile("ui/main.slint") and a minimal window that compi...
- [ ] #14 — [W0.1.4] Define the interfaces from section 7 as compiling stubs; add #![deny(unsafe_code)] unle...
- [ ] #15 — [W0.1.5] Add feature flags desktop (winit + software renderer) and kiosk (LinuxKMS + software re...
- [ ] #16 — [W0.1.6] Add Cross.toml using Slint cross images; pin image digests after verification. Add pre-...
- [ ] #17 — [W0.1.7] Prove the highest-risk path: cross build --release --target aarch64-unknown-linux-gnu -...
- [ ] #18 — [W0.1.8] Add config.example.toml, .gitignore, scripts/run-dev.sh, and a CI workflow skeleton tha...
- [ ] #19 — [W0.1.9] Write AGENTS.md with the language, GitHub identity, layout, and build rules.
- [ ] #145 — [W0.1.10] Commit: chore: scaffold ha-kiosk and prove cross-build.
