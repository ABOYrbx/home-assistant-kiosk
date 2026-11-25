#!/usr/bin/env python3
"""Build a Pi Zero 2 W performance record from saved measurement evidence.

The raw evidence comes from the commands in ``docs/performance.md``: systemd
cgroup memory, an epoch-timestamped strace of the first DRM presentation, ten
one-minute pidstat samples, ``os-release`` and the device-tree model. The
collector refuses substitute hardware, partial CPU samples, and any record that
the budget checker rejects, so a plausible-looking number is never invented.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

MIB = 1024 * 1024
PRIMARY_DEVICE = "raspberry-pi-zero-2-w"
SCHEMA_VERSION = 1


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


def _budget_checker():
    """Load ``perf-budget.py``; its hyphenated name is not importable."""
    path = Path(__file__).resolve().parent / "perf-budget.py"
    spec = importlib.util.spec_from_file_location("perf_budget", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load the budget checker at {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_record(
    base: object,
    *,
    rss: str,
    first_frame: str,
    idle_cpu: str,
    os_release: str,
    device: str,
    recorded_at: str,
    notes: str = "",
) -> dict:
    """Return a complete schema-v1 record built from the base record and evidence."""
    if not isinstance(base, dict):
        raise ValueError("base record must be a JSON object")
    if base.get("build") != "release":
        raise ValueError('base record build must be "release"; debug numbers are not a gate')
    if base.get("features") != "kiosk":
        raise ValueError('base record features must be "kiosk"')
    base_metrics = base.get("metrics")
    if not isinstance(base_metrics, dict):
        raise ValueError("base record metrics must be an object")

    metrics = dict(base_metrics)
    metrics["rss_idle_mb"] = parse_rss(rss)
    metrics["first_frame_ms"] = parse_first_frame(first_frame)
    metrics["idle_cpu_percent"] = parse_idle_cpu(idle_cpu)

    record = {
        "schema_version": SCHEMA_VERSION,
        "device": parse_device(device),
        "os": parse_os(os_release),
        "build": "release",
        "features": "kiosk",
        "recorded_at": recorded_at,
        "metrics": metrics,
    }
    if notes.strip():
        record["notes"] = notes.strip()

    errors, _warnings, _rows = _budget_checker().check_record(record)
    if errors:
        raise ValueError("; ".join(errors))
    return record


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _atomic_write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.write(text)
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def _read(path: Path, label: str) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        raise ValueError(f"cannot read {label} evidence at {path}: {exc}") from exc


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Build a Pi Zero 2 W performance record from saved evidence."
    )
    parser.add_argument("--base-record", required=True, type=Path)
    parser.add_argument("--rss", required=True, type=Path)
    parser.add_argument("--first-frame", required=True, type=Path)
    parser.add_argument("--idle-cpu", required=True, type=Path)
    parser.add_argument("--os", required=True, type=Path, dest="os_path")
    parser.add_argument("--device", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--recorded-at", default=None)
    parser.add_argument("--notes", default="")
    args = parser.parse_args(argv)

    output = args.output
    try:
        base = json.loads(_read(args.base_record, "base record"))
    except json.JSONDecodeError as exc:
        print(f"error: base record is not valid JSON: {exc}", file=sys.stderr)
        return 1

    try:
        record = build_record(
            base,
            rss=_read(args.rss, "memory"),
            first_frame=_read(args.first_frame, "first-frame"),
            idle_cpu=_read(args.idle_cpu, "idle-CPU"),
            os_release=_read(args.os_path, "os-release"),
            device=_read(args.device, "device model"),
            recorded_at=args.recorded_at or _timestamp(),
            notes=args.notes,
        )
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        print("no record was written", file=sys.stderr)
        return 1

    _atomic_write(output, json.dumps(record, indent=2, sort_keys=True) + "\n")
    metrics = record["metrics"]
    print(
        f"record: {record['device']}, OS {record['os']}\n"
        f"RSS {metrics['rss_idle_mb']:.1f} MB, "
        f"first frame {metrics['first_frame_ms']:.0f} ms, "
        f"idle CPU {metrics['idle_cpu_percent']:.1f} %"
    )
    print(f"written: {output}")
    print(f"check: python3 scripts/perf-budget.py {output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
