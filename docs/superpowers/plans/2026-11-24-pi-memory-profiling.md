# Pi Memory Profiling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide repeatable, tested tooling for Pi peak-RSS/cgroup memory evidence and x86_64 allocation-hotspot evidence without claiming unavailable hardware measurements.

**Architecture:** A Python CLI parses GNU `time -v` and `systemctl show` output into JSON evidence. A small Bash runner builds a symbol-enabled desktop release in a separate target directory, runs `heaptrack`, and writes a readable `heaptrack_print` summary. Documentation links both workflows to the existing performance record and budget checker.

**Tech Stack:** Python 3 standard library, Bash, GNU time, systemd, heaptrack, Cargo.

**Spec:** `PLAN.md` section 8, Wave 5, task 5.2; GitHub issue #110.

## Global Constraints

- English code, comments, tests, documentation, and commit messages.
- No new runtime dependency, configuration behavior, secret, personal identifier, or provider-specific value.
- Debug/profiling output is development evidence, never a release measurement.
- GNU time reports peak RSS, while systemd reports cgroup memory; preserve that distinction.
- Do not fabricate Pi or x86_64 measurements. Record unavailable tooling as not verified.
- Keep the Wave 5 hardware gate open until real device evidence exists.

## Review Focus

- GNU time output with a missing RSS field must fail rather than record zero.
- A nonzero target exit must retain the measurement and propagate the target status.
- Missing or unavailable systemd memory properties must fail clearly.
- `heaptrack` must refuse non-Linux/non-x86_64 hosts before building.
- Profiling artifacts must stay outside the shipped release target and the repository status.

---

### Task 1: Pi memory evidence CLI

**Files:**
- Create: `scripts/perf-memory.py`
- Test: `tests/test_perf_memory.py`
- Modify: `docs/performance.md`

**Interfaces:**
- Consumes: GNU `/usr/bin/time -v` report text and `systemctl show` property text.
- Produces: JSON evidence files containing method, timestamp, command/unit, exit information where applicable, and memory values in bytes/KiB and MiB.

- [ ] **Step 1: Write failing parser and CLI tests**
  - `parse_time_report` extracts `Maximum resident set size (kbytes)` and `Exit status`, converts KiB to MiB, and rejects missing RSS.
  - `parse_systemd_show` extracts `MainPID`, `MemoryCurrent`, and `MemoryPeak`, converts bytes to MiB, and rejects unavailable values.
  - CLI `time --output PATH -- COMMAND` invokes `/usr/bin/time -v`, writes JSON plus the raw report, and returns the target exit status.
  - CLI `systemd --unit ha-kiosk --output PATH` invokes `systemctl show`, writes JSON, and fails on invalid properties.

- [ ] **Step 2: Run the new tests and verify the expected missing-module failure**

Run: `python3 -m unittest tests.test_perf_memory -v`
Expected: FAIL because `scripts/perf-memory.py` does not exist.

- [ ] **Step 3: Implement the minimal CLI and parsers**

Use only the Python standard library. Force `LC_ALL=C` for GNU time, create output parents, write JSON atomically, and keep raw GNU time output beside the JSON.

- [ ] **Step 4: Run the new tests and verify they pass**

Run: `python3 -m unittest tests.test_perf_memory -v`
Expected: all new tests pass.

- [ ] **Step 5: Commit**

```bash
git add scripts/perf-memory.py tests/test_perf_memory.py docs/performance.md
git commit -m "perf: add Pi memory evidence tooling"
```

### Task 2: x86_64 heaptrack runner and operator workflow

**Files:**
- Create: `scripts/perf-heaptrack.sh`
- Test: `tests/test_perf_heaptrack.py`
- Modify: `docs/performance.md`
- Modify: `docs/troubleshooting.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: Linux x86_64 host with Cargo, `heaptrack`, and `heaptrack_print`; optional kiosk arguments after `--`.
- Produces: a separate symbol-enabled profiling build, compressed heaptrack data, a text summary, and console output identifying the artifacts.

- [ ] **Step 1: Write failing runner tests**
  - Stub `uname`, Cargo, `heaptrack`, and `heaptrack_print` on `PATH`.
  - Assert Linux x86_64 proceeds with `--locked --release --features desktop`, `CARGO_PROFILE_RELEASE_STRIP=false`, `CARGO_PROFILE_RELEASE_DEBUG=1`, and a profiling target directory.
  - Assert kiosk arguments are forwarded and the generated report is summarized.
  - Assert macOS and non-x86_64 hosts fail before Cargo runs.
  - Assert missing `heaptrack` fails before Cargo runs.

- [ ] **Step 2: Run the new tests and verify the expected missing-script failure**

Run: `python3 -m unittest tests.test_perf_heaptrack -v`
Expected: FAIL because `scripts/perf-heaptrack.sh` does not exist.

- [ ] **Step 3: Implement the guarded runner**

Keep the default build under `target/heaptrack`, create a unique run directory, preserve a nonzero profiled-process status while still summarizing an available report, and print artifact paths.

- [ ] **Step 4: Run both profiling test modules and verify they pass**

Run: `python3 -m unittest tests.test_perf_memory tests.test_perf_heaptrack -v`
Expected: all profiling tests pass.

- [ ] **Step 5: Document commands, interpretation, and verification limits**

Document exact commands, peak-versus-idle semantics, artifact locations, how to quit the kiosk cleanly so heaptrack flushes, and that this host has no Pi, Linux x86_64, or heaptrack available. Do not add example measurement values to the record.

- [ ] **Step 6: Run documentation checks and commit**

```bash
python3 scripts/check-docs.py
python3 -m unittest discover -s tests -p 'test_*.py' -v
git add scripts/perf-heaptrack.sh tests/test_perf_heaptrack.py docs/performance.md docs/troubleshooting.md README.md
git commit -m "perf: add x86_64 allocation profiling workflow"
```
