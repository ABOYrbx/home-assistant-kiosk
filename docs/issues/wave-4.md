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
- [ ] LinuxKMS path: `SLINT_BACKEND=linuxkms-software`, `SLINT_KMS_ROTATION`, libinput/libseat.
- [ ] X11 fallback: winit build, cursor hiding with `unclutter` or equivalent, blanking off (`xset s off -dpms`).
- [ ] Document touch calibration and rotation for common 7-inch panels; do not hardcode a calibration matrix.
- [ ] Disable console blanking for LinuxKMS (`consoleblank=0`) and document it.

### systemd and install

- [ ] `deploy/ha-kiosk.service`: restart on failure, start after the seat/graphical target or after `seatd`, run as `ha-kiosk`, never as root. Add baseline hardening (`NoNewPrivileges=yes`, `ProtectSystem=strict`, `ReadWritePaths=/var/lib/ha-kiosk`).
- [ ] Choose and document the seat model: systemd user service in an autologin session, system service with `PAMName=login` and a TTY, or `seatd` with the service user in the `seat` group. Test it.
- [ ] `deploy/kiosk-setup.sh`: console/autologin setup, console blanking off, rotation, reboot notice.
- [ ] Grant only the required groups (`video`, `input`, `render` as needed); prefer libseat/logind or seatd over root.
- [ ] `deploy/install.sh`: create user, install binary, create `/var/lib/ha-kiosk` owned by the service user, install config with `0600`, install unit, enable and start.
- [ ] `deploy/uninstall.sh`: stop/disable/remove; keep config unless `--purge`.
- [ ] README: install, upgrade, logs (`journalctl -u ha-kiosk`), display backends, rollback.

## Acceptance gate

- A release binary runs on a Pi Zero 2 W and a Pi 3 with Bookworm.
- The service starts after reboot.
- No cursor and no display blanking on the kiosk.
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
