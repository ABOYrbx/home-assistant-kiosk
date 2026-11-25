#!/usr/bin/env python3
"""Build a Pi Zero 2 W performance record from saved measurement evidence."""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

MIB = 1024 * 1024
PRIMARY_DEVICE = "raspberry-pi-zero-2-w"


def parse_rss(text: str) -> float:
    """Return systemd's current service memory in MiB."""
    for line in text.splitlines():
        key, separator, value = line.partition("=")
        if not separator or key.strip() != "MemoryCurrent":
            continue
        try:
            byte_count = int(value.strip())
        except ValueError as exc:
            raise ValueError("MemoryCurrent must be an integer byte count") from exc
        if byte_count <= 0:
            raise ValueError("MemoryCurrent must be greater than zero")
        return byte_count / MIB
    raise ValueError("systemd evidence is missing MemoryCurrent")


_STRACE_LINE = re.compile(r"^\s*\d+\s+(\d+(?:\.\d+)?)\s+(.*)$")


def parse_first_frame(text: str) -> float:
    """Return milliseconds from process exec to the first DRM presentation ioctl."""
    started_at: float | None = None
    for line in text.splitlines():
        match = _STRACE_LINE.match(line)
        if match is None:
            if "execve(" in line or "DRM_IOCTL_MODE_" in line:
                raise ValueError("strace evidence contains a malformed timestamp")
            continue

        timestamp = float(match.group(1))
        syscall = match.group(2)
        if started_at is None and syscall.startswith("execve("):
            if not re.search(r"=\s*0\s*$", syscall):
                continue
            started_at = timestamp
            continue

        if started_at is not None and (
            "DRM_IOCTL_MODE_PAGE_FLIP" in syscall or "DRM_IOCTL_MODE_ATOMIC" in syscall
        ):
            elapsed_ms = (timestamp - started_at) * 1000
            if elapsed_ms < 0:
                raise ValueError("first-frame timestamp precedes process start")
            return round(elapsed_ms, 3)

    if started_at is None:
        raise ValueError("strace evidence has no successful execve process start")
    raise ValueError("strace evidence has no DRM page-flip or atomic presentation")


def parse_idle_cpu(text: str, expected_samples: int = 10) -> float:
    """Return the mean CPU percentage from complete one-minute pidstat samples."""
    if expected_samples <= 0:
        raise ValueError("expected_samples must be greater than zero")

    samples: list[float] = []
    for line in text.splitlines():
        if not line.strip() or line.lstrip().startswith("Average:"):
            continue
        fields = line.split()
        if len(fields) < 11:
            continue
        if not re.fullmatch(r"\d{2}:\d{2}:\d{2}", fields[0]):
            continue
        if fields[1] not in {"AM", "PM"} or not fields[2].isdigit() or not fields[3].isdigit():
            continue
        try:
            percentages = [float(value) for value in fields[4:10]]
        except ValueError as exc:
            raise ValueError("pidstat evidence contains a malformed CPU value") from exc
        if any(value < 0 for value in percentages):
            raise ValueError("pidstat CPU values must not be negative")
        samples.append(percentages[-1])

    if len(samples) != expected_samples:
        raise ValueError(
            f"expected {expected_samples} one-minute pidstat samples, found {len(samples)}"
        )
    return round(sum(samples) / len(samples), 3)


def parse_os(text: str) -> str:
    """Return PRETTY_NAME from os-release evidence."""
    for line in text.splitlines():
        key, separator, value = line.partition("=")
        if not separator or key.strip() != "PRETTY_NAME":
            continue
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
            value = value[1:-1]
        if value:
            return value
    raise ValueError("os-release evidence is missing PRETTY_NAME")


def parse_device(text: str) -> str:
    """Return the canonical target model, rejecting substitute hardware."""
    model = text.replace("\0", "").strip()
    if not model:
        raise ValueError("device model evidence is empty")
    if model.lower().startswith("raspberry pi zero 2 w"):
        return PRIMARY_DEVICE
    raise ValueError(f"expected a Pi Zero 2 W device model, got {model!r}")


_PERF_BUDGET_MODULE = None


