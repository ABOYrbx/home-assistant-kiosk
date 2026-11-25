#!/usr/bin/env python3
"""Tests for scripts/perf-record.py."""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "perf-record.py"

_SPEC = importlib.util.spec_from_file_location("perf_record", SCRIPT)
assert _SPEC is not None and _SPEC.loader is not None
perf_record = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(perf_record)

PIDSTAT_HEADER = """Linux 6.6.31-rpt-rpi-v8 (pi-zero) 11/24/2026  _aarch64_  (4 CPU)

10:00:00 AM UID PID %usr %system %soft %irq %guest %CPU CPU Command
"""

PIDSTAT_ROWS = """10:01:00 AM 1000 321 0.10 0.10 0.00 0.00 0.00 0.20 0 ha-kiosk
10:02:00 AM 1000 321 0.20 0.20 0.00 0.00 0.00 0.40 0 ha-kiosk
10:03:00 AM 1000 321 0.30 0.30 0.00 0.00 0.00 0.60 0 ha-kiosk
10:04:00 AM 1000 321 0.40 0.40 0.00 0.00 0.00 0.80 0 ha-kiosk
10:05:00 AM 1000 321 0.50 0.50 0.00 0.00 0.00 1.00 0 ha-kiosk
10:06:00 AM 1000 321 0.60 0.60 0.00 0.00 0.00 1.20 0 ha-kiosk
10:07:00 AM 1000 321 0.70 0.70 0.00 0.00 0.00 1.40 0 ha-kiosk
10:08:00 AM 1000 321 0.80 0.80 0.00 0.00 0.00 1.60 0 ha-kiosk
10:09:00 AM 1000 321 0.90 0.90 0.00 0.00 0.00 1.80 0 ha-kiosk
10:10:00 AM 1000 321 1.00 1.00 0.00 0.00 0.00 2.00 0 ha-kiosk
Average: 1000 321 0.55 0.55 0.00 0.00 0.00 1.10 0 ha-kiosk
"""


class RssParserTest(unittest.TestCase):
    def test_systemd_memory_current_is_converted_to_mib(self):
        text = "MemoryCurrent=104857600\nMemoryPeak=125829120\n"
        self.assertEqual(perf_record.parse_rss(text), 100.0)

    def test_missing_memory_current_fails(self):
        with self.assertRaisesRegex(ValueError, "MemoryCurrent"):
            perf_record.parse_rss("MemoryPeak=125829120\n")

    def test_malformed_memory_current_fails(self):
        with self.assertRaisesRegex(ValueError, "MemoryCurrent"):
            perf_record.parse_rss("MemoryCurrent=unknown\n")

    def test_zero_memory_current_fails(self):
        with self.assertRaisesRegex(ValueError, "MemoryCurrent"):
            perf_record.parse_rss("MemoryCurrent=0\n")


class FirstFrameParserTest(unittest.TestCase):
    def test_first_drm_atomic_commit_after_exec_is_the_first_frame(self):
        text = """321 1763980000.100000 execve("/usr/local/bin/ha-kiosk", ["ha-kiosk"], 0x7ffd) = 0
321 1763980000.900000 ioctl(3, 0xc01864c1, 0x7ffd) = 0
321 1763980001.850000 ioctl(3, DRM_IOCTL_MODE_ATOMIC, 0x7ffd) = 0
321 1763980003.000000 ioctl(3, DRM_IOCTL_MODE_ATOMIC, 0x7ffd) = 0
"""
        self.assertEqual(perf_record.parse_first_frame(text), 1750.0)

    def test_epoch_timestamps_are_safe_across_midnight(self):
        text = """321 1763980799.900000 execve("/usr/local/bin/ha-kiosk", ["ha-kiosk"], 0x7ffd) = 0
321 1763980801.700000 ioctl(3, DRM_IOCTL_MODE_PAGE_FLIP, 0x7ffd) = 0
"""
        self.assertEqual(perf_record.parse_first_frame(text), 1800.0)

    def test_missing_process_start_fails(self):
        text = "321 1763980001.850000 ioctl(3, DRM_IOCTL_MODE_ATOMIC, 0x7ffd) = 0\n"
        with self.assertRaisesRegex(ValueError, "execve"):
            perf_record.parse_first_frame(text)

    def test_missing_drm_presentation_fails(self):
        text = """321 1763980000.100000 execve("/usr/local/bin/ha-kiosk", ["ha-kiosk"], 0x7ffd) = 0
321 1763980000.900000 ioctl(3, 0xc01864c1, 0x7ffd) = 0
"""
        with self.assertRaisesRegex(ValueError, "DRM"):
            perf_record.parse_first_frame(text)

    def test_malformed_timestamp_fails(self):
        text = """321 not-a-time execve("/usr/local/bin/ha-kiosk", ["ha-kiosk"], 0x7ffd) = 0
321 1763980001.850000 ioctl(3, DRM_IOCTL_MODE_ATOMIC, 0x7ffd) = 0
"""
        with self.assertRaisesRegex(ValueError, "timestamp"):
            perf_record.parse_first_frame(text)


