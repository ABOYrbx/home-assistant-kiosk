# Deployment

This guide covers the default kiosk deployment: a systemd system service that
runs as the `ha-kiosk` user and owns the display through `seatd` and LinuxKMS.
The X11 fallback and the alternative seat models are documented below.

The scripts live in [`deploy/`](../deploy):

| File | Purpose |
| --- | --- |
| [`ha-kiosk.service`](../deploy/ha-kiosk.service) | Default system unit: seatd, LinuxKMS, restart on failure, baseline hardening. |
| [`ha-kiosk-user.service`](../deploy/ha-kiosk-user.service) | Example user unit for the X11 fallback inside an autologin session. |
| [`install.sh`](../deploy/install.sh) | Create the user, install the binary, configuration, token, packages, unit, and start the service. |
| [`kiosk-setup.sh`](../deploy/kiosk-setup.sh) | Enable seatd, grant groups, disable console blanking, set rotation, print the reboot notice. |
| [`uninstall.sh`](../deploy/uninstall.sh) | Stop, disable, and remove the unit and binary. Keeps data unless `--purge` is passed. |
| [`lib.sh`](../deploy/lib.sh) | Shared shell helpers used by the three scripts. |

## Default model: seatd plus a system service

The service runs as `ha-kiosk`, never as root. `seatd` owns the seat and passes
the DRM and input file descriptors to the process. The service user is a member
of `seat` for the seatd socket and of `video`, `render`, and `input` for device
access granted by the kernel or a udev rule. Running as root is not an
acceptable workaround: it bypasses the session model and the hardening in the
unit.

### Prerequisites

- Raspberry Pi OS Bookworm (64-bit for Pi Zero 2 W and Pi 3) or another
  Debian-based system with systemd.
- A release binary built for the device architecture. See
  [`PLAN.md`](../PLAN.md) section 8, Wave 4.1.
- A Home Assistant long-lived access token in a file that only the service user
  can read.
- The device timezone and NTP configured. A Pi without an RTC shows the wrong
  clock after boot otherwise.

### Install

```bash
sudo deploy/install.sh \
  --binary dist/ha-kiosk-<version>-aarch64-unknown-linux-gnu \
  --config config.toml \
  --token-file ha_token

sudo deploy/kiosk-setup.sh --rotation 0
sudo reboot
```

`--binary` accepts any path, for example `target/release/ha-kiosk` for a native
build on the device or a `dist/ha-kiosk-<version>-<target>` release artifact.

`install.sh` performs these steps:

1. Install the runtime packages: `libseat1`, `libinput10`, `libudev1`,
   `libxkbcommon0`, `libfontconfig1`, `fonts-dejavu-core`, `ca-certificates`,
   and `seatd`.
2. Create the `ha-kiosk` system user with `/usr/sbin/nologin`, and add it to
   `seat`, `video`, `render`, and `input`. Missing groups are created.
3. Create `/etc/ha-kiosk` (`0750`) and `/var/lib/ha-kiosk` (`0700`), both owned
   by the service user.
4. Install the binary as `/usr/local/bin/ha-kiosk` (`0755`) and the unit as
   `/etc/systemd/system/ha-kiosk.service` (`0644`).
5. Install the configuration and token with mode `0600`.
6. Install the README and this guide under `/usr/share/doc/ha-kiosk/`.
7. Enable `seatd` and `ha-kiosk`, then start the kiosk.

`kiosk-setup.sh` enables `seatd`, re-applies the group membership, adds
`consoleblank=0` to the kernel command line, writes the rotation drop-in, and
prints a reboot notice. The kernel command line change takes effect only after
a reboot.

Use `--no-start` to install and enable without starting, `--skip-packages` on
an offline device with the packages already present, and `--dry-run` to review
the actions. Use `--force` to replace an existing configuration or token file.

### Configuration and permissions

The unit starts:

```text
/usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml
```

The configuration is mode `0600` and owned by `ha-kiosk`. The token comes from
`HA_TOKEN`, the configured `token_file`, or the development-only literal token.
Prefer a dedicated Home Assistant user with only the entities and actions the
kiosk needs.

The offline cache and other runtime state live in `/var/lib/ha-kiosk`. The unit
sets `StateDirectory=ha-kiosk` with mode `0700` and allows writes only there.

