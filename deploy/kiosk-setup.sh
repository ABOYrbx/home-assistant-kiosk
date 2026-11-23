#!/usr/bin/env bash
# Prepare a LinuxKMS kiosk display: console blanking, NTP, and seatd.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    echo "kiosk-setup.sh must run as root" >&2
    exit 1
fi

# Raspberry Pi OS Bookworm uses /boot/firmware; older images use /boot.
CMDLINE=""
for candidate in /boot/firmware/cmdline.txt /boot/cmdline.txt; do
    if [[ -f "$candidate" ]]; then
        CMDLINE="$candidate"
        break
    fi
done

if [[ -z "$CMDLINE" ]]; then
    echo "warning: no kernel command line found; add consoleblank=0 manually" >&2
elif grep -q "consoleblank=0" "$CMDLINE"; then
    echo "consoleblank=0 already set in $CMDLINE"
else
    # cmdline.txt must stay on one line, so append without a trailing newline.
    printf ' consoleblank=0' >>"$CMDLINE"
    echo "Added consoleblank=0 to $CMDLINE (reboot required)."
fi

if command -v timedatectl >/dev/null 2>&1; then
    timedatectl set-ntp true || echo "warning: could not enable NTP" >&2
fi

systemctl enable --now seatd.service 2>/dev/null ||
    echo "warning: seatd.service is not available" >&2

echo "Kiosk setup steps complete. Reboot to apply console blanking."
