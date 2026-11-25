#!/usr/bin/env python3
"""Tests for scripts/perf-record.py."""

from __future__ import annotations

import importlib.util
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


if __name__ == "__main__":
    unittest.main()