### Rotation and console blanking

- Rotation on LinuxKMS is `SLINT_KMS_ROTATION`, written by
  `kiosk-setup.sh --rotation 90` (or `180`, `270`) into
  `/etc/systemd/system/ha-kiosk.service.d/10-kiosk-rotation.conf`. Restart the
  service after a change; a reboot is not required.
- `kiosk-setup.sh --rotation 0` removes the drop-in.
- Console blanking is disabled with `consoleblank=0` in
  `/boot/firmware/cmdline.txt` (Bookworm) or `/boot/cmdline.txt` (older
  images). Verify with `cat /proc/cmdline` after the reboot.
- Multi-display and explicit mode selection use `SLINT_DRM_OUTPUT` and
  `SLINT_DRM_MODE`. Add them to a drop-in rather than editing the packaged
  unit.

### Display backends

**LinuxKMS (default).** `SLINT_BACKEND=linuxkms-software`,
`LIBSEAT_BACKEND=seatd`. This is the release gate: no X server, no compositor,
no cursor, lowest memory use.

**X11 (fallback).** A system service cannot open X11 without a session, so the
fallback runs as a systemd user service inside an autologin session. Use a
minimal window manager (openbox or matchbox), hide the pointer with
`unclutter`, and disable blanking with `xset s off -dpms`. Without a window
manager, fullscreen may not cover the screen. See the X11 section below.

## Seat model alternatives

The default is seatd plus a system service. Two alternatives are supported but
not the release gate:

1. **Systemd user service inside an autologin session.** The kiosk starts with
   the desktop user's graphical session. This is the X11 fallback model and is
   the least surprising way to give a service access to X11. Use
   [`ha-kiosk-user.service`](../deploy/ha-kiosk-user.service). The config,
   token, and state live under the user's home directory.
2. **System service with `PAMName=login` and a TTY.** PAM opens a login session
   so logind grants a seat. It requires `PAMName=login`, a `TTYPath`, and
   careful ordering with `getty`. It is more fragile than seatd and is not
   installed by these scripts; document any local variation.

Do not replace the seat model with root. Root also needs a DRM master and input
access, and it defeats `NoNewPrivileges` and the file ownership model.

## X11 fallback

`kiosk-setup.sh --backend x11` installs `xserver-xorg`, `xinit`, `openbox`,
`unclutter`, `x11-xserver-utils`, and `fonts-dejavu-core`, and writes
`/etc/ha-kiosk/x11-session.sh`. The script disables X blanking, hides the
pointer, starts openbox, applies the rotation with `xrandr`, and execs the
kiosk.

Start the session directly:

```bash
startx /etc/ha-kiosk/x11-session.sh
```

Put `config.toml` and the token under `~/.config/ha-kiosk/` with mode `0600`,
because `/etc/ha-kiosk` belongs to the `ha-kiosk` service user. Confirm that
openbox is running and that fullscreen covers the panel.

For an autologin graphical session, use the example user unit instead:

```bash
install -D -m 0644 deploy/ha-kiosk-user.service \
  ~/.config/systemd/user/ha-kiosk.service
systemctl --user daemon-reload
systemctl --user enable ha-kiosk.service
```

In that model the graphical session must start openbox and `unclutter`; the
user unit starts only the kiosk. The user unit deliberately does not set
`PrivateTmp=yes`: the X11 socket lives in `/tmp/.X11-unix`, and a private
`/tmp` would hide it.

## Hardening

The default unit sets `NoNewPrivileges=yes`, `ProtectSystem=strict`,
`ReadWritePaths=/var/lib/ha-kiosk`, `PrivateTmp=yes`, `ProtectHome=yes`, and a
restricted set of address families and namespaces. `/etc` is read-only to the
service; it only reads its configuration and token.

`PrivateDevices` must stay off: DRM and input devices are opened through the
seat granted by seatd. `MemoryDenyWriteExecute` is not enabled because the
graphics and font stacks are not guaranteed to avoid writable-executable
mappings on every target.

If a distribution blocks the seatd or logind socket under
`ProtectSystem=strict`, relax the unit to `ProtectSystem=full` in a drop-in and
record the reason here:

```ini
# /etc/systemd/system/ha-kiosk.service.d/20-protect-system.conf
[Service]
ProtectSystem=full
```

