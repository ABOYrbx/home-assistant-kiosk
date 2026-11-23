#!/usr/bin/env python3
"""Tests for the reproducible release build tooling.

The build script is exercised end to end with a stub ``cross`` on PATH. The
stub records the arguments and environment it receives and writes a dummy
binary, so the test verifies the script's real logic without Docker or a Rust
toolchain.
"""

from __future__ import annotations

import hashlib
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILD_ALL = ROOT / "scripts" / "build-all.sh"
CROSS_TOML = ROOT / "Cross.toml"

TARGETS = ("aarch64-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf", "x86_64-unknown-linux-gnu")

STUB_CROSS = """#!/usr/bin/env bash
set -euo pipefail
log="${STUB_LOG:?}"
printf 'argv: %s\\n' "$*" >> "$log"
env | LC_ALL=C sort | sed 's/^/env: /' >> "$log"
target=""
target_dir=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --target) target="$2"; shift 2 ;;
        --target-dir) target_dir="$2"; shift 2 ;;
        *) shift ;;
    esac
done
if [ -z "$target" ] || [ -z "$target_dir" ]; then
    echo "stub cross: missing --target or --target-dir" >&2
    exit 2
fi
out="${target_dir}/${target}/release"
mkdir -p "$out"
printf 'ha-kiosk %s %s\\n' "$target" "${SOURCE_DATE_EPOCH:-unset}" > "$out/ha-kiosk"
"""


class BuildAllTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        self.cross = bin_dir / "cross"
        self.cross.write_text(STUB_CROSS, encoding="utf-8")
        self.cross.chmod(0o755)
        self.log = self.tmp / "stub.log"
        self.dist = self.tmp / "dist"
        self.target_dir = self.tmp / "target"

    def run_build(self, **overrides: str) -> subprocess.CompletedProcess[str]:
        env = {
            "PATH": f"{self.cross.parent}{os.pathsep}{os.environ.get('PATH', '')}",
            "HOME": os.environ.get("HOME", str(self.tmp)),
            "CROSS": str(self.cross),
            "VERSION": "9.9.9",
            "TARGETS": "aarch64-unknown-linux-gnu armv7-unknown-linux-gnueabihf",
            "SOURCE_DATE_EPOCH": "1700000000",
            "DIST_DIR": str(self.dist),
            "CARGO_TARGET_DIR": str(self.target_dir),
            "STUB_LOG": str(self.log),
        }
        env.update(overrides)
        return subprocess.run(
            [str(BUILD_ALL)],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

    def artifact_names(self) -> list[str]:
        return sorted(path.name for path in self.dist.glob("ha-kiosk-*"))

    def read_checksums(self) -> list[str]:
        return (self.dist / "SHA256SUMS").read_text(encoding="utf-8").splitlines()

    def test_builds_every_target_with_reproducible_flags(self) -> None:
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.artifact_names(),
            [
                "ha-kiosk-9.9.9-aarch64-unknown-linux-gnu",
                "ha-kiosk-9.9.9-armv7-unknown-linux-gnueabihf",
            ],
        )
        for name in self.artifact_names():
            self.assertEqual((self.dist / name).stat().st_mode & 0o777, 0o755)

        log = self.log.read_text(encoding="utf-8")
        for target in ("aarch64-unknown-linux-gnu", "armv7-unknown-linux-gnueabihf"):
            self.assertIn(
                f"argv: build --locked --release --target {target} "
                f"--target-dir {self.target_dir} --no-default-features --features kiosk",
                log,
            )
        for expected in (
            "env: CARGO_INCREMENTAL=0",
            "env: CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1",
            "env: CARGO_PROFILE_RELEASE_LTO=fat",
            "env: CARGO_PROFILE_RELEASE_OPT_LEVEL=s",
            "env: CARGO_PROFILE_RELEASE_PANIC=abort",
            "env: CARGO_PROFILE_RELEASE_STRIP=symbols",
            "env: LC_ALL=C",
            "env: SOURCE_DATE_EPOCH=1700000000",
            "env: TZ=UTC",
            f"--remap-path-prefix={ROOT}=/build/ha-kiosk",
        ):
            self.assertIn(expected, log)

    def test_checksums_match_the_artifacts(self) -> None:
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = self.read_checksums()
        self.assertEqual([line.split("  ", 1)[1] for line in lines], self.artifact_names())
        for line in lines:
            digest, name = line.split("  ", 1)
            self.assertEqual(digest, hashlib.sha256((self.dist / name).read_bytes()).hexdigest())

    def test_second_build_is_byte_identical(self) -> None:
        first = self.run_build()
        self.assertEqual(first.returncode, 0, first.stderr)
        first_sums = self.read_checksums()

        second = self.run_build()
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(self.read_checksums(), first_sums)

    def test_build_timestamp_changes_the_output(self) -> None:
        first = self.run_build()
        self.assertEqual(first.returncode, 0, first.stderr)
        first_sums = self.read_checksums()

        second = self.run_build(SOURCE_DATE_EPOCH="1700000001")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertNotEqual(self.read_checksums(), first_sums)
        self.assertIn("env: SOURCE_DATE_EPOCH=1700000001", self.log.read_text(encoding="utf-8"))

    def test_target_subset_builds_only_that_target(self) -> None:
        result = self.run_build(TARGETS="x86_64-unknown-linux-gnu")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.artifact_names(), ["ha-kiosk-9.9.9-x86_64-unknown-linux-gnu"])
        self.assertEqual(len(self.read_checksums()), 1)

    def test_stale_artifacts_and_checksums_are_removed(self) -> None:
        self.dist.mkdir()
        (self.dist / "ha-kiosk-0.0.0-riscv64gc-unknown-linux-gnu").write_bytes(b"old")
        (self.dist / "SHA256SUMS").write_text("stale\n", encoding="utf-8")

        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("ha-kiosk-0.0.0-riscv64gc-unknown-linux-gnu", self.artifact_names())
        self.assertNotIn("stale", self.read_checksums())


class CrossConfigTest(unittest.TestCase):
    def test_every_target_is_pinned_by_digest(self) -> None:
        text = CROSS_TOML.read_text(encoding="utf-8")
        for target in TARGETS:
            match = re.search(
                rf'\[target\.{re.escape(target)}\]\s*\nimage = "([^"]+)"',
                text,
            )
            self.assertIsNotNone(match, f"missing image for {target}")
            image = match.group(1)
            self.assertIn(target, image)
            self.assertRegex(image, r"@sha256:[0-9a-f]{64}$")

    def test_pre_build_installs_the_link_dependencies(self) -> None:
        text = CROSS_TOML.read_text(encoding="utf-8")
        match = re.search(r"pre-build = \[(.*?)\]", text, re.S)
        self.assertIsNotNone(match, "Cross.toml has no [build] pre-build list")
        pre_build = match.group(1)
        for package in (
            "libfontconfig1-dev",
            "libinput-dev",
            "libudev-dev",
            "libxkbcommon-dev",
            "libseat-dev",
        ):
            self.assertIn(package, pre_build)


if __name__ == "__main__":
    unittest.main()
