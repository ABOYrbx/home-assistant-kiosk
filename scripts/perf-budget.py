#!/usr/bin/env python3
"""Check a recorded performance measurement against the Pi Zero 2 W budgets.

The record is produced on the target device with the method in
docs/performance.md. A record that breaches a hard gate fails, a record that
misses the memory target passes with a warning, and a malformed record fails.
The default check covers the full Wave 5 gate; --runtime-only covers issue #111
without requiring the separate 24-hour soak metrics.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from datetime import datetime
from pathlib import Path

SCHEMA_VERSION = 1

# Hard gates and targets from PLAN.md sections 3 and 8.
RSS_HARD_MB = 150.0
RSS_TARGET_MB = 100.0
FIRST_FRAME_HARD_MS = 3000.0
IDLE_CPU_HARD_PERCENT = 25.0
SOAK_MIN_HOURS = 24.0
SOAK_GROWTH_HARD_MB = 10.0

REQUIRED_FIELDS = (
    "schema_version",
    "device",
    "os",
    "build",
    "features",
    "recorded_at",
    "metrics",
)
OPTIONAL_FIELDS = ("notes",)
RUNTIME_METRICS = (
    "rss_idle_mb",
    "first_frame_ms",
    "idle_cpu_percent",
)
SOAK_METRICS = (
    "soak_hours",
    "soak_rss_growth_mb",
    "throttled",
)
METRICS = RUNTIME_METRICS + SOAK_METRICS
PRIMARY_DEVICE = "raspberry-pi-zero-2-w"

# (key, value, budget, status)
Row = tuple[str, str, str, str]


def _number(metrics: dict, key: str, errors: list[str]) -> float | None:
    if key not in metrics:
        return None
    value = metrics[key]
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        errors.append(f"metric {key} must be a number")
        return None
    if not math.isfinite(value):
        errors.append(f"metric {key} must be finite")
        return None
    if value < 0:
        errors.append(f"metric {key} must not be negative")
        return None
    return float(value)


def _check_metrics(
    metrics: object, required_metrics: tuple[str, ...]
) -> tuple[list[str], list[str], list[Row]]:
    errors: list[str] = []
    warnings: list[str] = []
    rows: list[Row] = []

    if not isinstance(metrics, dict):
        return (["metrics must be an object"], warnings, rows)

    for key in sorted(set(metrics) - set(METRICS)):
        errors.append(f"unknown metric: {key}")
    for key in required_metrics:
        if key not in metrics:
            errors.append(f"missing metric: {key}")

    rss = _number(metrics, "rss_idle_mb", errors)
    if rss is not None:
        if rss >= RSS_HARD_MB:
            errors.append(
                f"rss_idle_mb is {rss:.1f} MB; the hard gate is below {RSS_HARD_MB:.0f} MB"
            )
            rows.append(("rss_idle_mb", f"{rss:.1f} MB", f"< {RSS_HARD_MB:.0f} MB", "fail"))
        elif rss >= RSS_TARGET_MB:
            warnings.append(
                f"rss_idle_mb is {rss:.1f} MB; the target is below {RSS_TARGET_MB:.0f} MB"
            )
            rows.append(
                (
                    "rss_idle_mb",
                    f"{rss:.1f} MB",
                    f"< {RSS_HARD_MB:.0f} MB (target < {RSS_TARGET_MB:.0f} MB)",
                    "warn",
                )
            )
        else:
            rows.append(
                (
                    "rss_idle_mb",
                    f"{rss:.1f} MB",
                    f"< {RSS_HARD_MB:.0f} MB (target < {RSS_TARGET_MB:.0f} MB)",
                    "ok",
                )
            )

    first_frame = _number(metrics, "first_frame_ms", errors)
    if first_frame is not None:
        budget = f"< {FIRST_FRAME_HARD_MS:.0f} ms"
        if first_frame >= FIRST_FRAME_HARD_MS:
            errors.append(
                f"first_frame_ms is {first_frame:.0f} ms; the hard gate is below "
                f"{FIRST_FRAME_HARD_MS:.0f} ms"
            )
            rows.append(("first_frame_ms", f"{first_frame:.0f} ms", budget, "fail"))
        else:
            rows.append(("first_frame_ms", f"{first_frame:.0f} ms", budget, "ok"))

    idle_cpu = _number(metrics, "idle_cpu_percent", errors)
    if idle_cpu is not None:
        budget = f"<= {IDLE_CPU_HARD_PERCENT:.0f} % of one core"
        if idle_cpu > IDLE_CPU_HARD_PERCENT:
            errors.append(
                f"idle_cpu_percent is {idle_cpu:.1f} %; the limit is "
                f"{IDLE_CPU_HARD_PERCENT:.0f} % of one core"
            )
            rows.append(("idle_cpu_percent", f"{idle_cpu:.1f} %", budget, "fail"))
        else:
            rows.append(("idle_cpu_percent", f"{idle_cpu:.1f} %", budget, "ok"))

    soak_hours = _number(metrics, "soak_hours", errors)
    if soak_hours is not None:
        budget = f">= {SOAK_MIN_HOURS:.0f} h"
        if soak_hours < SOAK_MIN_HOURS:
            errors.append(
                f"soak_hours is {soak_hours:.1f} h; the soak must run at least "
                f"{SOAK_MIN_HOURS:.0f} h"
            )
            rows.append(("soak_hours", f"{soak_hours:.1f} h", budget, "fail"))
        else:
            rows.append(("soak_hours", f"{soak_hours:.1f} h", budget, "ok"))

    growth = _number(metrics, "soak_rss_growth_mb", errors)
    if growth is not None:
        budget = f"<= {SOAK_GROWTH_HARD_MB:.0f} MB"
        if growth > SOAK_GROWTH_HARD_MB:
            errors.append(
                f"soak_rss_growth_mb is {growth:.1f} MB; the leak limit is "
                f"{SOAK_GROWTH_HARD_MB:.0f} MB"
            )
            rows.append(("soak_rss_growth_mb", f"{growth:.1f} MB", budget, "fail"))
        else:
            rows.append(("soak_rss_growth_mb", f"{growth:.1f} MB", budget, "ok"))

    throttled = metrics.get("throttled")
    if "throttled" in metrics:
        if not isinstance(throttled, bool):
            errors.append("metric throttled must be true or false")
        elif throttled:
            errors.append(
                "the firmware reported a throttling condition (throttled = true)"
            )
            rows.append(("throttled", "yes", "false", "fail"))
        else:
            rows.append(("throttled", "no", "false", "ok"))

    return errors, warnings, rows


def check_record(
    record: object, *, runtime_only: bool = False
) -> tuple[list[str], list[str], list[Row]]:
    """Return (errors, warnings, rows) for a decoded performance record.

    The full Wave 5 record is the default. ``runtime_only`` requires just the
    RSS, first-frame, and idle CPU metrics from issue #111.
    """
    errors: list[str] = []
    warnings: list[str] = []

    if not isinstance(record, dict):
        return (["record must be a JSON object"], warnings, [])

    for field in REQUIRED_FIELDS:
        if field not in record:
            errors.append(f"missing field: {field}")
    for field in sorted(set(record) - set(REQUIRED_FIELDS) - set(OPTIONAL_FIELDS)):
        errors.append(f"unknown field: {field}")

    version = record.get("schema_version")
    if not isinstance(version, int) or isinstance(version, bool) or version != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}")

    for field in ("device", "os", "build", "features"):
        if field in record:
            value = record[field]
            if not isinstance(value, str) or not value.strip():
                errors.append(f"{field} must be a non-empty string")

    build = record.get("build")
    if isinstance(build, str) and build.strip() and build != "release":
        errors.append('build must be "release"; debug numbers are not a release gate')

    recorded_at = record.get("recorded_at")
    if isinstance(recorded_at, str):
        try:
            datetime.fromisoformat(recorded_at.replace("Z", "+00:00"))
        except ValueError:
            errors.append("recorded_at must be an ISO-8601 timestamp")
    elif "recorded_at" in record:
        errors.append("recorded_at must be an ISO-8601 timestamp")

    device = record.get("device")
    if isinstance(device, str) and device.strip() and device != PRIMARY_DEVICE:
        warnings.append(f"device is {device!r}, not the primary target {PRIMARY_DEVICE}")

    if "notes" in record and not isinstance(record["notes"], str):
        errors.append("notes must be a string")

    if "metrics" in record:
        required_metrics = RUNTIME_METRICS if runtime_only else METRICS
        metric_errors, metric_warnings, rows = _check_metrics(
            record["metrics"], required_metrics
        )
        errors.extend(metric_errors)
        warnings.extend(metric_warnings)
    else:
        rows = []

    return errors, warnings, rows


def _print_report(record: dict, rows: list[Row]) -> None:
    print(
        f"record: {record.get('device')} | {record.get('os')} | "
        f"{record.get('build')}/{record.get('features')} | {record.get('recorded_at')}"
    )
    if not rows:
        return
    key_width = max(len(row[0]) for row in rows)
    value_width = max(len(row[1]) for row in rows)
    budget_width = max(len(row[2]) for row in rows)
    print(f"{'metric':<{key_width}}  {'value':<{value_width}}  {'budget':<{budget_width}}  status")
    for key, value, budget, status in rows:
        print(f"{key:<{key_width}}  {value:<{value_width}}  {budget:<{budget_width}}  {status}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check a performance record against the Pi Zero 2 W budgets."
    )
    parser.add_argument("record", type=Path, help="JSON performance record to check")
    parser.add_argument(
        "--runtime-only",
        action="store_true",
        help="require only RSS, first-frame, and idle CPU metrics (issue #111)",
    )
    args = parser.parse_args(argv)

    try:
        raw = args.record.read_text(encoding="utf-8")
    except OSError as exc:
        print(f"error: cannot read {args.record}: {exc}", file=sys.stderr)
        return 2

    try:
        record = json.loads(raw)
    except json.JSONDecodeError as exc:
        print(f"error: {args.record} is not valid JSON: {exc}", file=sys.stderr)
        return 2

    errors, warnings, rows = check_record(
        record, runtime_only=args.runtime_only
    )
    if isinstance(record, dict):
        _print_report(record, rows)

    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    if errors:
        print(f"\n{len(errors)} error(s); the performance gate is not met")
        return 1

    if args.runtime_only:
        print("\nresult: runtime budgets pass")
    else:
        print("\nresult: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
