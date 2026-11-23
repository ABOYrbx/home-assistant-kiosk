#!/usr/bin/env bash
# Configure the kiosk display: seatd, console blanking, and rotation.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

BACKEND=kms
ROTATION=0
SERVICE_USER=ha-kiosk
DRY_RUN=0
export DRY_RUN

ROTATION_DROPIN_DIR=/etc/systemd/system/ha-kiosk.service.d
X11_SESSION=/etc/ha-kiosk/x11-session.sh

usage() {
  cat <<'EOF'
Usage: sudo deploy/kiosk-setup.sh [options]

Configure the device for the default seatd + LinuxKMS deployment. Run
deploy/install.sh first. A reboot is required after the kernel command line
changes.

Options:
  --backend kms|x11  Display backend. kms is the default and the supported
                     model. x11 installs the fallback packages and writes a
                     session script; the remaining autologin and user-service
                     steps are printed and documented in docs/deployment.md.
  --rotation DEG     0, 90, 180, or 270. Default 0. On KMS this writes
                     SLINT_KMS_ROTATION into a systemd drop-in; on X11 it is
                     applied with xrandr in the session script.
  --user NAME        Service user for the KMS model. Default ha-kiosk.
  --dry-run          Print the actions without changing the system.
  -h, --help         Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --backend)
      BACKEND="${2:?--backend needs kms or x11}"
      shift 2
      ;;
    --rotation)
      ROTATION="${2:?--rotation needs 0, 90, 180, or 270}"
      shift 2
      ;;
    --user)
      SERVICE_USER="${2:?--user needs a name}"
      shift 2
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1 (try --help)"
      ;;
  esac
done

validate_backend "$BACKEND" || die "invalid backend: $BACKEND (use kms or x11)"
validate_rotation "$ROTATION" || die "invalid rotation: $ROTATION (use 0, 90, 180, or 270)"

require_root
require_debian

NEEDS_REBOOT=0

configure_consoleblank() {
  local required="${1:-1}" file
  if file=$(find_cmdline_file); then
    if ! grep -q 'consoleblank=0' "$file"; then
      NEEDS_REBOOT=1
    fi
    ensure_consoleblank "$file"
  elif [ "$DRY_RUN" -eq 1 ] || [ "$required" -eq 0 ]; then
    warn "no kernel command line found; console blanking must be handled by the session"
  else
    die "no kernel command line found at /boot/firmware/cmdline.txt or /boot/cmdline.txt"
  fi
}

require_service_user() {
  if id -u "$SERVICE_USER" >/dev/null 2>&1; then
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    warn "user $SERVICE_USER does not exist yet; run deploy/install.sh first"
    return 0
  fi
  die "user $SERVICE_USER does not exist; run deploy/install.sh first"
}

setup_kms() {
  log "configuring the LinuxKMS + seatd deployment"
  require_service_user
  apt_install seatd
  enable_unit_if_present seatd.service

  ensure_group seat
  ensure_group video
  ensure_group render
  ensure_group input
  add_user_to_group "$SERVICE_USER" seat
  add_user_to_group "$SERVICE_USER" video
  add_user_to_group "$SERVICE_USER" render
  add_user_to_group "$SERVICE_USER" input

  configure_consoleblank

  if [ "$ROTATION" -eq 0 ]; then
    if [ -e "$ROTATION_DROPIN_DIR/10-kiosk-rotation.conf" ]; then
      log "removing the rotation drop-in (rotation 0)"
      run rm -f "$ROTATION_DROPIN_DIR/10-kiosk-rotation.conf"
    fi
  else
    write_rotation_dropin "$ROTATION_DROPIN_DIR" "$ROTATION"
  fi

  run systemctl daemon-reload
  if [ -f /etc/systemd/system/ha-kiosk.service ]; then
    run systemctl restart ha-kiosk.service
  else
    warn "ha-kiosk.service is not installed; run deploy/install.sh first"
  fi
}

write_x11_session() {
  local tmp rotation
  rotation=$(x11_rotation)
  if [ "$DRY_RUN" -eq 1 ]; then
    log "would write $X11_SESSION with xrandr rotation $rotation"
    return 0
  fi
  tmp=$(mktemp)
  cat >"$tmp" <<'EOS'
#!/bin/sh
# Started by the desktop user's X session. Keep the panel awake and the
# pointer hidden, apply the configured rotation, then start the kiosk.
set -eu
HA_KIOSK_XRANDR_ROTATE=${HA_KIOSK_XRANDR_ROTATE:-@ROTATION@}
xset s off -dpms s noblank
unclutter -idle 0 -root &
openbox &
if [ -n "$HA_KIOSK_XRANDR_ROTATE" ]; then
  output=$(xrandr | awk '/ connected/{print $1; exit}')
  if [ -n "$output" ]; then
    xrandr --output "$output" --rotate "$HA_KIOSK_XRANDR_ROTATE"
  fi
fi
exec /usr/local/bin/ha-kiosk --config "$HOME/.config/ha-kiosk/config.toml"
EOS
  sed -i "s/@ROTATION@/$rotation/" "$tmp"
  install -D -m 0755 "$tmp" "$X11_SESSION"
  rm -f "$tmp"
  log "wrote $X11_SESSION"
}

x11_rotation() {
  case "$ROTATION" in
    0) printf 'normal\n' ;;
    90) printf 'left\n' ;;
    180) printf 'inverted\n' ;;
    270) printf 'right\n' ;;
    *) die "invalid rotation: $ROTATION" ;;
  esac
}

setup_x11() {
  log "configuring the X11 fallback"
  apt_install xserver-xorg xinit openbox unclutter x11-xserver-utils fonts-dejavu-core
  configure_consoleblank 0
  write_x11_session
  cat <<EOF
X11 session script: $X11_SESSION

The system unit deploy/ha-kiosk.service is not used for X11. Start the session
with:

  startx $X11_SESSION

The script disables blanking, hides the pointer, starts openbox, applies the
rotation, and execs the kiosk. Put config.toml and the token under
~/.config/ha-kiosk/ with mode 0600.

Alternative: an autologin graphical session plus the example user unit
deploy/ha-kiosk-user.service. In that model the session must start openbox and
unclutter; the user unit starts only the kiosk. See docs/deployment.md.

The X11 path is a documented fallback and is not the release gate.
EOF
}

case "$BACKEND" in
  kms) setup_kms ;;
  x11) setup_x11 ;;
esac

if [ "$NEEDS_REBOOT" -eq 1 ]; then
  cat <<'EOF'

Reboot required: the kernel command line changed. Reboot the device before
judging the display; console blanking is applied only at boot.
EOF
fi
