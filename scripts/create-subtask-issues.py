#!/usr/bin/env python3
"""Create one GitHub issue per PLAN.md subtask (checkbox).

The seven wave issues are epics. Every checkbox in PLAN.md section 8 becomes a
separate issue, except pure `Commit:` steps, which stay inside their task.

The script is idempotent: docs/issues/subtasks.json stores the mapping, and an
existing entry is reused. It does not run in CI.

Requires: gh authenticated as ABOYrbx.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "ABOYrbx/home-assistant-kiosk"
INDEX_PATH = ROOT / "docs/issues/index.md"
MAPPING_PATH = ROOT / "docs/issues/subtasks.json"

EPICS = {
    0: 5,
    1: 3,
    2: 2,
    3: 6,
    4: 1,
    5: 4,
    6: 7,
}

WAVE_FILES = {n: ROOT / f"docs/issues/wave-{n}.md" for n in EPICS}


def plain(text: str) -> str:
    text = re.sub(r"`([^`]*)`", r"\1", text)
    text = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", text)
    text = text.replace("**", "")
    text = re.sub(r"\s+", " ", text).strip()
    if len(text) > 90:
        text = text[:87].rstrip() + "..."
    return text


def parse_plan() -> list[dict]:
    lines = (ROOT / "PLAN.md").read_text(encoding="utf-8").splitlines()
    wave = None
    task = "0.1"
    task_title = "Scaffold and interface contract"
    counters: dict[str, int] = {}
    items: list[dict] = []

    for line in lines:
        wave_match = re.match(r"^### Wave (\d+)", line)
        if wave_match:
            wave = int(wave_match.group(1))
            if wave == 0:
                task = "0.1"
                task_title = "Scaffold and interface contract"
            else:
                task = f"{wave}.0"
                task_title = "Wave tasks"
            continue

        task_match = re.match(r"^#### ([0-9]+\.[0-9]+) (.+)$", line)
        if task_match and wave is not None:
            task = task_match.group(1)
            task_title = task_match.group(2).strip()
            continue

        box = re.match(r"^- \[ \] (.+)$", line)
        if not box or wave is None:
            continue
        text = box.group(1).strip()
        if text.startswith("Commit:"):
            continue
        counters[task] = counters.get(task, 0) + 1
        subtask_id = f"W{task}.{counters[task]}"
        items.append(
            {
                "id": subtask_id,
                "wave": wave,
                "task": task,
                "task_title": task_title,
                "text": text,
            }
        )
    return items


def load_mapping() -> dict:
    if not MAPPING_PATH.is_file():
        return {"repo": REPO, "items": {}}
    data = json.loads(MAPPING_PATH.read_text(encoding="utf-8"))
    data.setdefault("items", {})
    return data


def save_mapping(mapping: dict) -> None:
    mapping["updated_at"] = datetime.now(timezone.utc).isoformat()
    tmp = MAPPING_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(mapping, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    tmp.replace(MAPPING_PATH)


def gh_json(args: list[str], stdin: str | None = None) -> dict:
    last = ""
    for attempt in range(4):
        proc = subprocess.run(args, input=stdin, text=True, capture_output=True)
        if proc.returncode == 0:
            return json.loads(proc.stdout or "{}")
        last = proc.stderr.strip() or proc.stdout.strip()
        lowered = last.lower()
        if "rate limit" in lowered or "secondary" in lowered or "abuse" in lowered:
            wait = 60 * (attempt + 1)
            print(f"  rate limited; sleeping {wait}s", flush=True)
            time.sleep(wait)
        else:
            time.sleep(5)
    raise RuntimeError(f"gh failed: {last}")


def issue_body(item: dict) -> str:
    epic = EPICS[item["wave"]]
    return f"""**Parent epic:** #{epic}
