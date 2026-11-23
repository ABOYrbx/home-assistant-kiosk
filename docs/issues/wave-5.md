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
- [ ] Add `cargo audit` (and optionally `cargo deny check licenses`) to CI; review findings before release.

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

---

## Subtask issues

The wave issue is the epic. Every checkbox above, including the commit step, is a separate issue:

### 5.1 End-to-end tests

- [ ] #103 — [W5.1.1] Mock Home Assistant server covering config → connect → snapshot → UI state.
- [ ] #104 — [W5.1.2] Offline scenario: server stops, cached data appears with a stale badge; server returns,...
- [ ] #105 — [W5.1.3] Authentication failure: invalid token shows an error and does not loop.
- [ ] #106 — [W5.1.4] Service failure: tile shows an error and does not fake success.
- [ ] #107 — [W5.1.5] Malformed messages and unknown domains do not crash the app.
- [ ] #108 — [W5.1.6] Add cargo audit (and optionally cargo deny check licenses) to CI; review findings befor...
- [ ] #160 — [W5.1.7] Commit: test: end-to-end and failure scenarios.

### 5.2 Performance and stability

- [ ] #109 — [W5.2.1] Measure RSS, first-frame time, and idle CPU on the Pi Zero 2 W; record the method and n...
- [ ] #110 — [W5.2.2] Use /usr/bin/time -v or systemd memory accounting on the Pi; use heaptrack on x86_64 fo...
- [ ] #111 — [W5.2.3] Verify RSS < 150 MB (target < 100 MB), first frame < 3 s, idle CPU low enough that the...
- [ ] #112 — [W5.2.4] Run a 24-hour soak with a simulated state-change stream; check for leaks and unbounded...
- [ ] #113 — [W5.2.5] Tune release profile (opt-level = "s", LTO, strip, panic = "abort") and confirm behavior.
- [ ] #161 — [W5.2.6] Commit: perf: verify Pi Zero 2 W budgets.

### 5.3 Documentation and release

- [ ] #114 — [W5.3.1] README: features, supported hardware, install, config reference, troubleshooting, secur...
- [ ] #115 — [W5.3.2] Commented config.example.toml; no real credentials or personal identifiers.
- [ ] #116 — [W5.3.3] Troubleshooting: cross-compile errors, glibc mismatch, no display backend, touch rotati...
- [ ] #117 — [W5.3.4] Release checklist: tests, clippy, fmt, cross artifacts, checksums, changelog, known lim...
- [ ] #162 — [W5.3.5] Commit: docs: complete operator documentation.
