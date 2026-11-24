#!/usr/bin/env bash
# Validate the deployment units and shell scripts without touching the system.
set -euo pipefail

ROOT=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

FAILURES=0

pass() { printf 'ok - %s\n' "$*"; }
fail() {
  printf 'not ok - %s\n' "$*" >&2
  FAILURES=$((FAILURES + 1))
}

assert_contains() {
  local file="$1" needle="$2"
  if grep -Fq -- "$needle" "$file"; then
    pass "$file contains: $needle"
  else
    fail "$file is missing: $needle"
  fi
}

assert_not_contains() {
  local file="$1" needle="$2"
  if grep -Fq -- "$needle" "$file"; then
    fail "$file must not contain: $needle"
  else
    pass "$file does not contain: $needle"
  fi
}

printf '# syntax\n'
for script in "$ROOT"/deploy/*.sh "$ROOT"/tests/deploy_scripts_test.sh; do
  if bash -n "$script"; then
    pass "bash -n ${script#"$ROOT"/}"
  else
    fail "bash -n ${script#"$ROOT"/}"
  fi
done

printf '# unit files\n'
UNIT="$ROOT/deploy/ha-kiosk.service"
for needle in \
  'User=ha-kiosk' \
  'Group=ha-kiosk' \
  'SupplementaryGroups=seat video render input' \
  'Restart=on-failure' \
  'RestartSec=5' \
  'StartLimitIntervalSec=300' \
  'StartLimitBurst=5' \
  'Environment=RUST_LOG=info' \
  'Environment=SLINT_BACKEND=linuxkms-software' \
  'Environment=LIBSEAT_BACKEND=seatd' \
  'NoNewPrivileges=yes' \
  'ProtectSystem=strict' \
  'ReadWritePaths=/var/lib/ha-kiosk' \
  'StateDirectoryMode=0700' \
  'WantedBy=multi-user.target'; do
  assert_contains "$UNIT" "$needle"
done
assert_not_contains "$UNIT" 'User=root'

USER_UNIT="$ROOT/deploy/ha-kiosk-user.service"
assert_contains "$USER_UNIT" 'Environment=SLINT_BACKEND=winit'
assert_contains "$USER_UNIT" 'WantedBy=graphical-session.target'
assert_not_contains "$USER_UNIT" 'PrivateTmp=yes'

if command -v systemd-analyze >/dev/null 2>&1; then
  for unit in "$UNIT" "$USER_UNIT"; do
    name=$(basename "$unit")
    sed -e 's#^ExecStart=.*#ExecStart=/bin/true#' \
      -e 's#^Documentation=.*#Documentation=https://example.test/#' \
      "$unit" >"$WORK/$name"
    if systemd-analyze verify "$WORK/$name" 2>"$WORK/$name.err"; then
      pass "systemd-analyze verify $name"
    else
      cat "$WORK/$name.err" >&2
      fail "systemd-analyze verify $name"
    fi
  done
else
  printf 'skip - systemd-analyze is not installed\n'
fi

printf '# shared helpers\n'
# shellcheck source=/dev/null
. "$ROOT/deploy/lib.sh"

for rotation in 0 90 180 270; do
  if validate_rotation "$rotation"; then
    pass "rotation $rotation is valid"
  else
    fail "rotation $rotation should be valid"
  fi
done
for rotation in 45 -90 360 ''; do
  if validate_rotation "$rotation"; then
    fail "rotation '$rotation' should be rejected"
  else
    pass "rotation '$rotation' is rejected"
  fi
done
for backend in kms x11; do
  if validate_backend "$backend"; then
    pass "backend $backend is valid"
  else
    fail "backend $backend should be valid"
  fi
done
if validate_backend wayland; then
  fail 'backend wayland should be rejected'
else
  pass 'backend wayland is rejected'
fi

mkdir -p "$WORK/root/boot/firmware"
printf 'console=tty1 root=PARTUUID=abc\n' >"$WORK/root/boot/firmware/cmdline.txt"
found=$(find_cmdline_file "$WORK/root")
if [ "$found" = "$WORK/root/boot/firmware/cmdline.txt" ]; then
  pass 'find_cmdline_file finds the Bookworm path'
else
  fail "find_cmdline_file returned: $found"
fi

CMDLINE="$WORK/cmdline.txt"
printf 'console=tty1 root=PARTUUID=abc rootfstype=ext4 fsck.repair=yes\n' >"$CMDLINE"
ensure_consoleblank "$CMDLINE" >/dev/null
assert_contains "$CMDLINE" 'consoleblank=0'
if [ "$(wc -l <"$CMDLINE")" -eq 1 ]; then
  pass 'consoleblank keeps one line'
else
  fail 'consoleblank must keep the kernel command line on one line'
fi
ensure_consoleblank "$CMDLINE" >/dev/null
if [ "$(grep -o 'consoleblank=0' "$CMDLINE" | wc -l)" -eq 1 ]; then
  pass 'consoleblank is idempotent'
else
  fail 'consoleblank must be idempotent'
fi

CMDLINE_REPLACE="$WORK/cmdline-replace.txt"
printf 'console=tty1 consoleblank=600 root=/dev/mmcblk0p2\n' >"$CMDLINE_REPLACE"
ensure_consoleblank "$CMDLINE_REPLACE" >/dev/null
assert_contains "$CMDLINE_REPLACE" 'consoleblank=0'
assert_not_contains "$CMDLINE_REPLACE" 'consoleblank=600'

DROPIN_DIR="$WORK/unit.d"
write_rotation_dropin "$DROPIN_DIR" 90
assert_contains "$DROPIN_DIR/10-kiosk-rotation.conf" 'Environment=SLINT_KMS_ROTATION=90'

printf '# binary replacement\n'
mkdir -p "$WORK/bin"
cp /bin/sleep "$WORK/bin/ha-kiosk"
"$WORK/bin/ha-kiosk" 5 &
RUNNING_PID=$!
if install_binary /bin/true "$WORK/bin/ha-kiosk" "$(id -un)" "$(id -gn)" >/dev/null; then
  pass 'install_binary replaces a running binary'
else
  fail 'install_binary could not replace a running binary'
fi
kill "$RUNNING_PID" 2>/dev/null || true
wait "$RUNNING_PID" 2>/dev/null || true
installed_files=$(find "$WORK/bin" -maxdepth 1 -type f | wc -l)
if [ -x "$WORK/bin/ha-kiosk" ] && [ "$installed_files" -eq 1 ]; then
  pass 'install_binary leaves one executable and no temporary file'
else
  fail 'install_binary left an unexpected file in the destination directory'
fi

printf '# scripts\n'
for script in install.sh uninstall.sh kiosk-setup.sh; do
  if bash "$ROOT/deploy/$script" --help >/dev/null; then
    pass "$script --help"
  else
    fail "$script --help"
  fi
done

if bash "$ROOT/deploy/install.sh" --dry-run --binary /bin/true \
  --config "$ROOT/config.example.toml" --skip-packages >"$WORK/install.out" 2>&1; then
  pass 'install.sh --dry-run'
else
  cat "$WORK/install.out" >&2
  fail 'install.sh --dry-run'
fi
assert_contains "$WORK/install.out" 'installing the ha-kiosk system service'

if bash "$ROOT/deploy/uninstall.sh" --dry-run >"$WORK/uninstall.out" 2>&1; then
  pass 'uninstall.sh --dry-run'
else
  cat "$WORK/uninstall.out" >&2
  fail 'uninstall.sh --dry-run'
fi
assert_contains "$WORK/uninstall.out" 'systemctl stop ha-kiosk.service'

if bash "$ROOT/deploy/uninstall.sh" --dry-run --purge >"$WORK/purge.out" 2>&1; then
  pass 'uninstall.sh --dry-run --purge'
else
  cat "$WORK/purge.out" >&2
  fail 'uninstall.sh --dry-run --purge'
fi
assert_contains "$WORK/purge.out" 'rm -rf /etc/ha-kiosk /var/lib/ha-kiosk'

if bash "$ROOT/deploy/kiosk-setup.sh" --dry-run --backend kms \
  --rotation 90 >"$WORK/setup.out" 2>&1; then
  pass 'kiosk-setup.sh --dry-run'
else
  cat "$WORK/setup.out" >&2
  fail 'kiosk-setup.sh --dry-run'
fi
assert_contains "$WORK/setup.out" 'SLINT_KMS_ROTATION=90'

if bash "$ROOT/deploy/kiosk-setup.sh" --dry-run --backend x11 \
  --rotation 270 >"$WORK/setup-x11.out" 2>&1; then
  pass 'kiosk-setup.sh --dry-run --backend x11'
else
  cat "$WORK/setup-x11.out" >&2
  fail 'kiosk-setup.sh --dry-run --backend x11'
fi
assert_contains "$WORK/setup-x11.out" 'xrandr rotation right'

if bash "$ROOT/deploy/kiosk-setup.sh" --rotation 45 >"$WORK/rotation.out" 2>&1; then
  fail 'kiosk-setup.sh must reject rotation 45'
else
  pass 'kiosk-setup.sh rejects rotation 45'
fi
assert_contains "$WORK/rotation.out" 'invalid rotation'

if bash "$ROOT/deploy/install.sh" --dry-run --skip-packages >"$WORK/nobinary.out" 2>&1; then
  fail 'install.sh must require --binary'
else
  pass 'install.sh requires --binary'
fi

if command -v shellcheck >/dev/null 2>&1; then
  if (cd "$ROOT" && shellcheck -x deploy/*.sh tests/deploy_scripts_test.sh); then
    pass 'shellcheck'
  else
    fail 'shellcheck'
  fi
else
  printf 'skip - shellcheck is not installed\n'
fi

if [ "$FAILURES" -ne 0 ]; then
  printf '\n%d deployment check(s) failed\n' "$FAILURES" >&2
  exit 1
fi
printf '\nall deployment script checks passed\n'
