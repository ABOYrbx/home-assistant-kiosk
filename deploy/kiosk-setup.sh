#!/usr/bin/env bash
# Optional display/seat preparation for a LinuxKMS kiosk.
set -euo pipefail

# Disable console blanking for the LinuxKMS path (append to kernel cmdline).
CMDLINE="/boot/firmware/cmdline.txt"
if [[ -f "$CMDLINE" ]] && ! grep -q "consoleblank=0" "$CMDLINE"; then
    sed -i 's/$/ consoleblank=0/' "$CMDLINE"
    echo "Added consoleblank=0 to $CMDLINE (reboot required)."
fi

# Enable NTP
if command -v timedatectl >/dev/null 2>&1; then
    timedatectl set-ntp true || true
fi

# Seatd is provided by the install script; ensure it is enabled.
systemctl enable --now seatd.service 2>/dev/null || true
echo "Kiosk setup steps complete. Reboot to apply console blanking."
