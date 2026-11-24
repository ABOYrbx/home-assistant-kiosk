#!/usr/bin/env python3
"""Capture Pi memory evidence with GNU time or systemd cgroup accounting.

GNU time reports the peak resident set of the profiled process. systemd reports
memory charged to the service cgroup, which can include page cache and is not
the same metric. The JSON output keeps the methods separate so a peak process
RSS is never recorded as an idle cgroup value.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

SCHEMA_VERSION = 1
MIB = 1024 * 1024
UINT64_MAX = 2**64 - 1


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _match_number(pattern: str, text: str, field: str) -> int:
    match = re.search(pattern, text, flags=re.MULTILINE)
    if match is None:
        raise ValueError(f"missing {field}")
    return int(match.group(1))


def parse_time_report(text: str) -> dict:
    """Parse the fields needed from a GNU ``time -v`` report."""
    peak_rss_kib = _match_number(
        r"^\s*Maximum resident set size \(kbytes\):\s*(\d+)\s*$",
        text,
        "Maximum resident set size (kbytes)",
    )
    exit_status = _match_number(r"^\s*Exit status:\s*(\d+)\s*$", text, "Exit status")
    return {
        "peak_rss_kib": peak_rss_kib,
        "peak_rss_mib": peak_rss_kib / 1024,
        "exit_status": exit_status,
    }


def parse_systemd_show(text: str) -> dict:
    """Parse systemd service memory properties and reject unavailable values."""
    properties = {}
    for line in text.splitlines():
        key, separator, value = line.partition("=")
        if separator:
            properties[key] = value

    values = {}
    for key in ("MainPID", "MemoryCurrent", "MemoryPeak"):
        raw = properties.get(key)
        if raw is None:
            raise ValueError(f"missing {key}")
        try:
            value = int(raw)
        except ValueError as exc:
            raise ValueError(f"{key} is not available: {raw!r}") from exc
        if value < 0 or value >= UINT64_MAX:
            raise ValueError(f"{key} is not available: {raw!r}")
        values[key] = value

    if values["MainPID"] == 0:
        raise ValueError("MainPID is 0; the service is not running")

    return {
        "main_pid": values["MainPID"],
        "memory_current_bytes": values["MemoryCurrent"],
        "memory_current_mib": values["MemoryCurrent"] / MIB,
        "memory_peak_bytes": values["MemoryPeak"],
        "memory_peak_mib": values["MemoryPeak"] / MIB,
    }


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


def _write_json(path: Path, record: dict) -> None:
    _atomic_write(path, json.dumps(record, indent=2, sort_keys=True) + "\n")


def run_time(output: Path, command: list[str]) -> int:
    env = os.environ.copy()
    env["LC_ALL"] = "C"
    completed = subprocess.run(
        ["/usr/bin/time", "-v", *command],
        capture_output=True,
        text=True,
        env=env,
        check=False,
    )
    raw_report = output.with_suffix(".time.txt")
    _atomic_write(raw_report, completed.stderr)

    try:
        parsed = parse_time_report(completed.stderr)
    except ValueError as exc:
        print(f"error: cannot parse GNU time report: {exc}", file=sys.stderr)
        print(f"raw report: {raw_report}", file=sys.stderr)
        return completed.returncode if completed.returncode != 0 else 1

    record = {
        "schema_version": SCHEMA_VERSION,
        "method": "gnu-time",
        "recorded_at": _timestamp(),
        "command": command,
        **parsed,
    }
    _write_json(output, record)
    print(
        f"peak RSS: {parsed['peak_rss_mib']:.1f} MiB; "
        f"target exit status: {completed.returncode}"
    )
    print(f"evidence: {output}")
    print(f"raw report: {raw_report}")
    return completed.returncode


def run_systemd(output: Path, unit: str) -> int:
    completed = subprocess.run(
        [
            "systemctl",
            "show",
            unit,
            "--property=MainPID",
            "--property=MemoryCurrent",
            "--property=MemoryPeak",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        print(
            f"error: systemctl show failed for {unit}: {completed.stderr.strip()}",
            file=sys.stderr,
        )
        return completed.returncode

    try:
        parsed = parse_systemd_show(completed.stdout)
    except ValueError as exc:
        print(f"error: cannot use systemd memory evidence for {unit}: {exc}", file=sys.stderr)
        return 1

    record = {
        "schema_version": SCHEMA_VERSION,
        "method": "systemd-cgroup",
        "recorded_at": _timestamp(),
        "unit": unit,
        **parsed,
    }
    _write_json(output, record)
    print(
        f"cgroup memory: current {parsed['memory_current_mib']:.1f} MiB; "
        f"peak {parsed['memory_peak_mib']:.1f} MiB"
    )
    print(f"evidence: {output}")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Capture Pi memory evidence with GNU time or systemd."
    )
    commands = parser.add_subparsers(dest="method", required=True)

    time_parser = commands.add_parser("time", help="run a command under /usr/bin/time -v")
    time_parser.add_argument("--output", required=True, type=Path, help="JSON evidence path")
    time_parser.add_argument("command", nargs=argparse.REMAINDER, help="command to profile")

    systemd_parser = commands.add_parser("systemd", help="read a systemd service cgroup")
    systemd_parser.add_argument("--unit", required=True, help="systemd unit, for example ha-kiosk")
    systemd_parser.add_argument("--output", required=True, type=Path, help="JSON evidence path")

    args = parser.parse_args(argv)
    if args.method == "time":
        command = args.command
        if command and command[0] == "--":
            command = command[1:]
        if not command:
            time_parser.error("a command is required after --")
        return run_time(args.output, command)
    return run_systemd(args.output, args.unit)


if __name__ == "__main__":
    sys.exit(main())
