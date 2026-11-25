# Troubleshooting

This guide covers the failures an operator is most likely to hit while building, installing, and running the kiosk. It follows the behaviour described in [`PLAN.md`](../PLAN.md); the configuration reference is in [`config.example.toml`](../config.example.toml) and the release procedure is in [`release-checklist.md`](release-checklist.md).

## Collect evidence first

Before changing anything, capture the state of the service and the last log lines:

```bash
systemctl status ha-kiosk --no-pager
journalctl -u ha-kiosk -n 200 --no-pager
```

For a noisy problem, raise the log level for one run:

```bash
sudo systemctl edit ha-kiosk
# [Service]
# Environment=RUST_LOG=debug
sudo systemctl restart ha-kiosk
journalctl -u ha-kiosk -f
```

Reset `RUST_LOG=info` afterwards. `debug` is verbose and costs CPU on a Pi Zero 2 W.

The logs never contain the Home Assistant token, the authorization header, or image query strings. If you see any of them, treat it as a security bug and report it through [`SECURITY.md`](../SECURITY.md), not a public issue. Do not paste a token into an issue, a screenshot, or a chat.

Record these facts in any bug report:

- kiosk version or commit,
- Home Assistant version (Status page or `GET /api/`),
- OS release (`cat /etc/os-release`), architecture (`uname -m`), and hardware model,
- display backend (`SLINT_BACKEND`), rotation, and panel resolution,
- the exact command that failed and its output with secrets removed.

## Build and cross-compilation

Use `scripts/build-all.sh` for release artifacts. It selects the kiosk feature set, passes `--locked`, uses the image pinned for the target in `Cross.toml`, and names the artifact with its target. A host-native `cargo build` does not produce a Pi binary.

### `cross` is missing or Docker is not running

`cross` needs Docker (or Podman with a compatible socket). Check both:

```bash
cross --version
docker info
```

Install `cross` with `cargo install cross --locked` and start the Docker daemon. On a remote build host, confirm the current user can access the Docker socket.

### The cross image cannot be pulled

