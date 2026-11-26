#!/usr/bin/env python3
"""Tests for the release preflight command."""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "check-release.sh"

STUB = """#!/bin/bash
set -euo pipefail
printf 'argv: %s\\n' "$*" >> "${STUB_LOG:?}"
if [ -n "${STUB_FAIL:-}" ] && printf '%s' "$*" | grep -Fq -- "$STUB_FAIL"; then
    exit 7
fi
exit 0
"""


class CheckReleaseTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        for name in ("cargo", "python3", "bash", "sha256sum"):
            stub = bin_dir / name
            stub.write_text(STUB, encoding="utf-8")
            stub.chmod(0o755)
        self.log = self.tmp / "stub.log"
        self.env = {
            "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
            "HOME": os.environ.get("HOME", str(self.tmp)),
            "STUB_LOG": str(self.log),
        }

    def run_script(self, *args: str, **overrides: str) -> subprocess.CompletedProcess[str]:
        env = dict(self.env)
        env.update(overrides)
        return subprocess.run(
            [str(SCRIPT), *args],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def argv_lines(self) -> list[str]:
        if not self.log.exists():
            return []
        return [line for line in self.log.read_text(encoding="utf-8").splitlines() if line.startswith("argv: ")]

    def test_default_command_order(self) -> None:
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.argv_lines(),
            [
                "argv: fmt --check",
                "argv: clippy --all-targets -- -D warnings",
                "argv: test",
                "argv: audit",
                "argv: scripts/check-docs.py",
                "argv: -m unittest discover -s tests -p test_*.py",
                "argv: tests/deploy_scripts_test.sh",
            ],
        )

    def test_fail_fast_stops_after_first_failure(self) -> None:
        result = self.run_script(STUB_FAIL="fmt --check")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.argv_lines(), ["argv: fmt --check"])

    def test_unknown_option_is_rejected(self) -> None:
        result = self.run_script("--bogus")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.argv_lines(), [])

    def test_with_artifacts_adds_build_and_checksum_verification(self) -> None:
        dist = ROOT / "dist"
        dist.mkdir(exist_ok=True)
        (dist / "SHA256SUMS").write_text("", encoding="utf-8")
        self.addCleanup(shutil.rmtree, dist, ignore_errors=True)
        result = self.run_script("--with-artifacts")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.argv_lines(),
            [
                "argv: fmt --check",
                "argv: clippy --all-targets -- -D warnings",
                "argv: test",
                "argv: audit",
                "argv: scripts/check-docs.py",
                "argv: -m unittest discover -s tests -p test_*.py",
                "argv: tests/deploy_scripts_test.sh",
                "argv: scripts/build-all.sh",
                "argv: -c SHA256SUMS",
            ],
        )


if __name__ == "__main__":
    unittest.main()
