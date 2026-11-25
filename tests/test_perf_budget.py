#!/usr/bin/env python3
"""Tests for scripts/perf-budget.py."""

from __future__ import annotations

import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "perf-budget.py"

_SPEC = importlib.util.spec_from_file_location("perf_budget", SCRIPT)
assert _SPEC is not None and _SPEC.loader is not None
perf_budget = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(perf_budget)

BASE = {
    "schema_version": 1,
    "device": "raspberry-pi-zero-2-w",
    "os": "Raspberry Pi OS Bookworm 64-bit",
    "build": "release",
    "features": "kiosk",
    "recorded_at": "2026-11-24T10:00:00Z",
    "metrics": {
        "rss_idle_mb": 78.4,
        "first_frame_ms": 1820,
        "idle_cpu_percent": 6.2,
        "soak_hours": 24.0,
        "soak_rss_growth_mb": 2.7,
        "throttled": False,
    },
}


def make_record(**overrides):
    data = copy.deepcopy(BASE)
    for key, value in overrides.items():
        if key == "metrics":
            data["metrics"].update(value)
        else:
            data[key] = value
    return data


def check(**overrides):
    return perf_budget.check_record(make_record(**overrides))


def runtime_record():
    record = make_record()
    for key in ("soak_hours", "soak_rss_growth_mb", "throttled"):
        del record["metrics"][key]
    return record


class WithinBudgetTest(unittest.TestCase):
    def test_record_within_budget_passes(self):
        errors, warnings, rows = check()
        self.assertEqual(errors, [])
        self.assertEqual(warnings, [])
        self.assertEqual(len(rows), 6)
        self.assertTrue(all(row[3] == "ok" for row in rows))

    def test_notes_are_accepted(self):
        errors, warnings, _ = check(notes="First frame from the first DRM page flip.")
        self.assertEqual(errors, [])
        self.assertEqual(warnings, [])


class RssBudgetTest(unittest.TestCase):
    def test_rss_at_hard_gate_fails(self):
        errors, _, _ = check(metrics={"rss_idle_mb": 150.0})
        self.assertTrue(any("rss_idle_mb" in error for error in errors))

    def test_rss_above_hard_gate_fails(self):
        errors, _, _ = check(metrics={"rss_idle_mb": 181.2})
        self.assertTrue(any("rss_idle_mb" in error for error in errors))

    def test_rss_below_hard_gate_but_above_target_warns(self):
        errors, warnings, rows = check(metrics={"rss_idle_mb": 120.0})
        self.assertEqual(errors, [])
        self.assertTrue(any("rss_idle_mb" in warning for warning in warnings))
        self.assertEqual([row[3] for row in rows if row[0] == "rss_idle_mb"], ["warn"])

    def test_rss_at_target_warns(self):
        errors, warnings, _ = check(metrics={"rss_idle_mb": 100.0})
        self.assertEqual(errors, [])
        self.assertTrue(warnings)

    def test_rss_below_target_is_ok(self):
        errors, warnings, _ = check(metrics={"rss_idle_mb": 99.9})
        self.assertEqual(errors, [])
        self.assertEqual(warnings, [])


class FirstFrameBudgetTest(unittest.TestCase):
    def test_first_frame_at_hard_gate_fails(self):
        errors, _, _ = check(metrics={"first_frame_ms": 3000})
        self.assertTrue(any("first_frame_ms" in error for error in errors))

    def test_first_frame_below_hard_gate_passes(self):
        errors, _, _ = check(metrics={"first_frame_ms": 2999})
        self.assertEqual(errors, [])


class IdleCpuBudgetTest(unittest.TestCase):
    def test_idle_cpu_at_limit_passes(self):
        errors, _, _ = check(metrics={"idle_cpu_percent": 25.0})
        self.assertEqual(errors, [])

    def test_idle_cpu_above_limit_fails(self):
        errors, _, _ = check(metrics={"idle_cpu_percent": 25.1})
        self.assertTrue(any("idle_cpu_percent" in error for error in errors))


class RuntimeBudgetScopeTest(unittest.TestCase):
    def test_runtime_record_without_soak_passes(self):
        errors, warnings, rows = perf_budget.check_record(
            runtime_record(), runtime_only=True
        )
        self.assertEqual(errors, [])
        self.assertEqual(warnings, [])
        self.assertEqual(len(rows), 3)
        self.assertTrue(all(row[3] == "ok" for row in rows))

    def test_runtime_scope_requires_each_runtime_metric(self):
        for key in ("rss_idle_mb", "first_frame_ms", "idle_cpu_percent"):
            with self.subTest(metric=key):
                record = runtime_record()
                del record["metrics"][key]
                errors, _, _ = perf_budget.check_record(
                    record, runtime_only=True
                )
                self.assertTrue(any(key in error for error in errors))

    def test_non_finite_metrics_fail(self):
        for key in (
            "rss_idle_mb",
            "first_frame_ms",
            "idle_cpu_percent",
            "soak_hours",
            "soak_rss_growth_mb",
        ):
            for value in (float("nan"), float("inf"), float("-inf")):
                with self.subTest(metric=key, value=value):
                    errors, _, _ = check(metrics={key: value})
                    self.assertTrue(
                        any(key in error and "finite" in error for error in errors)
                    )


