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