**Wave / task:** Wave {item["wave"]}, Task {item["task"]} — {item["task_title"]}
**Subtask:** {item["text"]}
**Plan:** [PLAN.md](https://github.com/{REPO}/blob/main/PLAN.md), section 8, Wave {item["wave"]}

## Definition of done

- [ ] Implemented as described.
- [ ] Tests or recorded manual evidence; run the applicable wave gate in `PLAN.md`.
- [ ] Documentation and `config.example.toml` updated if behavior changed.
- [ ] No secrets, personal data, or provider-specific URL/ID added.
- [ ] English; pushed with `ABOYrbx`.

This issue is one checkbox from the Wave {item["wave"]} epic. The epic stays open until all subtasks and the wave gate are complete.
"""


def create_issue(item: dict) -> dict:
    title = f'[{item["id"]}] {plain(item["text"])}'
    payload = {
        "title": title,
        "body": issue_body(item),
        "labels": ["implementation", "subtask", f'wave-{item["wave"]}'],
    }
    result = gh_json(
        ["gh", "api", "-X", "POST", f"repos/{REPO}/issues", "--input", "-"],
        stdin=json.dumps(payload),
    )
    return {
        "issue": result["number"],
        "url": result["html_url"],
        "title": title,
    }


def sort_key(item: dict) -> list[int]:
    return [int(part) for part in item["id"][1:].split(".")]


def epic_section(wave: int, mapping: dict) -> str:
    items = [v for v in mapping["items"].values() if v["wave"] == wave]
    items.sort(key=sort_key)
    lines = [
        "---",
        "",
        "## Subtask issues",
        "",
        "The wave issue is the epic. Every checkbox above, except the commit step, is a separate issue:",
        "",
    ]
    current_task = None
    for item in items:
        if item["task"] != current_task:
            current_task = item["task"]
            if lines and lines[-1] != "":
                lines.append("")
            lines.append(f"### {item['task']} {item['task_title']}")
            lines.append("")
        lines.append(f"- [ ] #{item['issue']} — {item['title']}")
    lines.append("")
    return "\n".join(lines)


def update_wave_file(wave: int, mapping: dict) -> None:
    path = WAVE_FILES[wave]
    text = path.read_text(encoding="utf-8")
    text = text.split("\n---\n\n## Subtask issues")[0].rstrip()
    path.write_text(text + "\n\n" + epic_section(wave, mapping), encoding="utf-8")


def update_epic(wave: int, mapping: dict) -> None:
    path = WAVE_FILES[wave]
    body = path.read_text(encoding="utf-8")
    tmp = ROOT / f".epic-{wave}.md"
    tmp.write_text(body, encoding="utf-8")
    subprocess.run(
        [
            "gh",
            "issue",
            "edit",
            str(EPICS[wave]),
            "--repo",
            REPO,
            "--title",
            f"Epic — Wave {wave}: {wave_title(wave)}",
            "--add-label",
            "epic",
            "--body-file",
            str(tmp),
        ],
        check=True,
    )
    tmp.unlink(missing_ok=True)


def wave_title(wave: int) -> str:
    for line in (ROOT / "PLAN.md").read_text(encoding="utf-8").splitlines():
        match = re.match(rf"^### Wave {wave} — (.+)$", line)
        if match:
            return match.group(1)
    return "implementation"


def write_index(mapping: dict) -> None:
    lines = [
        "# Subtask Issue Index",
        "",
        "The seven wave issues are epics. Every implementation checkbox in `PLAN.md`",
        "section 8 has its own issue, except pure `Commit:` steps. This file is",
        "generated by `scripts/create-subtask-issues.py`.",
        "",
        f"Generated: {mapping.get('updated_at', 'unknown')}",
        "",
        "| Wave | Task | Issue | Title |",
        "| --- | --- | --- | --- |",
    ]
    items = sorted(mapping["items"].values(), key=sort_key)
    for item in items:
        lines.append(
            f"| {item['wave']} | {item['task']} | [#{item['issue']}]({item['url']}) | {item['title']} |"
        )
    lines.append("")
    INDEX_PATH.write_text("\n".join(lines), encoding="utf-8")


def main() -> int:
    account = subprocess.run(
        ["gh", "api", "user", "--jq", ".login"], text=True, capture_output=True
    )
    if account.returncode != 0 or account.stdout.strip() != "ABOYrbx":
        print("error: gh must be authenticated as ABOYrbx", file=sys.stderr)
        return 1

    plan_items = parse_plan()
    mapping = load_mapping()
    print(f"Found {len(plan_items)} subtasks; {len(mapping['items'])} already mapped")

    for index, item in enumerate(plan_items, start=1):
        if item["id"] in mapping["items"]:
            continue
        print(f"[{index}/{len(plan_items)}] {item['id']} {plain(item['text'])[:60]}", flush=True)
        created = create_issue(item)
        mapping["items"][item["id"]] = {**item, **created}
        save_mapping(mapping)
        time.sleep(1.2)

    # Ensure every plan item is mapped before touching epics.
    missing = [item["id"] for item in plan_items if item["id"] not in mapping["items"]]
    if missing:
        print(f"error: {len(missing)} subtasks are still unmapped: {missing[:5]}", file=sys.stderr)
        return 1

    for wave in EPICS:
        update_wave_file(wave, mapping)
        update_epic(wave, mapping)

    write_index(mapping)
    save_mapping(mapping)
    print(f"Done: {len(mapping['items'])} subtask issues, {len(EPICS)} epics")
    return 0


if __name__ == "__main__":
    sys.exit(main())