class IdleCpuParserTest(unittest.TestCase):
    def test_ten_sample_mean_ignores_headers_and_average_row(self):
        self.assertEqual(
            perf_record.parse_idle_cpu(PIDSTAT_HEADER + PIDSTAT_ROWS),
            1.1,
        )

    def test_partial_sample_set_fails(self):
        rows = "\n".join(PIDSTAT_ROWS.splitlines()[:9]) + "\n"
        with self.assertRaisesRegex(ValueError, "10"):
            perf_record.parse_idle_cpu(PIDSTAT_HEADER + rows)

    def test_custom_sample_count_is_honored(self):
        rows = "\n".join(PIDSTAT_ROWS.splitlines()[:2]) + "\n"
        self.assertEqual(perf_record.parse_idle_cpu(rows, expected_samples=2), 0.3)

    def test_malformed_cpu_value_fails(self):
        rows = PIDSTAT_ROWS.replace("0.20 0 ha-kiosk", "unknown 0 ha-kiosk", 1)
        with self.assertRaisesRegex(ValueError, "CPU"):
            perf_record.parse_idle_cpu(PIDSTAT_HEADER + rows)


class MetadataParserTest(unittest.TestCase):
    def test_os_pretty_name_is_used(self):
        text = 'PRETTY_NAME="Raspberry Pi OS Bookworm (64-bit)"\nNAME="Raspberry Pi OS"\n'
        self.assertEqual(
            perf_record.parse_os(text),
            "Raspberry Pi OS Bookworm (64-bit)",
        )

    def test_missing_os_pretty_name_fails(self):
        with self.assertRaisesRegex(ValueError, "PRETTY_NAME"):
            perf_record.parse_os('NAME="Raspberry Pi OS"\n')

    def test_zero_2_w_model_is_normalized_and_nul_is_removed(self):
        self.assertEqual(
            perf_record.parse_device("Raspberry Pi Zero 2 W Rev 1.0\0"),
            "raspberry-pi-zero-2-w",
        )

    def test_other_device_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Pi Zero 2 W"):
            perf_record.parse_device("Raspberry Pi 3 Model B Plus Rev 1.3\0")

    def test_empty_device_model_fails(self):
        with self.assertRaisesRegex(ValueError, "device model"):
            perf_record.parse_device("\0\n")


BASE_RECORD = {
    "schema_version": 1,
    "device": "raspberry-pi-zero-2-w",
    "os": "Raspberry Pi OS Bookworm 64-bit",
    "build": "release",
    "features": "kiosk",
    "recorded_at": "2026-11-24T10:00:00Z",
    "metrics": {
        "rss_idle_mb": 1.0,
        "first_frame_ms": 1.0,
        "idle_cpu_percent": 1.0,
        "soak_hours": 24.0,
        "soak_rss_growth_mb": 2.0,
        "throttled": False,
    },
}

RSS_TEXT = "MainPID=1234\nMemoryCurrent=94371840\nMemoryPeak=104857600\n"
FIRST_FRAME_TEXT = """\
321 1763980000.100000 execve("/usr/local/bin/ha-kiosk", ["ha-kiosk"], 0x7ffd) = 0
321 1763980001.100000 ioctl(3, DRM_IOCTL_MODE_ATOMIC, 0x7ffd) = 0
"""
DEVICE_TEXT = "Raspberry Pi Zero 2 W Rev 1.0\0"
OS_TEXT = 'PRETTY_NAME="Raspberry Pi OS Bookworm 64-bit"\n'