class SoakBudgetTest(unittest.TestCase):
    def test_short_soak_fails(self):
        errors, _, _ = check(metrics={"soak_hours": 23.9})
        self.assertTrue(any("soak_hours" in error for error in errors))

    def test_soak_growth_at_limit_passes(self):
        errors, _, _ = check(metrics={"soak_rss_growth_mb": 10.0})
        self.assertEqual(errors, [])

    def test_soak_growth_above_limit_fails(self):
        errors, _, _ = check(metrics={"soak_rss_growth_mb": 10.1})
        self.assertTrue(any("soak_rss_growth_mb" in error for error in errors))

    def test_throttled_device_fails(self):
        errors, _, _ = check(metrics={"throttled": True})
        self.assertTrue(any("throttled" in error for error in errors))


class SchemaTest(unittest.TestCase):
    def test_missing_metric_fails(self):
        record = make_record()
        del record["metrics"]["idle_cpu_percent"]
        errors, _, _ = perf_budget.check_record(record)
        self.assertTrue(any("idle_cpu_percent" in error for error in errors))

    def test_unknown_metric_fails(self):
        errors, _, _ = check(metrics={"battery_percent": 12})
        self.assertTrue(any("battery_percent" in error for error in errors))

    def test_missing_field_fails(self):
        record = make_record()
        del record["device"]
        errors, _, _ = perf_budget.check_record(record)
        self.assertTrue(any("device" in error for error in errors))

    def test_unknown_field_fails(self):
        errors, _, _ = perf_budget.check_record(make_record(comment="hand written"))
        self.assertTrue(any("comment" in error for error in errors))

    def test_wrong_schema_version_fails(self):
        errors, _, _ = perf_budget.check_record(make_record(schema_version=2))
        self.assertTrue(any("schema_version" in error for error in errors))

    def test_non_numeric_metric_fails(self):
        errors, _, _ = check(metrics={"rss_idle_mb": "78"})
        self.assertTrue(any("rss_idle_mb" in error for error in errors))

    def test_boolean_metric_fails(self):
        errors, _, _ = check(metrics={"first_frame_ms": True})
        self.assertTrue(any("first_frame_ms" in error for error in errors))

    def test_negative_metric_fails(self):
        errors, _, _ = check(metrics={"soak_hours": -1})
        self.assertTrue(any("soak_hours" in error for error in errors))

    def test_non_release_build_fails(self):
        errors, _, _ = perf_budget.check_record(make_record(build="debug"))
        self.assertTrue(any("release" in error for error in errors))

    def test_bad_timestamp_fails(self):
        errors, _, _ = perf_budget.check_record(make_record(recorded_at="yesterday"))
        self.assertTrue(any("recorded_at" in error for error in errors))

    def test_other_device_warns(self):
        errors, warnings, _ = perf_budget.check_record(
            make_record(device="generic-x86_64")
        )
        self.assertEqual(errors, [])
        self.assertTrue(any("generic-x86_64" in warning for warning in warnings))

    def test_non_object_record_fails(self):
        errors, _, rows = perf_budget.check_record([])
        self.assertTrue(errors)
        self.assertEqual(rows, [])


class CliTest(unittest.TestCase):
    def run_cli(self, text: str, *args: str) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "perf-record.json"
            path.write_text(text, encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(SCRIPT), *args, str(path)],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_passing_record_exits_zero(self):
        result = self.run_cli(json.dumps(make_record()))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("result: pass", result.stdout)

    def test_runtime_only_record_exits_zero(self):
        result = self.run_cli(json.dumps(runtime_record()), "--runtime-only")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("result: runtime budgets pass", result.stdout)
        self.assertNotIn("soak_hours", result.stdout)

    def test_runtime_only_missing_metric_exits_one(self):
        record = runtime_record()
        del record["metrics"]["first_frame_ms"]
        result = self.run_cli(json.dumps(record), "--runtime-only")
        self.assertEqual(result.returncode, 1)
        self.assertIn("missing metric: first_frame_ms", result.stdout)

    def test_non_finite_metric_exits_one(self):
        result = self.run_cli(
            json.dumps(make_record(metrics={"first_frame_ms": float("nan")})),
            "--runtime-only",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("finite", result.stdout)

    def test_target_miss_exits_zero_with_warning(self):
        result = self.run_cli(json.dumps(make_record(metrics={"rss_idle_mb": 120.0})))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("warning:", result.stdout)
        self.assertIn("result: pass", result.stdout)

    def test_budget_breach_exits_one(self):
        result = self.run_cli(json.dumps(make_record(metrics={"rss_idle_mb": 160.0})))
        self.assertEqual(result.returncode, 1)
        self.assertIn("error:", result.stdout)

    def test_malformed_json_exits_two(self):
        result = self.run_cli("{")
        self.assertEqual(result.returncode, 2)
        self.assertIn("not valid JSON", result.stderr)

    def test_missing_file_exits_two(self):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "/nonexistent/perf-record.json"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("cannot read", result.stderr)


if __name__ == "__main__":
    unittest.main()
