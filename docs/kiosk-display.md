# Kiosk display setup

This guide explains how the kiosk reaches the screen, how to rotate and
calibrate a panel, and how to keep the display awake. It covers the primary
LinuxKMS path and the X11 fallback. The specification is
[`PLAN.md`](../PLAN.md) section 8, Wave 4.

The binary resolves the display before it opens a window. If no backend is
available it exits with an error that names the missing setting instead of
starting a service that shows a blank screen. The same information is logged at
startup:

```text
INFO ha_kiosk: resolved display plan backend="linuxkms" backend_description="LinuxKMS direct-to-DRM software renderer" rotation=Some(90) fullscreen=true
```

## LinuxKMS (primary)

LinuxKMS writes directly to DRM and reads input through libinput and libseat.
There is no X server, no compositor, and no pointer, so the display needs no
cursor hiding and uses less memory than a windowed session.

Build the kiosk binary with the LinuxKMS features:

```bash
cargo build --release --no-default-features --features kiosk
```

Select the backend explicitly:

```bash
export SLINT_BACKEND=linuxkms-software
```

The systemd unit shipped in [`deploy/ha-kiosk.service`](../deploy/ha-kiosk.service)
sets `SLINT_BACKEND=linuxkms-software` and `LIBSEAT_BACKEND=seatd` already.

### Device access

LinuxKMS needs DRM and input access without root:

1. Install and enable `seatd`.
2. Add the service user to the `video`, `input`, and `render` groups.
3. Run the service with `LIBSEAT_BACKEND=seatd`.

Check access with:

```bash
ls -l /dev/dri/card0
groups ha-kiosk
systemctl status seatd
```

If the service cannot open the DRM device, the kiosk exits with a message that
points at the backend. Fix the seat or group access; do not run the service as
root.

### Rotation

`display.rotation` in the configuration accepts `0`, `90`, `180`, and `270`.
The kiosk passes it to Slint as `SLINT_KMS_ROTATION`. An explicit
`SLINT_KMS_ROTATION` in the environment wins over the configuration value, which
is useful for a one-off test.

```bash
# Override the configured rotation for a single run
SLINT_KMS_ROTATION=270 cargo run --release --no-default-features --features kiosk -- \
  --config /etc/ha-kiosk/config.toml
```

Rotation is a backend concern. Touch input follows the rotated output when the
panel and the touch controller report the same orientation. If the touch axis is
wrong after rotation, calibrate the panel as described below.

### Display selection

On a device with more than one connector or several supported modes, select the
output and mode explicitly:

```bash
export SLINT_DRM_OUTPUT=HDMI-A-1
export SLINT_DRM_MODE=800x480
```

`SLINT_DRM_OUTPUT` takes a connector name from `/sys/class/drm`. `SLINT_DRM_MODE`
takes a resolution the connector advertises. Leave both unset to let Slint use
the first connected output and its preferred mode.

### Console blanking

The kernel console can blank the panel even though Slint keeps drawing. Disable
console blanking for the LinuxKMS path by adding `consoleblank=0` to the kernel
command line:

```bash
# Raspberry Pi OS Bookworm
sudo sed -i 's/$/ consoleblank=0/' /boot/firmware/cmdline.txt
sudo reboot
```

[`deploy/kiosk-setup.sh`](../deploy/kiosk-setup.sh) applies this change and
enables NTP. Reboot afterwards; the kernel command line is read only at boot.

Verify the setting:

```bash
cat /proc/cmdline
```

## X11 fallback

Use X11 when LinuxKMS is not available, for example on a desktop distribution
with an existing session. Build with the `desktop` feature:

```bash
cargo build --release --features desktop
```

The winit backend needs `DISPLAY` and, when the X server uses authorization,
`XAUTHORITY`. A minimal window manager is required for fullscreen to cover the
screen. Without one, the window can keep a title bar or fail to raise.

```bash
sudo apt install openbox unclutter
```

Start a minimal session:

```bash
openbox &
unclutter -idle 0 &
xset s off -dpms
exec ha-kiosk --config /etc/ha-kiosk/config.toml
```

- `unclutter -idle 0` hides the pointer immediately. Slint 1.19 has no
  cursor-visibility API, so cursor hiding is a session setting.
- `xset s off -dpms` disables the X screen saver and display power management.
  Add it to the session startup so the panel stays awake after a reboot.

Wayland compositors use the same winit backend. Hide the pointer in the
compositor settings instead of `unclutter`, and disable idle blanking there.

## Touch calibration

Do not hardcode a calibration matrix in the kiosk. The correct matrix depends on
the panel, the controller, and the rotation. Common 7-inch panels need one of:

- **Rotation only:** set `display.rotation` or `SLINT_KMS_ROTATION`. This fixes
  a portrait panel that is mounted sideways.
- **Axis swap or inversion:** set the libinput calibration matrix for the touch
  device. This fixes a display that is rotated correctly but whose touch axes
  are swapped or mirrored.

Find the touch device and its matrix:

```bash
libinput list-devices
udevadm info --query=property --name=/dev/input/event0 | grep -i matrix
```

Set the matrix through a udev rule or the desktop session. For example, a
90-degree clockwise rotation with swapped axes uses:

```bash
LIBINPUT_CALIBRATION_MATRIX="0 1 0 -1 0 1"
```

Record the values that work for your panel in the deployment notes. Do not add
them to the repository; the matrix is hardware-specific.

## Fonts and icons

The UI uses the system sans font and does not fetch a font at runtime.

- Install `fonts-dejavu-core` on the device, or bundle a licensed font with the
  binary and record its license.
- Weather icons are compiled into the binary from the source tree. Their license
  and attribution are listed in the README under Assets.

If text renders as empty boxes, the font package is missing. Install it and
restart the service.

## Time and timezone

A Raspberry Pi without a real-time clock starts with a wrong clock until the
network is up. Enable time synchronization and set the device timezone:

```bash
sudo timedatectl set-ntp true
sudo timedatectl set-timezone Etc/UTC
timedatectl status
```

`systemd-timesyncd` is enough for a kiosk; `chrony` is an alternative for
networks with poor time sources. The kiosk uses the Home Assistant timezone from
`get_config` for entity timestamps, so the clock and the calendar stay correct
even when the device and Home Assistant use different timezones.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `no display backend is available` | Set `SLINT_BACKEND=linuxkms-software` on a kiosk build, or start the desktop build under X11 or Wayland. |
| `SLINT_BACKEND=... is not compiled` | Rebuild with `--no-default-features --features kiosk` for LinuxKMS or `--features desktop` for X11 and Wayland. |
| Blank screen after boot | Confirm `consoleblank=0` in `/proc/cmdline`, then reboot. |
| Touch axis wrong | Set the rotation first, then the libinput calibration matrix for the panel. |
| Window does not cover the screen | Run a minimal window manager such as openbox or matchbox. |
| Pointer visible on X11 | Run `unclutter -idle 0`; Slint cannot hide the cursor itself. |
| Screen blanks on X11 | Run `xset s off -dpms` in the session startup. |
| Wrong clock after boot | Enable NTP and set the device timezone with `timedatectl`. |
| DRM permission denied | Check `seatd`, the `video`/`input`/`render` groups, and `LIBSEAT_BACKEND=seatd`. |
