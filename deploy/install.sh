#!/usr/bin/env bash
# Install ha-kiosk on a Raspberry Pi OS Bookworm system.
set -euo pipefail

BINARY="${1:-/usr/local/bin/ha-kiosk}"
CONFIG_SRC="${2:-/etc/ha-kiosk/config.toml}"
SERVICE_USER="ha-kiosk"
SERVICE_GROUP="ha-kiosk"

if [[ $EUID -ne 0 ]]; then
    echo "install.sh must run as root" >&2
    exit 1
fi

# Runtime packages
apt-get update
apt-get install -y --no-install-recommends \
    ca-certificates fonts-dejavu-core seatd \
    libseat1 libinput10 libudev1 libxkbcommon0 libfontconfig1

# Service user
if ! id -u "${SERVICE_USER}" >/dev/null 2>&1; then
    useradd --system --home /var/lib/ha-kiosk --shell /usr/sbin/nologin "${SERVICE_USER}"
fi
usermod -aG video,input,render,seat "${SERVICE_USER}" 2>/dev/null || true

# Directories
install -d -m 0750 -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" /etc/ha-kiosk
install -d -m 0700 -o "${SERVICE_USER}" -g "${SERVICE_GROUP}" /var/lib/ha-kiosk

# Binary
install -m 0755 -o root -g root "${BINARY}" /usr/local/bin/ha-kiosk

# Unit
install -m 0644 deploy/ha-kiosk.service /etc/systemd/system/ha-kiosk.service 2>/dev/null || \
    install -m 0644 /usr/share/ha-kiosk/ha-kiosk.service /etc/systemd/system/ha-kiosk.service

systemctl daemon-reload
systemctl enable --now ha-kiosk.service
echo "ha-kiosk installed and started"
