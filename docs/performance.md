# Performance budgets and Pi Zero 2 W record

**Status: not verified on hardware.** No Raspberry Pi Zero 2 W measurement is
recorded yet. On 2026-11-25 the only reachable SBC was a Pi 3 Model B+ and it
cannot run the kiosk: the HDMI connector reports `disconnected`, there is no X11
or Wayland session, its distro Rust is 1.85 (the workspace requires 1.95), and
`vcgencmd get_throttled` reports `0xd0000` from earlier boot conditions. A
substitute device is not evidence for this gate, so the Wave 5 performance gate
stays open until a device record is filled in below and accepted by
[`scripts/perf-budget.py`](../scripts/perf-budget.py).
Tracking issues: #109 and #110 (measurement and method), #161 (this record), and
the parent epic #4.

This file is the written test record for the performance part of the Wave 5
gate. It defines the budgets, the measurement method, the soak procedure, and
the release profile. [`PLAN.md`](../PLAN.md) section 3 and section 8 remain the
source of truth; this file records how the numbers are produced.

## Budgets

| Metric | Record key | Hard gate | Target | Source |
| --- | --- | --- | --- | --- |
| Resident set size at idle with cached data | `rss_idle_mb` | below 150 MB | below 100 MB | PLAN.md section 3 |
| Time to first frame | `first_frame_ms` | below 3000 ms | none | PLAN.md section 3 |
| Idle CPU, ten-minute mean | `idle_cpu_percent` | at most 25 % of one core | none | PLAN.md section 8 |
| Soak duration | `soak_hours` | at least 24 h | none | PLAN.md section 8 |
| RSS growth over the soak | `soak_rss_growth_mb` | at most 10 MB | none | leak check |
| Firmware throttle flags after the soak | `throttled` | false | none | `vcgencmd get_throttled` |

The 25 % idle CPU limit is a working limit, not a product promise. The design
has no continuous animation: the clock updates once per second and the
software renderer redraws only when state changes. A sustained value above one
quarter of a core on a passively cooled Pi Zero 2 W leads to thermal
throttling, which the last row catches. The plan asks for an idle CPU "low
enough that the device does not throttle" and does not name a percentage, so
this record fixes the percentage; a higher measurement fails the check and must
be investigated or the limit revisited with the owner.

The plan asks for a leak check but does not set a numeric limit. This record
uses 10 MB of RSS growth over the soak as that limit.

A record that stays under 150 MB but misses the 100 MB target passes the check
with a warning. The warning is carried into the release checklist instead of
being silently accepted.

## Measurement method

Measure the release binary built for the `kiosk` feature set. Debug builds are
not a release gate and the checker rejects them.

```bash
cross build --release --target aarch64-unknown-linux-gnu \
  --no-default-features --features kiosk
```

On the device itself, the equivalent native build is
`cargo build --release --no-default-features --features kiosk`.

Install the binary and the systemd unit with [`deploy/install.sh`](../deploy/install.sh)
so that the service runs as the `ha-kiosk` user with the same cgroup settings
as production. Measure RSS and idle CPU from the service; an interactive shell
has different accounting and a different environment. The first-frame trace
below needs a fresh process start and the DRM device for itself, so stop the
service for that measurement and start it again afterwards.

### Resident set size

Wait ten minutes after the first frame so that startup allocations, the cache
load, and the first Home Assistant snapshot have settled. Then read the service
cgroup:

```bash
systemctl show ha-kiosk -p MemoryCurrent -p MemoryPeak
```

Both values are bytes; divide by 1048576 for MB. `MemoryCurrent` is the value
for `rss_idle_mb`. `MemoryPeak` requires cgroup v2 with systemd 248 or newer,
which Raspberry Pi OS Bookworm provides. On a Pi 3 with the legacy cgroup
hierarchy, use the cross-check below instead.

Cross-check with GNU time, which reports the peak resident set at exit:

```bash
/usr/bin/time -v /usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml
```

The `Maximum resident set size (kbytes)` line is a peak, not an idle value. It
must not be lower than `MemoryCurrent`; if it is, the service was not idle long
enough or the measurement ran with a different configuration.

### First frame

Record the wall-clock time from process start to the first presented frame. On
the LinuxKMS software backend the first presented frame follows the first
presentation ioctl. Trace ioctls with absolute timestamps:

```bash
sudo timeout 30 strace -f -tt -e trace=ioctl -o /tmp/ha-kiosk-frames.log \
  /usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml
```

```bash
python3 - <<'PY'
import re
from datetime import datetime

stamp = "%H:%M:%S.%f"
pattern = re.compile(r"(\d{2}:\d{2}:\d{2}\.\d+)\s+ioctl\(")
lines = [line for line in open("/tmp/ha-kiosk-frames.log") if pattern.search(line)]
start = datetime.strptime(pattern.search(lines[0]).group(1), stamp)
present = next(
    datetime.strptime(pattern.search(line).group(1), stamp)
    for line in lines
    if "DRM_IOCTL_MODE_PAGE_FLIP" in line or "DRM_IOCTL_MODE_ATOMIC" in line
)
print(f"{(present - start).total_seconds() * 1000:.0f} ms")
PY
```

