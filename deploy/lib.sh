#!/usr/bin/env bash
# Shared helpers for the ha-kiosk deployment scripts.
# Sourced by install.sh, uninstall.sh, and kiosk-setup.sh; not executed alone.
# shellcheck shell=bash

: "${DRY_RUN:=0}"

log() { printf 'ha-kiosk: %s\n' "$*"; }
warn() { printf 'ha-kiosk: warning: %s\n' "$*" >&2; }
die() {
  printf 'ha-kiosk: error: %s\n' "$*" >&2
  exit 1
}

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '  + %s\n' "$*"
    return 0
  fi
  "$@"
}

require_root() {
  if [ "$DRY_RUN" -eq 1 ]; then
    return 0
  fi
  [ "$(id -u)" -eq 0 ] || die "run as root, for example: sudo bash $0"
}

require_debian() {
  command -v apt-get >/dev/null 2>&1 ||
    die "this installer supports Debian-based systems only; apt-get was not found"
}

validate_rotation() {
  case "${1:-}" in
    0 | 90 | 180 | 270) return 0 ;;
    *) return 1 ;;
  esac
}

validate_backend() {
  case "${1:-}" in
    kms | x11) return 0 ;;
    *) return 1 ;;
  esac
}

find_cmdline_file() {
  local prefix="${1:-}" candidate
  for candidate in /boot/firmware/cmdline.txt /boot/cmdline.txt; do
    if [ -f "${prefix}${candidate}" ]; then
      printf '%s\n' "${prefix}${candidate}"
      return 0
    fi
  done
  return 1
}

# Keep the kernel command line on one line and replace any existing
# consoleblank= value. Kernel parameters are space separated.
ensure_consoleblank() {
  local file="$1" content tmp
  [ -f "$file" ] || die "kernel command line not found: $file"
  if grep -q 'consoleblank=0' "$file"; then
    log "console blanking is already disabled in $file"
    return 0
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    log "would set consoleblank=0 in $file"
    return 0
  fi
  content=$(tr -d '\n' <"$file")
  content=$(printf '%s' "$content" |
    sed -E 's/[[:space:]]*consoleblank=[0-9]+//g; s/[[:space:]]+/ /g; s/^ //; s/ $//')
  tmp=$(mktemp "${file}.XXXXXX") || die "cannot create a temporary file next to $file"
  printf '%s consoleblank=0\n' "$content" >"$tmp"
  # /boot/firmware is often vfat, where chmod and chown are not supported.
  chmod --reference="$file" "$tmp" 2>/dev/null || true
  if [ "$(id -u)" -eq 0 ]; then
    chown --reference="$file" "$tmp" 2>/dev/null || true
  fi
  mv -f "$tmp" "$file" || die "cannot replace $file"
  log "set consoleblank=0 in $file"
}

ensure_group() {
  local group="$1"
  if getent group "$group" >/dev/null 2>&1; then
    return 0
  fi
  log "creating system group $group"
  run groupadd --system "$group"
}

ensure_system_user() {
  local user="$1" home="$2" group="${3:-$1}"
  if id -u "$user" >/dev/null 2>&1; then
    return 0
  fi
  log "creating system user $user"
  run useradd --system --gid "$group" --no-create-home --home-dir "$home" \
    --shell /usr/sbin/nologin "$user"
}

add_user_to_group() {
  local user="$1" group="$2"
  if id -nG "$user" 2>/dev/null | tr ' ' '\n' | grep -qx "$group"; then
    return 0
  fi
  log "adding $user to group $group"
  run usermod -aG "$group" "$user"
}

# write_rotation_dropin <drop-in-dir> <degrees>
write_rotation_dropin() {
  local dir="$1" rotation="$2" file
  validate_rotation "$rotation" || die "invalid rotation: $rotation (use 0, 90, 180, or 270)"
  file="$dir/10-kiosk-rotation.conf"
  if [ "$DRY_RUN" -eq 1 ]; then
    log "would write $file with SLINT_KMS_ROTATION=$rotation"
    return 0
  fi
  install -d -m 0755 "$dir"
  printf '[Service]\nEnvironment=SLINT_KMS_ROTATION=%s\n' "$rotation" >"$file"
  chmod 0644 "$file"
  log "wrote $file"
}

install_file() {
  local src="$1" dst="$2" mode="$3" owner="$4" group="$5"
  if [ ! -f "$src" ]; then
    if [ "$DRY_RUN" -eq 1 ]; then
      warn "would install missing file $src to $dst"
      return 0
    fi
    die "file not found: $src"
  fi
  run install -D -m "$mode" -o "$owner" -g "$group" "$src" "$dst"
}

# Replace a running binary atomically: install() would truncate the target and
# fail with ETXTBSY while the service is still running, while rename() only
# swaps the directory entry and leaves the old inode to the running process.
install_binary() {
  local src="$1" dst="$2" owner="${3:-root}" group="${4:-root}" tmp
  if [ ! -f "$src" ]; then
    if [ "$DRY_RUN" -eq 1 ]; then
      warn "would install missing binary $src to $dst"
      return 0
    fi
    die "binary not found: $src"
  fi
  if [ "$DRY_RUN" -eq 1 ]; then
    log "would install $src to $dst"
    return 0
  fi
  tmp=$(mktemp "${dst}.XXXXXX") || die "cannot create a temporary file next to $dst"
  if ! install -m 0755 -o "$owner" -g "$group" "$src" "$tmp"; then
    rm -f "$tmp"
    die "failed to install $src"
  fi
  mv -f "$tmp" "$dst" || {
    rm -f "$tmp"
    die "cannot replace $dst"
  }
}

apt_install() {
  [ "$#" -gt 0 ] || return 0
  if [ "$DRY_RUN" -eq 1 ]; then
    log "would install packages: $*"
    return 0
  fi
  DEBIAN_FRONTEND=noninteractive apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$@"
}

enable_unit_if_present() {
  local unit="$1"
  if systemctl cat "$unit" >/dev/null 2>&1; then
    run systemctl enable --now "$unit"
  elif [ "$DRY_RUN" -eq 1 ]; then
    run systemctl enable --now "$unit"
  else
    warn "$unit is not installed; skipping enable"
  fi
}