The build uses the Slint cross images pinned in `Cross.toml`. A pull failure is usually a registry outage, an expired token, or a digest that no longer exists. Retry once, then check the pinned digest. Do not replace a pinned digest with a floating tag to work around an outage: that makes the build unreproducible. Use the [native-build fallback](release-builds.md#native-build-fallback-on-the-pi) until the registry recovers.

### The binary cannot execute on the Pi

`Exec format error` or `cannot execute binary file` means the artifact architecture does not match the device. Check both sides:

```bash
file dist/ha-kiosk-<version>-<target>
uname -m
```

Use `aarch64-unknown-linux-gnu` for a 64-bit Pi OS (`aarch64`), `armv7-unknown-linux-gnueabihf` for a 32-bit Pi OS (`armv7l`), and `x86_64-unknown-linux-gnu` for `x86_64`. Rebuild with `scripts/build-all.sh`; renaming an artifact does not convert it. If you invoke `cargo build --target` directly, install the matching Rust standard library with `rustup target add <target>` first. `cross` supplies the target toolchain inside its container.

### The cross build cannot find C libraries

A linker or `pkg-config` error naming `fontconfig`, `xkbcommon`, `seat`, `input`, or `udev` belongs to the target sysroot. Check the `[build] pre-build` list in `Cross.toml`; it installs the architecture-qualified development packages inside the container. Rebuild from a clean target directory after changing that list. Installing the packages only on the build host does not fix a target linker error.

A native build needs the same development packages on the device. On Debian or Raspberry Pi OS:

```bash
sudo apt install build-essential pkg-config libfontconfig1-dev \
  libxkbcommon-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libseat-dev libinput-dev libudev-dev
```

Re-run the native build after installing them. On a Pi Zero 2 W, use `CARGO_BUILD_JOBS=1` and provide swap as described in the [release-build guide](release-builds.md#native-build-fallback-on-the-pi).

### `--all-features` fails on macOS or a non-Linux host

The LinuxKMS backend is Linux-only. Do not build with `--all-features` off Linux; select the desktop feature instead:

```bash
cargo build --features desktop
```

The kiosk feature set only builds on Linux:

```bash
cargo build --release --no-default-features --features kiosk
```

### Slint version conflicts

`slint`, `slint-build`, and `i-slint-backend-testing` must all stay on the same `1.19.x` release. A mixed version shows up as a trait or type mismatch between generated UI code and the runtime. Update all three requirements and their lockfile entries in one reviewed change; `cargo update --precise` does not accept a `1.19.*` wildcard. Keep `Cargo.lock` committed and verify with `cargo test`.

### A dependency fails the audit

`cargo audit` runs in CI and before a release. Read the advisory, check whether the vulnerable code path is reachable, and update the crate. Do not silence an advisory without a written reason in the issue or pull request.

## glibc mismatch

A cross-built binary reports a missing symbol version such as:

```text
./ha-kiosk: /lib/aarch64-linux-gnu/libc.so.6: version `GLIBC_2.35' not found
```

The cross images build against a glibc 2.35 sysroot (Ubuntu 22.04). Check the target:

```bash
ldd --version | head -1
cat /etc/os-release | grep VERSION_CODENAME
```

- Raspberry Pi OS **Bookworm** ships glibc 2.36 and works.
- Raspberry Pi OS **Bullseye** ships glibc 2.31 and does **not** run cross-built binaries. Build natively on the device instead, or upgrade to Bookworm.

Do not copy a Bullseye `libc` into the image, and do not statically link `glibc`. A native build is the supported fallback. `Exec format error` is an architecture problem, not a glibc problem; use the checks under [The binary cannot execute on the Pi](#the-binary-cannot-execute-on-the-pi) first.

## The display backend does not start

The process can exit cleanly while no window ever appears. Match the journal message to the failing layer before changing the panel configuration:

| Journal message | Meaning | Next step |
| --- | --- | --- |
| `SLINT_BACKEND=... is not compiled into this binary` | The selected backend is absent from the build | Rebuild with `--no-default-features --features kiosk` for LinuxKMS or `--features desktop` for X11/Wayland |
| `no display backend is available` | No explicit backend, DRM device, or windowed display was found | Set `SLINT_BACKEND=linuxkms-software` on a kiosk build, or start a desktop build inside X11/Wayland |
| DRM, seat, or input permission denied | The backend exists but the service user cannot open it | Check `seatd`, device groups, and the unit environment below |
| A backend starts but the screen is black | The backend opened; the mode, connector, blanking, or panel may be wrong | Check `SLINT_DRM_OUTPUT`, `SLINT_DRM_MODE`, and the screen-blanking section |

### LinuxKMS (default)

1. Confirm the environment of the running unit:

   ```bash
   systemctl show ha-kiosk -p Environment
   ```

   For a user service, use `systemctl --user show ha-kiosk -p Environment`. The default system unit sets `SLINT_BACKEND=linuxkms-software` and `LIBSEAT_BACKEND=seatd`.

2. Confirm a DRM device exists and the service user can use it:

   ```bash
   ls -l /dev/dri/
   id ha-kiosk
   ```

   The service user needs the `video`, `render`, and `input` groups for direct device access, and `seat` for the seatd socket. A working seat can grant the device descriptors without root.

3. Confirm the seat manager is running:

   ```bash
   systemctl status seatd --no-pager
   ```

4. If the unit hardening blocks the seat socket, the journal shows a permission error. Relax `ProtectSystem=strict` to `ProtectSystem=full` in a unit drop-in and record why in the deployment notes or issue. Do not run the kiosk as root to bypass the seat.

5. A wrong or unavailable DRM mode leaves a black screen. Select a mode advertised by the connector:

   ```bash
   Environment=SLINT_DRM_MODE=800x480
   ```

   For a multi-output device, select the connector with `SLINT_DRM_OUTPUT`, for example `HDMI-A-1`. Use the connector name from `/sys/class/drm/`. See [`kiosk-display.md`](kiosk-display.md#display-selection) for the full display setup.

### X11 and Wayland fallback

Build the windowed backend and run it inside a graphical session:

```bash
cargo build --release --features desktop
```

X11 needs `DISPLAY` and, when authorization is enabled, `XAUTHORITY`. Wayland needs `WAYLAND_DISPLAY` and the compositor's socket. Both need a window manager or compositor for fullscreen to cover the screen. On X11:

```bash
export DISPLAY=:0
xset s off -dpms
unclutter -idle 0 -root &
```

- Without a window manager, `set_fullscreen(true)` may not cover panels or the root window. Use a minimal window manager such as `openbox` or `matchbox`.
- A missing `XAUTHORITY` produces "cannot open display". Point the service environment at the correct file.
- A system service normally has no graphical session. Use the documented [user-service X11 model](deployment.md#x11-fallback) instead of granting root access.
- Cursor hiding is a deployment concern. `unclutter` works on X11; a Wayland compositor needs its own setting; the touch-only LinuxKMS path needs no pointer.

## Touch rotation and calibration

Rotation is a deployment concern, not an application setting alone.

- `display.rotation` accepts `0`, `90`, `180`, or `270`. On LinuxKMS it maps to `SLINT_KMS_ROTATION`; a non-empty environment value wins over the configuration, and `0` leaves the variable unset.
- `SLINT_KMS_ROTATION` does not rotate an X11 or Wayland desktop. Use `xrandr` or the compositor's output transform for those backends.
- Rotating the output does not rotate every touch controller automatically. On X11, map the device to the output or apply a coordinate transformation matrix with `xinput`. On LinuxKMS, libinput normally follows the output transform; if touches are mirrored, swapped, or offset, calibrate the touch device.

Find the touch device and its current transform without changing anything:

```bash
libinput list-devices
udevadm info --query=property --name=/dev/input/event0 | grep -i matrix
```

Replace `/dev/input/event0` with the touch controller's event device. Apply a panel-specific libinput calibration matrix through a udev rule or the desktop session; [`kiosk-display.md`](kiosk-display.md#touch-calibration) describes the options. Do not hardcode a calibration matrix in the repository. Record the working values in the device's local deployment notes and hardware test record.

After a rotation change, restart the service and test all four edges and corners of the screen. A wrong transform makes edge swipes unreachable. If the image is correct but the touch is 180 degrees off, fix the input transform, not the application layout.

## Token and permission problems

The installer creates the configuration and token with mode `0600`, owned by the service user. Startup rejects a token file with group or world permission bits. Keep the configuration at `0600` as a deployment security rule even though the current loader does not reject its mode before parsing.

Verify ownership and mode without displaying the token:

```bash
stat -c '%a %U:%G %n' /etc/ha-kiosk/config.toml /etc/ha-kiosk/ha_token
sudo -u ha-kiosk test -r /etc/ha-kiosk/ha_token
```

The second command must exit successfully. For the X11 user-service model, use that user's name and the paths under its home directory. If the configured paths differ, substitute them in both commands.

| Symptom | Cause | Fix |
| --- | --- | --- |
| Token file is world/group-readable | Permission bits are too broad | `sudo chmod 0600 /etc/ha-kiosk/ha_token` |
| Configuration is group/world-readable | Deployment file mode is unsafe | `sudo chmod 0600 /etc/ha-kiosk/config.toml` |
| "Permission denied" reading the token | Wrong owner or unreadable parent directory | `sudo chown ha-kiosk:ha-kiosk /etc/ha-kiosk/ha_token`, then check directory access |
| Token file is empty | The file contains only whitespace | Recreate it with the token and mode `0600`; do not paste the value into a log |
| "Authentication failed" and no retry | Invalid or revoked token | Create a new long-lived token and restart |
| A stale environment token wins | Non-empty `HA_TOKEN` overrides `ha.token_file` | Inspect the unit environment and drop-ins, remove the stale value, then restart |
| Token file not found | Relative path or wrong directory | Use an absolute path in `ha.token_file` |

Rules that catch operators out:

- `ha.token_file` must be absolute. Relative paths are rejected when the file is used.
- A non-empty `HA_TOKEN` overrides `ha.token_file`; an empty value falls through to the file. A stale value in a unit drop-in causes a confusing authentication failure.
- A literal `ha.token` in the configuration is for development only. Do not use it in a service.
- The token belongs to a Home Assistant user. Prefer a dedicated user that can see only the configured entities and actions. Some actions require an admin user; grant the minimum that the kiosk needs.
- After an `auth_invalid` response the kiosk stops reconnecting on purpose. It does not hammer Home Assistant with a bad token. Fix the token and restart the service.

If a token was ever committed, pasted into an issue, or shown in a screenshot, revoke it in Home Assistant and create a new one. Deleting the file is not enough.

## Home Assistant connection

### The version is too old

Check the version shown on the kiosk Status page and compare it with **Developer Tools → Info** in Home Assistant. The kiosk reads the version from the `auth_ok` response, so a mismatch means the kiosk is connected to a different instance than the one being inspected.

Home Assistant **2023.9** or newer is required, because the kiosk uses `subscribe_entities` and `weather.get_forecasts`. An older instance can authenticate but reject the subscription or forecast command. Upgrade Home Assistant; the kiosk does not fall back to a global `state_changed` subscription.

### The instance is behind a path prefix

Set the full URL, including the prefix:

```toml
[ha]
url = "https://ha.example.com/ha"
```

The WebSocket URL is derived as `wss://ha.example.com/ha/api/websocket`. A URL without the prefix connects to the wrong path and fails the handshake.

### TLS fails on a self-signed certificate

Install the CA certificate on the device so the system trust store accepts it. `allow_invalid_certs = true` is a documented last resort for a local certificate you control; it disables certificate validation and must never be used for a public host.

### The connection drops and reconnects

Reconnect uses exponential backoff with jitter, capped at 60 seconds. A flapping Wi-Fi link or a sleeping Home Assistant produces repeated attempts. Check the journal for the reason: a missed 30-second heartbeat, a closed socket, or a failed TLS handshake. Persistent `auth_invalid` is a token problem, not a network problem.

### A widget shows "unavailable" or "unknown"

The entity ID is validated syntactically but not checked against the server at startup. A typo, a removed entity, or an entity the token's user cannot see renders as unavailable or unknown. Compare the configured IDs with **Developer Tools → States** in Home Assistant. A missing entity must not blank the whole page; only that widget degrades.

## Offline and stale data

- When Home Assistant is unreachable, the last cached values stay visible with an "Offline — last update HH:MM" badge. This is expected behaviour, not a hang.
- The clock and date are always local and always live, even offline.
- A cache with a different schema version or configuration fingerprint is discarded. After adding, removing, or renaming a widget, tile, or entity, expect one empty start and a fresh snapshot.
- If the badge never clears after Home Assistant returns, check the journal for a failed re-authentication or a rejected subscription.
- The cache directory is `0700` and the cache file `0600`. A cache owned by the wrong user is not writable and the service logs a warning. Fix the ownership; do not widen the permissions.

## Timezone and clock

- The primary timezone source is `get_config.time_zone` from Home Assistant. `ha.time_zone` is the fallback, then the device timezone.
- Calendar and departure timestamps without an offset are interpreted in the Home Assistant timezone. A kiosk in another timezone still shows the correct local time.
- A Pi without a real-time clock shows the wrong time after boot until NTP syncs. Enable `systemd-timesyncd` or `chrony` and set the device timezone:

  ```bash
  timedatectl set-timezone Europe/Berlin
  timedatectl status
  ```

- If the clock is correct but calendar times are shifted, compare the Status page timezone with the Home Assistant general settings.

## Memory and performance

The hard gate is resident set size below 150 MB, with a target below 100 MB at idle. The first frame must appear within 3 seconds.

Measure on the device:

```bash
systemctl show ha-kiosk -p MemoryCurrent
/usr/bin/time -v ./ha-kiosk --config /etc/ha-kiosk/config.toml
```

On x86_64, `heaptrack` finds allocation hotspots. Common causes of high memory:

- album art from a large image; the cache is capped, but a huge source image still costs memory during decoding,
- a Home Assistant instance with a very large state set; the kiosk subscribes only to configured entities, so check for an accidental global subscription,
- `RUST_LOG=debug` left enabled in a unit drop-in,
- a community extension with an unbounded script (Wave 6).

Do not "fix" memory pressure by disabling the stale badge, the cache, or the service hardening. Fix the allocation or the configuration.

## Fonts and missing text

Text renders blank or with boxes when no usable font is installed. Install the runtime font package:

```bash
sudo apt install fonts-dejavu-core
fc-list | head
```

A bundled font is an alternative; fonts are never fetched at runtime. Icons are compiled into the binary, so a missing icon file is a build problem, not a deployment problem.

## Service and boot

- The unit runs as the `ha-kiosk` user and never as root. `systemctl status` shows the user it actually used.
- `Restart=on-failure` with `RestartSec=5` restarts a crashed process. A restart loop usually means a configuration, token, or display error that the journal reports on every attempt.
- `StartLimitBurst` stops a fast crash loop. After fixing the cause, reset the counter:

  ```bash
  sudo systemctl reset-failed ha-kiosk
  sudo systemctl start ha-kiosk
  ```

- The unit must start after the seat manager (`seatd`) or the graphical target. A start that is too early fails to open DRM or input. Add the dependency to the unit rather than sleeping in a wrapper script.
- `ProtectSystem=strict` allows writes only below `ReadWritePaths=/var/lib/ha-kiosk`. A cache path outside that directory fails with a read-only filesystem error.
- Check the boot order with `systemd-analyze critical-chain ha-kiosk.service`.

## Screen blanking and cursor

- LinuxKMS: disable console blanking with `consoleblank=0` on the kernel command line in `/boot/firmware/cmdline.txt` (Bookworm) and reboot.
- X11: `xset s off -dpms` in the session that owns the display, and `unclutter` to hide the pointer.
- Wayland: use the compositor's idle and cursor settings. The kiosk does not control cursor visibility; Slint 1.19 has no cursor-visibility API.
- A screen that blanks after a few minutes usually comes from the console or the desktop environment, not from the kiosk.

## Upgrade and rollback

- Stop the service before replacing the binary, then start it again and check the journal.
- Keep the previous binary until the new one has shown a frame and reconnected.
- If the new version fails, restore the previous binary, restart, and report the version and the journal excerpt.
- Configuration is not rewritten by an upgrade. A new required key appears as a startup error naming the key; update `config.toml` from `config.example.toml`.
- A cache written by a newer schema is discarded with a warning, not migrated. That is expected.

## Still stuck

Open a bug report with the evidence listed at the top of this page. Include the command, the output, and what you expected. Remove tokens, entity IDs, hostnames, and addresses first.

A security issue goes through [`SECURITY.md`](../SECURITY.md) and never into a public issue.
