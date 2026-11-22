# Wave 5 — Integration, performance, and release

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 5.
**Depends on:** Wave 4.
**Blocks:** first release.
**Labels:** `implementation`, `wave-5`.

## Context

Prove the system under realistic failure modes and on the actual target hardware, then finish the operator documentation and release checklist.

## Checklist

### End-to-end tests

- [ ] Mock Home Assistant covers config → connect → snapshot → UI state.
- [ ] Offline scenario: server stops, cached data appears with a stale badge; server returns, state resyncs.
- [ ] Invalid token shows an error and does not loop.
- [ ] Failed service call shows an error and does not fake success.
- [ ] Malformed messages and unknown domains do not crash the app.

### Performance and stability

- [ ] Measure RSS, first-frame time, and idle CPU on the Pi Zero 2 W; record method and numbers.
- [ ] Use `/usr/bin/time -v` or systemd memory accounting on the Pi; use `heaptrack` on x86_64.
- [ ] Verify RSS < 150 MB (target < 100 MB), first frame < 3 s, acceptable idle CPU.
- [ ] Run a 24-hour soak with simulated state changes; check for leaks and unbounded caches.
- [ ] Tune release profile (`opt-level = "s"`, LTO, `strip`, `panic = "abort"`) and re-measure.

### Documentation and release

- [ ] README: features, hardware, install, config reference, troubleshooting, security, license status.
- [ ] Commented `config.example.toml`; no real credentials or identifiers.
- [ ] Troubleshooting: cross-compile errors, glibc mismatch, no display backend, touch rotation, token permissions, HA version.
- [ ] Release checklist: tests, clippy, fmt, cross artifacts, checksums, changelog, known limitations.

## Acceptance gate

- All automated checks pass.
- Performance numbers are recorded and within budget.
- The 24-hour soak shows no leak trend.
- Documentation matches the shipped behavior.
- The license decision is recorded before any binary is distributed.

## Out of scope

- New widgets or provider integrations.
- Visual redesign.

## Notes

- A green test suite is not evidence for the Pi memory or display path; those need the recorded hardware test.
- Do not distribute binaries while the Slint license decision is open.