Run `systemctl daemon-reload` after adding a drop-in.

## Logs

```bash
journalctl -u ha-kiosk -f
journalctl -u ha-kiosk -b --no-pager
systemctl status ha-kiosk
```

The service logs at `RUST_LOG=info` by default. The token, the authorization
header, and image query strings are never logged. Increase the level with a
drop-in:

```ini
[Service]
Environment=RUST_LOG=debug
```

## Upgrade

Install the new binary without starting it, keep the current one, and restart:

```bash
sudo cp /usr/local/bin/ha-kiosk /usr/local/bin/ha-kiosk.previous
sudo deploy/install.sh --binary dist/ha-kiosk-<new-version>-<target> --no-start
sudo systemctl restart ha-kiosk
journalctl -u ha-kiosk -b --no-pager
```

`install.sh` keeps the existing configuration, token, and cache when
`--config` and `--token-file` are omitted. A configuration fingerprint change
discards the offline cache; a schema mismatch is rejected with a warning.

## Rollback

1. Restore the previous binary without hitting a running executable:
   ```bash
   sudo cp /usr/local/bin/ha-kiosk.previous /usr/local/bin/ha-kiosk.new
   sudo mv /usr/local/bin/ha-kiosk.new /usr/local/bin/ha-kiosk
   sudo systemctl restart ha-kiosk
   ```
2. If the new release changed the configuration, restore the previous
   `config.toml` and restart. Keep a copy outside `/etc/ha-kiosk` before an
   upgrade.
3. A rejected cache rebuilds from the next Home Assistant snapshot; the kiosk
   shows the offline or stale state honestly until then.

## Uninstall

```bash
sudo deploy/uninstall.sh
```

This stops and disables the service, removes the unit, drop-ins, binary, and
installed documentation. The configuration, token, cache, and service user are
kept so a reinstall keeps the setup. `--purge` also removes `/etc/ha-kiosk`,
`/var/lib/ha-kiosk`, and the `ha-kiosk` user. That deletes the token and the
cache.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `Failed to determine supplementary groups` | A group in `SupplementaryGroups` is missing. Re-run `deploy/install.sh` or create the group. |
| `Permission denied` on `/run/seatd.sock` | `seatd` is not running, or the user is not in `seat`. Run `systemctl status seatd` and `id ha-kiosk`. |
| `No display backend` or a blank screen | Check `SLINT_BACKEND`, `/dev/dri/card0`, and the seat. On a multi-output device set `SLINT_DRM_OUTPUT`. |
| Wrong or unsupported mode | Set `SLINT_DRM_MODE` in a drop-in; list modes with `modetest` or the kernel log. |
| Display rotated incorrectly | Check `systemctl show ha-kiosk -p Environment` and the rotation drop-in, then restart the service. |
| Screen blanks after a few minutes | `cat /proc/cmdline` must contain `consoleblank=0`; reboot after the change. On X11 also check `xset q`. |
| Touch input has rotated axes | Use the panel's documented calibration or rotation setting; do not hardcode a calibration matrix in the repository. |
| Fullscreen does not cover the screen on X11 | A window manager is required; start openbox or matchbox before the kiosk. |
| `auth_invalid` | The token is invalid, expired, or unreadable. Check owner and mode `0600`; never paste the token into an issue or log. |
| Crash loop and the unit is inactive | `journalctl -u ha-kiosk -b`; the unit allows five starts per five minutes. Fix the cause before `systemctl reset-failed ha-kiosk`. |
| Clock is wrong after boot | Configure `systemd-timesyncd` or chrony and set the device timezone. Home Assistant timestamps use the instance timezone. |
| Cache permission error | `/var/lib/ha-kiosk` must be `0700` and owned by `ha-kiosk`. |

## Hardware verification record

The deployment scripts are not a substitute for a device test. Record the
following on a Pi Zero 2 W and a Pi 3 with Bookworm:

- The service is active after a reboot and restarts after a forced failure.
- The display has no cursor and does not blank.
- The X11 fallback, if used, covers the screen with a minimal window manager.
- Stopping Home Assistant produces the offline badge.
- `journalctl -u ha-kiosk` contains no token.
- Memory use stays within the Wave 5 budget.

If no physical device is available, record “not verified on hardware” in the
issue and pull request and list what was tested instead.
