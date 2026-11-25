#!/usr/bin/env bash
# Build a symbol-enabled desktop binary and collect x86_64 allocation evidence.
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/perf-heaptrack.sh [-- <ha-kiosk arguments>]

Requires Linux x86_64, Cargo, heaptrack, and heaptrack_print. The profiling
build and reports default to target/heaptrack. Override HEAPTRACK_TARGET_DIR
or HEAPTRACK_OUTPUT_DIR when a separate location is needed.
EOF
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi
if [[ $# -gt 0 && "$1" != "--" ]]; then
  usage >&2
  exit 2
fi
if [[ "${1:-}" == "--" ]]; then
  shift
fi

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "error: heaptrack profiling requires a Linux x86_64 host" >&2
  exit 1
fi

for tool in cargo heaptrack heaptrack_print; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool not found: $tool" >&2
    exit 1
  fi
done

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"
target_dir=${HEAPTRACK_TARGET_DIR:-"$root/target/heaptrack"}
output_root=${HEAPTRACK_OUTPUT_DIR:-"$target_dir/runs"}

CARGO_TARGET_DIR="$target_dir" \
CARGO_PROFILE_RELEASE_STRIP=false \
CARGO_PROFILE_RELEASE_DEBUG=1 \
  cargo build --locked --release --features desktop

binary="$target_dir/release/ha-kiosk"
if [[ ! -x "$binary" ]]; then
  echo "error: profiling build did not produce $binary" >&2
  exit 1
fi

mkdir -p "$output_root"
run_dir=$(mktemp -d "$output_root/run-$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX")
output="$run_dir/ha-kiosk"
report="$output.zst"
summary="$output.txt"

status=0
heaptrack -o "$output" "$binary" "$@" || status=$?

if [[ ! -f "$report" ]]; then
  echo "error: no heaptrack report was produced at $report" >&2
  if [[ $status -eq 0 ]]; then
    status=1
  fi
  exit "$status"
fi

print_status=0
heaptrack_print "$report" > "$summary" || print_status=$?
if [[ $print_status -ne 0 ]]; then
  echo "error: heaptrack_print failed for $report" >&2
  if [[ $status -eq 0 ]]; then
    status=$print_status
  fi
fi

echo "heaptrack report: $report"
echo "allocation summary: $summary"
if [[ $status -ne 0 ]]; then
  echo "warning: the profiled process exited with status $status" >&2
fi
exit "$status"
