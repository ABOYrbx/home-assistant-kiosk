#!/usr/bin/env bash
# Install or upgrade the ha-kiosk system service on a Debian-based device.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=deploy/lib.sh
. "$SCRIPT_DIR/lib.sh"

BINARY=""
CONFIG=""
TOKEN_FILE=""
START=1
PACKAGES=1
FORCE=0
DRY_RUN=0
export DRY_RUN

CONFIG_DIR=/etc/ha-kiosk
STATE_DIR=/var/lib/ha-kiosk
UNIT=/etc/systemd/system/ha-kiosk.service
BIN_DST=/usr/local/bin/ha-kiosk
DOC_DIR=/usr/share/doc/ha-kiosk
SERVICE_USER=ha-kiosk

usage() {
  cat <<'EOF'
Usage: sudo deploy/install.sh --binary PATH [options]

Install or upgrade the ha-kiosk system service. The default deployment runs as
the ha-kiosk user with seatd and LinuxKMS; it never runs as root.

Options:
  --binary PATH      Release binary to install (required).
  --config PATH      TOML configuration for a fresh install. Ignored when
                     /etc/ha-kiosk/config.toml already exists unless --force.
  --token-file PATH  Home Assistant token file to install with mode 0600.
                     Ignored when /etc/ha-kiosk/ha_token already exists unless
                     --force.
  --force            Overwrite an existing config or token file.
  --no-start         Install and enable the unit but do not start it.
  --skip-packages    Do not install runtime packages with apt-get.
  --dry-run          Print the actions without changing the system.
  -h, --help         Show this help.

Upgrade: run with --binary and without --config/--token-file. The existing
configuration, token, and cache are kept.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --binary)
      BINARY="${2:?--binary needs a path}"
      shift 2
      ;;
    --config)
      CONFIG="${2:?--config needs a path}"
      shift 2
      ;;
    --token-file)
      TOKEN_FILE="${2:?--token-file needs a path}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --no-start)
      START=0
      shift
      ;;
    --skip-packages)
      PACKAGES=0
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

[ -n "$BINARY" ] || die "--binary is required (try --help)"
if [ "$DRY_RUN" -eq 0 ]; then
  [ -f "$BINARY" ] || die "binary not found: $BINARY"
  [ -x "$BINARY" ] || die "binary is not executable: $BINARY"
  [ -f "$SCRIPT_DIR/ha-kiosk.service" ] || die "unit file not found: $SCRIPT_DIR/ha-kiosk.service"
fi

CONFIG_DST="$CONFIG_DIR/config.toml"
TOKEN_DST="$CONFIG_DIR/ha_token"

if [ -n "$CONFIG" ]; then
  if [ "$DRY_RUN" -eq 0 ] && [ ! -f "$CONFIG" ]; then
    die "config not found: $CONFIG"
  fi
  if [ -e "$CONFIG_DST" ] && [ "$FORCE" -eq 0 ]; then
    die "$CONFIG_DST already exists; pass --force to replace it, or omit --config to upgrade"
  fi
elif [ ! -e "$CONFIG_DST" ] && [ "$DRY_RUN" -eq 0 ]; then
  die "no configuration at $CONFIG_DST; pass --config PATH for a fresh install"
fi

if [ -n "$TOKEN_FILE" ]; then
  if [ "$DRY_RUN" -eq 0 ] && [ ! -f "$TOKEN_FILE" ]; then
    die "token file not found: $TOKEN_FILE"
  fi
  if [ -e "$TOKEN_DST" ] && [ "$FORCE" -eq 0 ]; then
    die "$TOKEN_DST already exists; pass --force to replace it"
  fi
fi

require_root
if [ "$PACKAGES" -eq 1 ]; then
  require_debian
fi

log "installing the ha-kiosk system service (dry run: $DRY_RUN)"

if [ "$PACKAGES" -eq 1 ]; then
  apt_install \
    libseat1 libinput10 libudev1 libxkbcommon0 libfontconfig1 \
    fonts-dejavu-core ca-certificates seatd
fi

ensure_group seat
ensure_group video
ensure_group render
ensure_group input
ensure_group "$SERVICE_USER"
ensure_system_user "$SERVICE_USER" "$STATE_DIR" "$SERVICE_USER"
add_user_to_group "$SERVICE_USER" seat
add_user_to_group "$SERVICE_USER" video
add_user_to_group "$SERVICE_USER" render
add_user_to_group "$SERVICE_USER" input

run install -d -m 0750 -o "$SERVICE_USER" -g "$SERVICE_USER" "$CONFIG_DIR"
run install -d -m 0700 -o "$SERVICE_USER" -g "$SERVICE_USER" "$STATE_DIR"

install_binary "$BINARY" "$BIN_DST" root root
install_file "$SCRIPT_DIR/ha-kiosk.service" "$UNIT" 0644 root root

if [ -n "$CONFIG" ]; then
  install_file "$CONFIG" "$CONFIG_DST" 0600 "$SERVICE_USER" "$SERVICE_USER"
elif [ ! -e "$CONFIG_DST" ]; then
  if [ "$DRY_RUN" -eq 1 ]; then
    warn "no configuration at $CONFIG_DST; a fresh install needs --config PATH"
  else
    die "no configuration at $CONFIG_DST"
  fi
else
  log "keeping existing configuration at $CONFIG_DST"
fi

if [ -n "$TOKEN_FILE" ]; then
  install_file "$TOKEN_FILE" "$TOKEN_DST" 0600 "$SERVICE_USER" "$SERVICE_USER"
elif [ -e "$TOKEN_DST" ]; then
  log "keeping existing token file at $TOKEN_DST"
else
  warn "no token file at $TOKEN_DST; set HA_TOKEN through a drop-in or install one before the first start"
fi

if [ -f "$SCRIPT_DIR/../README.md" ]; then
  install_file "$SCRIPT_DIR/../README.md" "$DOC_DIR/README.md" 0644 root root
fi
if [ -f "$SCRIPT_DIR/../docs/deployment.md" ]; then
  install_file "$SCRIPT_DIR/../docs/deployment.md" "$DOC_DIR/deployment.md" 0644 root root
fi

enable_unit_if_present seatd.service
run systemctl daemon-reload
run systemctl enable ha-kiosk.service
if [ "$START" -eq 1 ]; then
  if run systemctl restart ha-kiosk.service; then
    if [ "$DRY_RUN" -eq 0 ] && ! systemctl is-active --quiet ha-kiosk.service; then
      warn "ha-kiosk.service is not active; inspect journalctl -u ha-kiosk -b"
    fi
  else
    warn "could not start ha-kiosk.service; inspect journalctl -u ha-kiosk -b"
  fi
else
  log "unit enabled but not started (--no-start)"
fi

cat <<EOF
ha-kiosk is installed.

  Config:  $CONFIG_DST
  Token:   $TOKEN_DST
  State:   $STATE_DIR
  Binary:  $BIN_DST
  Logs:    journalctl -u ha-kiosk -f

Next: run "sudo deploy/kiosk-setup.sh" to apply the seat groups, disable
console blanking, and set the display rotation, then reboot. The service is
not verified on hardware by this installer.
EOF