def _load_perf_budget():
    global _PERF_BUDGET_MODULE
    if _PERF_BUDGET_MODULE is not None:
        return _PERF_BUDGET_MODULE
    try:
        import perf_budget  # type: ignore

        _PERF_BUDGET_MODULE = perf_budget
        return _PERF_BUDGET_MODULE
    except Exception:
        pass
    try:
        import importlib.util

        path = Path(__file__).resolve().parent / "perf-budget.py"
        spec = importlib.util.spec_from_file_location("perf_budget", path)
        if spec is None or spec.loader is None:
            return None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        _PERF_BUDGET_MODULE = module
        return _PERF_BUDGET_MODULE
    except Exception:
        return None


def _validate_record(record: object) -> list[str]:
    module = _load_perf_budget()
    if module is None:
        if not isinstance(record, dict):
            return ["record must be a JSON object"]
        return []
    errors, _warnings, _rows = module.check_record(record)
    return errors


def build_record(
    base: object,
    *,
    rss: str,
    first_frame: str,
    idle_cpu: str,
    os: str,
    device: str,
    recorded_at: str,
    notes: str = "",
) -> dict:
    """Build a schema-v1 performance record from parsed measurement evidence."""
    errors = _validate_record(base)
    if errors:
        raise ValueError("invalid base record: " + "; ".join(errors))

    rss_value = parse_rss(rss)
    first_frame_value = parse_first_frame(first_frame)
    idle_cpu_value = parse_idle_cpu(idle_cpu)
    os_value = parse_os(os)
    device_value = parse_device(device)

    if not isinstance(recorded_at, str) or not recorded_at.strip():
        raise ValueError("recorded_at must be a non-empty string")

    record = copy.deepcopy(base) if isinstance(base, dict) else {}
    record["schema_version"] = record.get("schema_version", 1)
    record["device"] = device_value
    record["os"] = os_value
    if "build" not in record or not isinstance(record.get("build"), str):
        record["build"] = "release"
    if "features" not in record or not isinstance(record.get("features"), str):
        record["features"] = "kiosk"
    record["recorded_at"] = recorded_at
    if notes.strip():
        record["notes"] = notes

    metrics = record.get("metrics")
    if not isinstance(metrics, dict):
        metrics = {}
        record["metrics"] = metrics
    metrics["rss_idle_mb"] = rss_value
    metrics["first_frame_ms"] = first_frame_value
    metrics["idle_cpu_percent"] = idle_cpu_value

    result_errors = _validate_record(record)
    if result_errors:
        raise ValueError("invalid performance record: " + "; ".join(result_errors))

    return record


def _read_evidence(value: str) -> str:
    path = Path(value)
    if path.is_file():
        return path.read_text(encoding="utf-8")
    return value


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Build a Pi Zero 2 W performance record from saved evidence."
    )
    parser.add_argument("--base-record", required=True)
    parser.add_argument("--rss", required=True)
    parser.add_argument("--first-frame", required=True)
    parser.add_argument("--idle-cpu", required=True)
    parser.add_argument("--os", required=True)
    parser.add_argument("--device", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--recorded-at")
    parser.add_argument("--notes", default="")
    args = parser.parse_args(argv)

    try:
        base = json.loads(Path(args.base_record).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: invalid base record: {exc}", file=sys.stderr)
        return 1

    try:
        rss_text = _read_evidence(args.rss)
        first_frame_text = _read_evidence(args.first_frame)
        idle_cpu_text = _read_evidence(args.idle_cpu)
        os_text = _read_evidence(args.os)
        device_text = _read_evidence(args.device)
    except OSError as exc:
        print(f"error: cannot read evidence: {exc}", file=sys.stderr)
        return 1

    recorded_at = args.recorded_at
    if not recorded_at:
        recorded_at = (
            datetime.now(timezone.utc)
            .replace(microsecond=0)
            .isoformat()
            .replace("+00:00", "Z")
        )

    try:
        record = build_record(
            base,
            rss=rss_text,
            first_frame=first_frame_text,
            idle_cpu=idle_cpu_text,
            os=os_text,
            device=device_text,
            recorded_at=recorded_at,
            notes=args.notes,
        )
    except (ValueError, TypeError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    try:
        Path(args.output).write_text(
            json.dumps(record, indent=2) + "\n", encoding="utf-8"
        )
    except OSError as exc:
        print(f"error: cannot write output: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
