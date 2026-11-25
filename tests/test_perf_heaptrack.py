#!/usr/bin/env python3
"""End-to-end tests for scripts/perf-heaptrack.sh using external tool stubs."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "perf-heaptrack.sh"

UNAME_STUB = """#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  -s) printf '%s\\n' "${STUB_OS:-Linux}" ;;
  -m) printf '%s\\n' "${STUB_ARCH:-x86_64}" ;;
  *) exit 2 ;;
esac
"""

CARGO_STUB = """#!/usr/bin/env bash
set -euo pipefail
{
  printf 'argv:'
  printf ' <%s>' "$@"
  printf '\\n'
  printf 'CARGO_TARGET_DIR=%s\\n' "${CARGO_TARGET_DIR:-}"
  printf 'CARGO_PROFILE_RELEASE_STRIP=%s\\n' "${CARGO_PROFILE_RELEASE_STRIP:-}"
  printf 'CARGO_PROFILE_RELEASE_DEBUG=%s\\n' "${CARGO_PROFILE_RELEASE_DEBUG:-}"
} >> "${STUB_LOG:?}"
mkdir -p "${CARGO_TARGET_DIR:?}/release"
printf '#!/usr/bin/env bash\\nexit "${STUB_APP_STATUS:-0}"\\n' > "${CARGO_TARGET_DIR}/release/ha-kiosk"
chmod +x "${CARGO_TARGET_DIR}/release/ha-kiosk"
"""

HEAPTRACK_STUB = """#!/usr/bin/env bash
set -euo pipefail
original_args=("$@")
output=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o|--output) output="$2"; shift 2 ;;
    *) shift ;;
  esac
done
{
  printf 'argv:'
  printf ' <%s>' "${original_args[@]}"
  printf '\\n'
} >> "${STUB_LOG:?}"
if [ "${STUB_WRITE_REPORT:-1}" = 1 ]; then
  printf 'compressed heaptrack data\\n' > "${output}.zst"
fi
exit "${STUB_HEAPTRACK_STATUS:-0}"
"""

HEAPTRACK_PRINT_STUB = """#!/usr/bin/env bash
set -euo pipefail
printf 'print: <%s>\\n' "$*" >> "${STUB_LOG:?}"
printf 'heaptrack summary for %s\\n' "$1"
"""


class HeaptrackRunnerTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.bin_dir = self.tmp / "bin"
        self.bin_dir.mkdir()
        self.log = self.tmp / "stub.log"
        self.target_dir = self.tmp / "target"
        self.output_dir = self.tmp / "runs"
        self.write_stub("uname", UNAME_STUB)
        self.write_stub("cargo", CARGO_STUB)
        self.write_stub("heaptrack", HEAPTRACK_STUB)
        self.write_stub("heaptrack_print", HEAPTRACK_PRINT_STUB)

    def write_stub(self, name: str, content: str) -> None:
        path = self.bin_dir / name
        path.write_text(content, encoding="utf-8")
        path.chmod(0o755)

    def run_runner(self, **overrides: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env.update(
            {
                "PATH": f"{self.bin_dir}{os.pathsep}{env.get('PATH', '')}",
                "STUB_LOG": str(self.log),
                "HEAPTRACK_TARGET_DIR": str(self.target_dir),
                "HEAPTRACK_OUTPUT_DIR": str(self.output_dir),
            }
        )
        env.update(overrides)
        return subprocess.run(
            [str(SCRIPT), "--", "--config", "config with spaces.toml"],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def read_log(self) -> str:
        return self.log.read_text(encoding="utf-8") if self.log.exists() else ""

    def artifacts(self) -> list[Path]:
        return sorted(self.output_dir.glob("*/ha-kiosk.zst"))

    def test_linux_x86_64_builds_profiling_binary_and_summarizes_report(self):
        result = self.run_runner()
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.read_log()
        self.assertIn("argv: <build> <--locked> <--release> <--features> <desktop>", log)
        self.assertIn(f"CARGO_TARGET_DIR={self.target_dir}", log)
        self.assertIn("CARGO_PROFILE_RELEASE_STRIP=false", log)
        self.assertIn("CARGO_PROFILE_RELEASE_DEBUG=1", log)
        self.assertIn("<--config> <config with spaces.toml>", log)
        self.assertEqual(len(self.artifacts()), 1)
        summary = self.artifacts()[0].with_suffix(".txt")
        self.assertEqual(
            summary.read_text(encoding="utf-8"),
            f"heaptrack summary for {self.artifacts()[0]}\n",
        )
        self.assertIn(str(self.artifacts()[0]), result.stdout)
        self.assertIn(str(summary), result.stdout)

    def test_macos_fails_before_cargo_runs(self):
        result = self.run_runner(STUB_OS="Darwin")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Linux x86_64", result.stderr)
        self.assertNotIn("argv: <build>", self.read_log())

    def test_non_x86_64_linux_fails_before_cargo_runs(self):
        result = self.run_runner(STUB_ARCH="aarch64")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Linux x86_64", result.stderr)
        self.assertNotIn("argv: <build>", self.read_log())

    def test_missing_heaptrack_fails_before_cargo_runs(self):
        (self.bin_dir / "heaptrack").unlink()
        result = self.run_runner()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("heaptrack", result.stderr)
        self.assertNotIn("argv: <build>", self.read_log())

    def test_nonzero_profiled_process_status_is_preserved_after_summary(self):
        result = self.run_runner(STUB_APP_STATUS="9", STUB_HEAPTRACK_STATUS="9")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(len(self.artifacts()), 1)
        self.assertTrue(self.artifacts()[0].with_suffix(".txt").exists())

    def test_missing_report_fails_instead_of_claiming_success(self):
        result = self.run_runner(STUB_WRITE_REPORT="0")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("no heaptrack report", result.stderr)
        self.assertEqual(self.artifacts(), [])


if __name__ == "__main__":
    unittest.main()