The snippet assumes `-f`, so each syscall line carries a thread ID before the
timestamp. If a future backend presents frames through a different call, record
the first presentation call in `notes` and use that timestamp. Do not measure
the time until the Home Assistant connection is established: the first frame
must come from cached data, and the connection is asynchronous by design.

### Idle CPU

With the service running, no page interaction, and a quiet Home Assistant
instance, sample one-minute averages for ten minutes:

```bash
sudo apt install sysstat
pid=$(systemctl show -p MainPID --value ha-kiosk)
pidstat -u -p "$pid" 60 10
```

Record the mean of the `%CPU` column in `idle_cpu_percent`. `pidstat` reports
a percentage of one core, which matches the budget. Do not measure while the
cache is being written or a forecast refresh is in flight; both are periodic
and would inflate the number.

### 24-hour soak

Run the service for 24 hours against the mock Home Assistant server from Wave
5.1 with a simulated state-change stream. A stream that touches the configured
entities every second is enough to exercise the coalescing path, the image
cache, and the cache writer.

The mock server and the state-change stream are part of
[`tests/e2e_test.rs`](../tests/e2e_test.rs). A shorter host-side leak check runs
the same client, state owner, and cache for a configurable duration and fails
when the entity map or the cache grows without bound:

```bash
HA_KIOSK_SOAK_SECONDS=3600 cargo test --test e2e_test \
  soak_with_state_change_stream_stays_bounded -- --ignored --nocapture
```

It prints the event count, cache writes, cache bytes, and the RSS baseline and
final value. This check measures the test process on the development host, not
the Pi; the 24-hour Pi run below is still required for the gate.

```bash
# Hourly samples; keep the output for the record.
while true; do
  date -u +%FT%TZ
  systemctl show ha-kiosk -p MemoryCurrent -p MemoryPeak
  du -sb /var/lib/ha-kiosk
  sleep 3600
done | tee /tmp/ha-kiosk-soak.log
```

At the end of the soak:

- `soak_hours` is the measured duration.
- `soak_rss_growth_mb` is the last hourly `MemoryCurrent` minus the sample at
  the one-hour mark, so that warm-up allocations are excluded. A leak shows as
  a monotonic rise, not as a single step.
- The cache size must stay bounded. It may grow when new entity states are
  cached, but it must not grow every hour. A rising cache size means an
  unbounded cache, which is a failure even when RSS is still under budget.
- `vcgencmd get_throttled` must print `throttled=0x0`. Any other value sets
  `throttled` to true, including the "has occurred since boot" bits. Reboot
  before the soak so that stale flags from earlier work do not count.

### Allocation hotspots

`heaptrack` is an x86_64 development aid, not a Pi gate. Build a profiling copy
with symbols, because the release profile strips them:

```bash
CARGO_PROFILE_RELEASE_STRIP=false cargo build --release --features desktop
heaptrack ./target/release/ha-kiosk --config config.example.toml
heaptrack_print heaptrack.ha-kiosk.*.zst | head -n 40
```

Look for allocations that scale with the number of entity updates: album art
buffers, decoded images, and JSON values from large snapshots. A hotspot that
grows with uptime is a soak failure even if the 24-hour run stays under
150 MB.

The guarded runner builds a symbol-enabled copy, runs it under `heaptrack`, and
keeps both the compressed report and a text summary under
`target/heaptrack/runs/<timestamp>-*`:

```bash
scripts/perf-heaptrack.sh -- --config config.example.toml
```

It refuses a non-Linux or non-x86_64 host and a missing `heaptrack` before it
compiles anything. Quit the kiosk normally after reproducing the workload so
heaptrack flushes its report.

## Release profile

The release profile in [`Cargo.toml`](../Cargo.toml) is part of the performance
contract. [`scripts/check-docs.py`](../scripts/check-docs.py) fails when it
drifts.

| Setting | Value | Effect |
| --- | --- | --- |
| `opt-level` | `"s"` | Optimise for size; the Pi Zero 2 W is memory-bound before it is compute-bound. |
| `lto` | `true` | Cross-crate inlining and dead-code removal; longer builds. |
| `strip` | `true` | Remove symbols from the shipped binary; use the profiling build above for analysis. |
| `panic` | `"abort"` | No unwinding tables; a panic terminates the process and systemd restarts it (`Restart=always`). |

Measure the same profile that ships. Changing a profile setting invalidates the
record and requires a new one.

## Record

| Field | Value |
| --- | --- |
| Status | not verified on hardware |
| Device | none recorded |
| OS | none recorded |
| Build | release, `kiosk` feature set |
| Recorded at | none recorded |
| `rss_idle_mb` | not measured |
| `first_frame_ms` | not measured |
| `idle_cpu_percent` | not measured |
| `soak_hours` | not run |
| `soak_rss_growth_mb` | not measured |
| `throttled` | not measured |
| Checker result | not run |

### What is verified without a device

