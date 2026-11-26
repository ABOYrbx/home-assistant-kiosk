# Host soak evidence (issue #112, Wave 5.2.4)

This record documents a host-side run of the simulated state-change soak in
`tests/e2e_test.rs::soak_with_state_change_stream_stays_bounded`. It is host
evidence only; the 24-hour Pi run required for the Wave 5 gate is still
outstanding and is not claimed here.

## Environment

- Host: macOS development machine (Apple Silicon), debug profile.
- Rust toolchain: 1.95 (edition 2024).
- Duration selected with `HA_KIOSK_SOAK_SECONDS`.

## Command

```bash
HA_KIOSK_SOAK_SECONDS=2 cargo test --test e2e_test \
  soak_with_state_change_stream_stays_bounded -- --ignored --nocapture
```

## Observed output

```text
soak: 2s, 112 events, 0 cache writes, 1520 cache bytes, RSS baseline None MiB,
final None MiB, growth None MiB, slope None MiB/h, projected24h None MiB,
max_entities 5, max_pending 0, max_cache_bytes 0, max_image_entries 57,
max_image_bytes 228, rss_samples 0
test soak_with_state_change_stream_stays_bounded ... ok
```

## Interpretation

- `rss_mb()` reads `/proc/self/status`, which is unavailable on macOS, so RSS
  is reported as `None` and no memory pass is claimed. This is the intended,
  explicit behaviour of `SoakMonitor`.
- The soak processed 112 state-change events in 2 seconds with one mock
  connection.
- The entity map stayed bounded (at most 5 entries against a 3-entity baseline
  plus one transient entry, the extra count coming from in-flight add/remove
  interleaving).
- The pending-action map stayed empty.
- The file cache stayed at 1520 bytes, within the 256 KiB cap; no cache write
  occurred because the 2-second run is shorter than the 10-second production
  debounce.
- The album-art LRU stayed within its 1 KiB budget (228 bytes, 57 entries).

## Release-profile run

A 60-second release-profile run was attempted with
`HA_KIOSK_SOAK_SECONDS=60 cargo test --release ...`; the build did not finish
within the available session time on this host. The debug run above is the
recorded host evidence for this change. A release-mode run should be recorded
before the Wave 5 gate is closed.

## Pi run

Not run. `soak_hours` and `soak_rss_growth_mb` remain unmeasured. The 24-hour
Pi procedure is documented in `docs/performance.md`.