class BuildRecordTest(unittest.TestCase):
    def build(self, **overrides):
        evidence = {
            "rss": RSS_TEXT,
            "first_frame": FIRST_FRAME_TEXT,
            "idle_cpu": PIDSTAT_HEADER + PIDSTAT_ROWS,
            "os_release": OS_TEXT,
            "device": DEVICE_TEXT,
        }
        evidence.update(overrides)
        return perf_record.build_record(
            BASE_RECORD,
            **evidence,
            recorded_at="2026-11-25T10:00:00Z",
        )

    def test_measured_metrics_replace_base_values_and_keep_the_soak(self):
        record = self.build()
        self.assertEqual(record["metrics"]["rss_idle_mb"], 90.0)
        self.assertEqual(record["metrics"]["first_frame_ms"], 1000.0)
        self.assertEqual(record["metrics"]["idle_cpu_percent"], 1.1)
        self.assertEqual(record["metrics"]["soak_hours"], 24.0)
        self.assertEqual(record["metrics"]["soak_rss_growth_mb"], 2.0)
        self.assertFalse(record["metrics"]["throttled"])
        self.assertEqual(record["device"], "raspberry-pi-zero-2-w")
        self.assertEqual(record["build"], "release")

    def test_debug_base_record_is_rejected(self):
        base = dict(BASE_RECORD, build="debug")
        with self.assertRaisesRegex(ValueError, "release"):
            perf_record.build_record(
                base,
                rss=RSS_TEXT,
                first_frame=FIRST_FRAME_TEXT,
                idle_cpu=PIDSTAT_HEADER + PIDSTAT_ROWS,
                os_release=OS_TEXT,
                device=DEVICE_TEXT,
                recorded_at="2026-11-25T10:00:00Z",
            )

    def test_non_kiosk_base_record_is_rejected(self):
        base = dict(BASE_RECORD, features="desktop")
        with self.assertRaisesRegex(ValueError, "kiosk"):
            perf_record.build_record(
                base,
                rss=RSS_TEXT,
                first_frame=FIRST_FRAME_TEXT,
                idle_cpu=PIDSTAT_HEADER + PIDSTAT_ROWS,
                os_release=OS_TEXT,
                device=DEVICE_TEXT,
                recorded_at="2026-11-25T10:00:00Z",
            )

    def test_budget_breach_is_rejected_instead_of_recorded(self):
        with self.assertRaisesRegex(ValueError, "rss_idle_mb"):
            self.build(rss="MainPID=1234\nMemoryCurrent=209715200\n")

    def test_a_substitute_device_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Pi Zero 2 W"):
            self.build(device="Raspberry Pi 3 Model B Plus Rev 1.3\0")


class CliTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def write(self, name: str, text: str) -> Path:
        path = self.root / name
        path.write_text(text, encoding="utf-8")
        return path

    def args(self, output: Path, **overrides) -> list[str]:
        files = {
            "base": self.write("base.json", json.dumps(BASE_RECORD)),
            "rss": self.write("rss.txt", RSS_TEXT),
            "first_frame": self.write("frames.txt", FIRST_FRAME_TEXT),
            "idle_cpu": self.write("pidstat.txt", PIDSTAT_HEADER + PIDSTAT_ROWS),
            "os": self.write("os-release", OS_TEXT),
            "device": self.write("model", DEVICE_TEXT),
        }
        files.update(overrides)
        return [
            "--base-record",
            str(files["base"]),
            "--rss",
            str(files["rss"]),
            "--first-frame",
            str(files["first_frame"]),
            "--idle-cpu",
            str(files["idle_cpu"]),
            "--os",
            str(files["os"]),
            "--device",
            str(files["device"]),
            "--recorded-at",
            "2026-11-25T10:00:00Z",
            "--output",
            str(output),
        ]

    def test_cli_writes_a_validated_record(self):
        output = self.root / "record.json"
        self.assertEqual(perf_record.main(self.args(output)), 0)
        record = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(record["metrics"]["rss_idle_mb"], 90.0)
        self.assertEqual(record["device"], "raspberry-pi-zero-2-w")

    def test_cli_writes_nothing_when_evidence_is_invalid(self):
        output = self.root / "record.json"
        args = self.args(output, rss=self.write("bad-rss.txt", "MemoryCurrent=0\n"))
        self.assertNotEqual(perf_record.main(args), 0)
        self.assertFalse(output.exists())

    def test_cli_writes_nothing_when_the_base_record_is_invalid(self):
        output = self.root / "record.json"
        args = self.args(output, base=self.write("bad-base.json", json.dumps({"metrics": {}})))
        self.assertNotEqual(perf_record.main(args), 0)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
