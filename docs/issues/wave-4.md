# Wave 4 — Kiosk and deployment: release builds, display, systemd

**Spec:** [PLAN.md](https://github.com/ABOYrbx/home-assistant-kiosk/blob/main/PLAN.md), section 8, Wave 4.
**Depends on:** Wave 3.
**Blocks:** Wave 5.
**Labels:** `implementation`, `wave-4`.

## Context

Turn the working desktop application into a kiosk installation: reproducible release binaries, a LinuxKMS display path with an X11 fallback, a systemd service, and install/uninstall scripts.

## Checklist

### Release builds and cross-compilation

- [ ] Finalize `Cross.toml` with pinned image digests and required `pre-build` packages.
- [ ] `scripts/build-all.sh` builds aarch64, armv7, and x86_64 with `--no-default-features --features kiosk`.
- [ ] Name artifacts with version and target; generate SHA-256 checksums in `dist/`.
- [ ] Document the glibc 2.35+ requirement and Bookworm-only cross-built binaries.
- [ ] Document the native-build fallback on the Pi.

### Kiosk mode

- [ ] `src/kiosk.rs`: fullscreen, rotation, platform detection, clear error when no display backend is available.
- [ ] LinuxKMS path: `SLINT_BACKEND=linuxkms-software`, `SLINT_KMS_ROTATION`, libinput/libseat. Document `SLINT_DRM_OUTPUT` and `SLINT_DRM_MODE`.
- [ ] X11 fallback: winit build, a minimal window manager (openbox/matchbox), `unclutter`, `xset s off -dpms`. Without a WM, fullscreen may not cover the screen.
- [ ] Install `fonts-dejavu-core` or bundle a licensed font; use a licensed icon set and record attribution.
- [ ] Ensure NTP (`systemd-timesyncd` or chrony) and the device timezone are configured.
- [ ] Document touch calibration and rotation for common 7-inch panels; do not hardcode a calibration matrix.
- [ ] Disable console blanking for LinuxKMS (`consoleblank=0`) and document it.

### systemd and install

- [ ] `deploy/ha-kiosk.service`: restart on failure, start after the seat/graphical target or after `seatd`, run as `ha-kiosk`, never as root. Use `RestartSec=5` and sane start limits. Set `Environment=RUST_LOG=info`, `SLINT_BACKEND=linuxkms-software`, and `LIBSEAT_BACKEND=seatd` on the default KMS path. Add `NoNewPrivileges=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/ha-kiosk`; relax to `ProtectSystem=full` if seat socket access fails.
- [ ] Default seat model: **seatd + system service**, service user in the `seat` group. Document the systemd-user-session and `PAMName=login` alternatives.
- [ ] `deploy/kiosk-setup.sh`: install/enable seatd, console blanking off, rotation, reboot notice.
- [ ] Grant only required groups (`video`, `input`, `render`); prefer libseat/seatd over root.
- [ ] `deploy/install.sh`: create user, install binary, create `/var/lib/ha-kiosk` (`0700`, owned by the service user), install config with `0600`, install runtime packages (`libseat1`, `libinput10`, `libudev1`, `libxkbcommon0`, `libfontconfig1`, `fonts-dejavu-core`, `ca-certificates`, `seatd` on the KMS path), install unit, enable and start.
- [ ] `deploy/uninstall.sh`: stop/disable/remove; keep config unless `--purge`.
- [ ] README: install, upgrade, logs (`journalctl -u ha-kiosk`), display backends, rollback.

## Acceptance gate

- A release binary runs on a Pi Zero 2 W and a Pi 3 with Bookworm.
- The service starts after reboot and restarts after a failure.
- No cursor and no display blanking on the kiosk; the X11 fallback uses a minimal WM.
- Fonts and icon licenses are documented.
- Stopping Home Assistant produces the offline badge.
- Logs contain no token.
- If hardware is unavailable, the issue records “not verified on hardware” and lists what was tested instead.

## Out of scope

- Performance tuning and 24-hour soak (Wave 5).
- Final operator documentation polish (Wave 5).
- Wayland as a first-class release gate.

## Notes

- LinuxKMS is marked experimental by Slint; test the actual panel.
- A system service cannot open X11 or DRM without the right session/device permissions. Running as root is not an acceptable workaround.
