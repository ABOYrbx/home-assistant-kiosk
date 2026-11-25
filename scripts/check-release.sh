#!/bin/bash
# Run the automated gates from docs/release-checklist.md.
#
# Usage:
#   scripts/check-release.sh [--with-artifacts]
#
# The default mode runs the fast documentation, Rust, audit, Python, and
# deployment gates. --with-artifacts also builds every supported target and
# verifies SHA256SUMS. This script does not replace hardware, soak, license,
# or publication sign-off.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

WITH_ARTIFACTS=0
for arg in "$@"; do
    case "$arg" in
        --with-artifacts)
            WITH_ARTIFACTS=1
            ;;
        -h|--help)
            printf 'Usage: %s [--with-artifacts]\n' "$0"
            exit 0
            ;;
        *)
            printf 'Unknown option: %s\n' "$arg" >&2
            printf 'Usage: %s [--with-artifacts]\n' "$0" >&2
            exit 2
            ;;
    esac
done

run() {
    printf '+ %s\n' "$*"
    "$@"
}

run cargo fmt --check
run cargo clippy --all-targets -- -D warnings
run cargo test
run cargo audit
run python3 scripts/check-docs.py
run python3 -m unittest discover -s tests -p 'test_*.py'
run bash tests/deploy_scripts_test.sh

if [ "$WITH_ARTIFACTS" -eq 1 ]; then
    run bash scripts/build-all.sh
    (
        mkdir -p dist
        cd dist
        run sha256sum -c SHA256SUMS
    )
fi

printf 'Release preflight passed\n'
