#!/usr/bin/env python3
"""Tests for the Pi memory evidence CLI."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "perf-memory.py"

_SPEC = importlib.util.spec_from_file_location("perf_memory", SCRIPT)
assert _SPEC is not None and _SPEC.loader is not None
perf_memory = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(perf_memory)

TIME_REPORT = """\
Command being timed: "/usr/local/bin/ha-kiosk --config /etc/ha-kiosk/config.toml"
\tUser time (seconds): 1.23
\tSystem time (seconds): 0.45
\tPercent of CPU: 8
\tElapsed (wall clock) time (h:mm:ss or m:ss): 0:20.00
\tMaximum resident set size (kbytes): 81920
\tExit status: 0
"""

SYSTEMD_REPORT = """\
MainPID=1234
MemoryCurrent=83886080
MemoryPeak=94371840
"""


class TimeReportTest(unittest.TestCase):
    def test_extracts_peak_rss_and_exit_status(self):
        result = perf_memory.parse_time_report(TIME_REPORT)
        self.assertEqual(
            result,
            {
                "peak_rss_kib": 81920,
                "peak_rss_mib": 80.0,
                "exit_status": 0,
            },
        )

    def test_preserves_nonzero_exit_status(self):
        report = TIME_REPORT.replace("Exit status: 0", "Exit status: 7")
        self.assertEqual(perf_memory.parse_time_report(report)["exit_status"], 7)

    def test_missing_peak_rss_fails_instead_of_recording_zero(self):
        report = TIME_REPORT.replace(
            "\tMaximum resident set size (kbytes): 81920\n", ""
        )
        with self.assertRaisesRegex(ValueError, "Maximum resident set size"):
            perf_memory.parse_time_report(report)

    def test_unavailable_peak_rss_fails(self):
        report = TIME_REPORT.replace("81920", "unavailable")
        with self.assertRaisesRegex(ValueError, "Maximum resident set size"):
            perf_memory.parse_time_report(report)


class SystemdReportTest(unittest.TestCase):
    def test_extracts_current_and_peak_cgroup_memory(self):
        self.assertEqual(
            perf_memory.parse_systemd_show(SYSTEMD_REPORT),
            {
                "main_pid": 1234,
                "memory_current_bytes": 83886080,
                "memory_current_mib": 80.0,
                "memory_peak_bytes": 94371840,
                "memory_peak_mib": 90.0,
            },
        )

    def test_missing_memory_property_fails(self):
        report = SYSTEMD_REPORT.replace("MemoryPeak=94371840\n", "")
        with self.assertRaisesRegex(ValueError, "MemoryPeak"):
            perf_memory.parse_systemd_show(report)

    def test_unavailable_memory_property_fails(self):
        report = SYSTEMD_REPORT.replace("MemoryCurrent=83886080", "MemoryCurrent=[not set]")
        with self.assertRaisesRegex(ValueError, "MemoryCurrent"):
            perf_memory.parse_systemd_show(report)

    def test_stopped_service_fails(self):
        report = SYSTEMD_REPORT.replace("MainPID=1234", "MainPID=0")
        with self.assertRaisesRegex(ValueError, "MainPID"):
            perf_memory.parse_systemd_show(report)


class MemoryCliTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.output = Path(self.tmp.name) / "evidence" / "pi-memory.json"

    def read_json(self):
        return json.loads(self.output.read_text(encoding="utf-8"))

    def test_time_writes_json_and_raw_report_and_returns_target_status(self):
        report = TIME_REPORT.replace("Exit status: 0", "Exit status: 7")
        completed = subprocess.CompletedProcess(
            args=["/usr/bin/time", "-v"], returncode=7, stdout="", stderr=report
        )
        command = ["/usr/local/bin/ha-kiosk", "--config", "/etc/ha-kiosk/config.toml"]
        with patch.object(perf_memory.subprocess, "run", return_value=completed) as run:
            status = perf_memory.main(["time", "--output", str(self.output), "--", *command])

        self.assertEqual(status, 7)
        run.assert_called_once()
        args, kwargs = run.call_args
        self.assertEqual(args[0], ["/usr/bin/time", "-v", *command])
        self.assertEqual(kwargs["env"]["LC_ALL"], "C")
        self.assertTrue(kwargs["capture_output"])
        self.assertTrue(kwargs["text"])
        self.assertFalse(kwargs["check"])

        record = self.read_json()
        self.assertEqual(record["schema_version"], 1)
        self.assertEqual(record["method"], "gnu-time")
        self.assertEqual(record["command"], command)
        self.assertEqual(record["exit_status"], 7)
        self.assertEqual(record["peak_rss_kib"], 81920)
        self.assertEqual(record["peak_rss_mib"], 80.0)
        self.assertTrue(record["recorded_at"].endswith("Z"))
        datetime.fromisoformat(record["recorded_at"].replace("Z", "+00:00"))
        self.assertEqual(self.output.with_suffix(".time.txt").read_text(encoding="utf-8"), report)

    def test_systemd_writes_cgroup_memory_evidence(self):
        completed = subprocess.CompletedProcess(
            args=["systemctl", "show"], returncode=0, stdout=SYSTEMD_REPORT, stderr=""
        )
        with patch.object(perf_memory.subprocess, "run", return_value=completed) as run:
            status = perf_memory.main(
                ["systemd", "--unit", "ha-kiosk", "--output", str(self.output)]
            )

        self.assertEqual(status, 0)
        args, _ = run.call_args
        self.assertEqual(
            args[0],
            [
                "systemctl",
                "show",
                "ha-kiosk",
                "--property=MainPID",
                "--property=MemoryCurrent",
                "--property=MemoryPeak",
            ],
        )
        record = self.read_json()
        self.assertEqual(record["method"], "systemd-cgroup")
        self.assertEqual(record["unit"], "ha-kiosk")
        self.assertEqual(record["main_pid"], 1234)
        self.assertEqual(record["memory_current_bytes"], 83886080)
        self.assertEqual(record["memory_current_mib"], 80.0)
        self.assertEqual(record["memory_peak_bytes"], 94371840)
        self.assertEqual(record["memory_peak_mib"], 90.0)

    def test_systemd_command_failure_does_not_write_evidence(self):
        completed = subprocess.CompletedProcess(
            args=["systemctl", "show"],
            returncode=1,
            stdout="",
            stderr="Failed to get properties: Unit ha-kiosk.service not found.",
        )
        with patch.object(perf_memory.subprocess, "run", return_value=completed):
            status = perf_memory.main(
                ["systemd", "--unit", "ha-kiosk", "--output", str(self.output)]
            )
        self.assertEqual(status, 1)
        self.assertFalse(self.output.exists())

    def test_systemd_invalid_properties_do_not_write_evidence(self):
        completed = subprocess.CompletedProcess(
            args=["systemctl", "show"],
            returncode=0,
            stdout=SYSTEMD_REPORT.replace("MemoryPeak=94371840", "MemoryPeak=[not set]"),
            stderr="",
        )
        with patch.object(perf_memory.subprocess, "run", return_value=completed):
            status = perf_memory.main(
                ["systemd", "--unit", "ha-kiosk", "--output", str(self.output)]
            )
        self.assertEqual(status, 1)
        self.assertFalse(self.output.exists())


if __name__ == "__main__":
    unittest.main()
