#!/usr/bin/env bash
# Remove the ha-kiosk system service.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

PURGE=0
DRY_RUN=0
export DRY_RUN

UNIT=/etc/systemd/system/ha-kiosk.service
DROPIN_DIR=/etc/systemd/system/ha-kiosk.service.d
BIN=/usr/local/bin/ha-kiosk
DOC_DIR=/usr/share/doc/ha-kiosk
CONFIG_DIR=/etc/ha-kiosk
STATE_DIR=/var/lib/ha-kiosk
SERVICE_USER=ha-kiosk

usage() {
  cat <<'EOF'
Usage: sudo deploy/uninstall.sh [options]

Stop and disable ha-kiosk, remove the unit and binary. The configuration,
token, cache, and service user are kept unless --purge is passed.

Options:
  --purge     Also remove /etc/ha-kiosk, /var/lib/ha-kiosk, and the ha-kiosk
              user. This deletes the token and the offline cache.
  --dry-run   Print the actions without changing the system.
  -h, --help  Show this help.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --purge)
      PURGE=1
      shift
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

require_root

if [ "$DRY_RUN" -eq 1 ] || systemctl is-active --quiet ha-kiosk.service; then
  if ! run systemctl stop ha-kiosk.service; then
    warn "could not stop ha-kiosk.service"
    if [ "$PURGE" -eq 1 ] && [ "$DRY_RUN" -eq 0 ]; then
      die "refusing to purge while the service may still be running"
    fi
  fi
fi
run systemctl disable ha-kiosk.service || true
run systemctl reset-failed ha-kiosk.service || true
run rm -f "$UNIT"
run rm -rf "$DROPIN_DIR"
run systemctl daemon-reload
run rm -f "$BIN" "${BIN}.previous"
run rm -rf "$DOC_DIR"

if [ "$PURGE" -eq 1 ]; then
  run rm -rf "$CONFIG_DIR" "$STATE_DIR"
  if id -u "$SERVICE_USER" >/dev/null 2>&1; then
    run userdel "$SERVICE_USER" || warn "could not remove user $SERVICE_USER"
  fi
  log "removed the configuration, token, cache, and service user"
else
  cat <<EOF
ha-kiosk is uninstalled. The configuration and cache were kept:
  $CONFIG_DIR
  $STATE_DIR

Remove them with --purge, or reinstall the binary to keep the existing setup.
EOF
fi
