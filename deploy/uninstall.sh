#!/usr/bin/env bash
set -euo pipefail

PURGE=0
if [[ "${1:-}" == "--purge" ]]; then
    PURGE=1
fi

if [[ $EUID -ne 0 ]]; then
    echo "uninstall.sh must run as root" >&2
    exit 1
fi

systemctl disable --now ha-kiosk.service 2>/dev/null || true
rm -f /etc/systemd/system/ha-kiosk.service
rm -f /usr/local/bin/ha-kiosk
systemctl daemon-reload

if [[ $PURGE -eq 1 ]]; then
    rm -rf /etc/ha-kiosk /var/lib/ha-kiosk
    userdel ha-kiosk 2>/dev/null || true
else
    echo "Configuration and data kept. Use --purge to remove them."
fi
echo "ha-kiosk uninstalled"
