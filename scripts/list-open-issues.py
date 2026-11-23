#!/usr/bin/env python3
"""Read `gh issue list --json ...` from stdin and print a stable TSV list."""

from __future__ import annotations

import json
import re
import sys

include_mode = sys.argv[1] if len(sys.argv) > 1 else "subtask"
data = json.load(sys.stdin)

rows = []
for item in data:
    labels = {label["name"] for label in item.get("labels", [])}
    if "epic" in labels:
        continue
    if "subtask" not in labels:
        continue
    if include_mode != "all" and "commit-step" in labels:
        continue

    title = item["title"]
    match = re.search(r"\[W(\d+)\.(\d+)\.(\d+)\]", title)
    if match:
        key = (int(match.group(1)), int(match.group(2)), int(match.group(3)), item["number"])
    else:
        key = (99, 0, 0, item["number"])
    rows.append((key, item["number"], title, ",".join(sorted(labels)), item["url"]))

rows.sort(key=lambda row: row[0])
for _, number, title, labels, url in rows:
    print(f"{number}\t{title}\t{labels}\t{url}")