| Item | Evidence | Status |
| --- | --- | --- |
| Budget thresholds are defined and enforced | `scripts/perf-budget.py` and its unit tests | verified in CI |
| Release profile matches PLAN.md | `scripts/check-docs.py` | verified in CI |
| Soak leak check runs against the real client, state, and cache | `tests/e2e_test.rs` ignored soak test | verified on the development host |
| Raw evidence becomes a validated record | `scripts/perf-record.py` and `tests/test_perf_record.py` | verified by unit tests |
| Memory evidence capture is method-separated | `scripts/perf-memory.py` and `tests/test_perf_memory.py` | verified by unit tests |
| x86_64 allocation profiling is guarded and reproducible | `scripts/perf-heaptrack.sh` and `tests/test_perf_heaptrack.py` | verified with external-tool stubs |
| Pi Zero 2 W RSS, first frame, idle CPU, soak | this record | not verified on hardware |

### Pi 3 Model B+ check (not the release gate)

On 2026-11-25 the only reachable single-board computer was a Pi 3 Model B+
running Debian 13 (trixie), kernel 6.18, 905 MB RAM. It cannot produce any of the
four measurements:

```text
$ cat /proc/device-tree/model
Raspberry Pi 3 Model B Plus Rev 1.3
$ cat /sys/class/drm/card0-HDMI-A-1/status
disconnected
$ systemctl list-units --type=service --state=running | grep -Ei 'x11|wayland|kiosk'
(no output)
$ rustc --version
rustc 1.85.1
$ vcgencmd get_throttled
throttled=0xd0000
```

No connector means no DRM page flip, so there is no first frame to time. No X11
or Wayland session means the `desktop` fallback cannot start either. The distro
Rust is older than the workspace's `rust-version = 1.95`, so a native build is
not possible without installing a newer toolchain. The throttling bits record
earlier boot conditions, so they would fail the `throttled` gate even after a
run.

A Pi 3 is a supported target in its own right, but it is not evidence for the Pi
Zero 2 W gate: its CPU is faster and its memory is larger, so a number from it
would flatter the budget. The record stays unfilled and the gate stays open.

## Recording and checking a record

Write the measurement to JSON. The shape is validated, so unknown fields and
unknown metrics fail instead of being ignored:

```json
{
  "schema_version": 1,
  "device": "raspberry-pi-zero-2-w",
  "os": "Raspberry Pi OS Bookworm 64-bit",
  "build": "release",
  "features": "kiosk",
  "recorded_at": "2026-11-24T10:00:00Z",
  "notes": "First frame from the first DRM page flip.",
  "metrics": {
    "rss_idle_mb": 78.4,
    "first_frame_ms": 1820,
    "idle_cpu_percent": 6.2,
    "soak_hours": 24.0,
    "soak_rss_growth_mb": 2.7,
    "throttled": false
  }
}
```

```bash
python3 scripts/perf-budget.py perf-record.json
```

The command prints a table, exits 0 when every hard gate is met (warnings for a
missed target), exits 1 when a gate is breached or the record is malformed, and
exits 2 when the file cannot be read or is not JSON.

### Build the record from raw evidence

`scripts/perf-record.py` reads the evidence saved by the commands above, so a
number is never copied by hand into the record. It keeps the soak metrics from a
base record and replaces the three metrics measured here:

```bash
python3 scripts/perf-record.py \
  --base-record docs/performance/2026-11-25-soak-base.json \
  --rss /tmp/ha-kiosk-systemd.txt \
  --first-frame /tmp/ha-kiosk-frames.log \
  --idle-cpu /tmp/ha-kiosk-pidstat.txt \
  --os /etc/os-release \
  --device /proc/device-tree/model \
  --output docs/performance/2026-11-25-raspberry-pi-zero-2-w.json
```

The collector refuses a base record that is not `release` with the `kiosk`
feature set, fewer than ten one-minute `pidstat` samples, a missing process
start or DRM presentation in the strace, and any device model that is not a
Raspberry Pi Zero 2 W. It validates the result with `perf-budget.py` and writes
nothing when validation fails.

Capture the raw evidence with `scripts/perf-memory.py` for the memory value:

```bash
python3 scripts/perf-memory.py systemd --unit ha-kiosk \
  --output /tmp/ha-kiosk-memory.json

python3 scripts/perf-memory.py time --output /tmp/ha-kiosk-time.json -- \
  /usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml
```

`systemd` reads `MemoryCurrent`, `MemoryPeak`, and `MainPID` from the service
cgroup; a stopped service or an unavailable `MemoryCurrent` is an error, not a
zero. `time` runs the command under `/usr/bin/time -v` and reports the peak
resident set at exit. Keep the two methods separate: a peak process RSS is not
an idle cgroup value.

The values above are a format example, not a measurement. An accepted record is
committed as `docs/performance/<date>-<device>.json` and this table is updated
with the numbers and the checker result. Do not copy the example values into
the record.

## Open gate items

The Pi Zero 2 W parts of the Wave 5 gate stay open until the record is filled
in. The subtask issues are #109 (measure), #110 (method), #111 (budgets), #112
(soak), and #113 (release profile); this document records their result.
