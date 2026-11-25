# Performance budgets and Pi Zero 2 W record

**Status: not verified on hardware.** No Raspberry Pi Zero 2 W measurement is
recorded yet. The Wave 5 performance gate stays open until a device record is
filled in below and accepted by [`scripts/perf-budget.py`](../scripts/perf-budget.py).
Tracking issues: #161 (this record) and the parent epic #4.

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
systemctl show ha-kiosk -p MemoryCurrent -p MemoryPeak | tee /tmp/ha-kiosk-rss.txt
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
sudo LC_ALL=C timeout 30 strace -f -ttt -e trace=ioctl -o /tmp/ha-kiosk-frames.log \
  /usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml
```

The collector in `scripts/perf-record.py` reads this log and computes the
elapsed time from the successful `execve` to the first `DRM_IOCTL_MODE_PAGE_FLIP`
or `DRM_IOCTL_MODE_ATOMIC` presentation call. The trace uses epoch timestamps
(`-ttt`) so the calculation is safe across midnight.
Do not measure the time until the Home Assistant connection is established:
the first frame must come from cached data, and the connection is asynchronous
by design.

### Idle CPU

With the service running, no page interaction, and a quiet Home Assistant
instance, sample one-minute averages for ten minutes:

```bash
sudo apt install sysstat
pid=$(systemctl show -p MainPID --value ha-kiosk)
LC_ALL=C pidstat -u -p "$pid" 60 10 | tee /tmp/ha-kiosk-cpu.log
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
| Pi Zero 2 W RSS, first frame, idle CPU, soak | this record | not verified on hardware |

## Building the record with the collector

The measurements above are saved as raw evidence. The collector turns that
evidence into a schema-v1 record without inventing or transcribing numbers:

```bash
python3 scripts/perf-record.py \\
  --base-record /tmp/ha-kiosk-soak-record.json \\
  --rss /tmp/ha-kiosk-rss.txt \\
  --first-frame /tmp/ha-kiosk-frames.log \\
  --idle-cpu /tmp/ha-kiosk-cpu.log \\
  --os /etc/os-release \\
  --device /proc/device-tree/model \\
  --recorded-at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \\
  --notes "First frame from the first DRM page flip." \\
  --output docs/performance/2026-11-24-raspberry-pi-zero-2-w.json
```

`--base-record` is a schema-v1 record that already carries the soak metrics
(`soak_hours`, `soak_rss_growth_mb`, `throttled`) from the 24-hour run. The
collector replaces only `rss_idle_mb`, `first_frame_ms`, and `idle_cpu_percent`,
and preserves the soak values. It rejects malformed evidence, a non-Pi-Zero-2-W
device model, and an invalid base record before writing the output file.

The collector does not fabricate a Pi Zero 2 W result. Until it is run on that
hardware, the record stays absent and the Wave 5 performance gate remains open.

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

The values above are a format example, not a measurement. An accepted record is
committed as `docs/performance/<date>-<device>.json` and this table is updated
with the numbers and the checker result. Do not copy the example values into
the record.

## Open gate items

The Pi Zero 2 W parts of the Wave 5 gate stay open until the record is filled
in. The subtask issues are #109 (measure), #110 (method), #111 (budgets), #112
(soak), and #113 (release profile); this document records their result.
