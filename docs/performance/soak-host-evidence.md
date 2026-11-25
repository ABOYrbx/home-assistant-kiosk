# Host Soak Evidence

**Status: host evidence only; the 24-hour Pi soak is not verified on hardware.**

This record documents a short host run of the simulated state-change soak from
issue #112. It proves that the harness runs, that transient entities and
service calls are cleaned up, and that the cache and album-art counters stay
bounded. It does not prove the Pi Zero 2 W memory or 24-hour leak gate; the Pi
record in [`../performance.md`](../performance.md) stays open.

## What was run

```bash
HA_KIOSK_SOAK_SECONDS=60 cargo test --release --test e2e_test \
  soak_with_state_change_stream_stays_bounded -- --ignored --nocapture
```

- Host: macOS development machine (not the target device).
- Build: `release` test profile.
- Stream: simulated Home Assistant snapshot, per-second state changes, a
  transient entity that is added and removed, service calls every five
  seconds, and a unique album-art URL every second.
- The monitor samples once per second and excludes the first five seconds from
  the RSS baseline.

## Result

```text
running 1 test
test soak_with_state_change_stream_stays_bounded has been running for over 60 seconds
soak: 60s, 1256 events, 11 service calls, 62 cache writes, 1116 cache bytes, RSS baseline None MiB, final None MiB, slope None MiB/h, projected 24h growth None MiB, max entities 4, max pending 0, max cache 1357 bytes, max album-art 16/65536 entries/bytes
soak: RSS is unavailable on this host; memory is not claimed as verified
test soak_with_state_change_stream_stays_bounded ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 12 filtered out; finished in 60.06s
```

## Limits of this evidence

- RSS is read from `/proc/self/status`, which does not exist on macOS. The run
  reports RSS as unavailable and does not claim a memory pass.
- The host check uses a 16 MiB observed and projected growth limit. The Pi gate
  remains 10 MB and must be measured on the device.
- The 24-hour duration is not exercised here. A production gate run uses
  `HA_KIOSK_SOAK_SECONDS=86400` on the Pi with the service under the production
  cgroup, and fills in `soak_hours`, `soak_rss_growth_mb`, and `throttled` in
  the record table.
