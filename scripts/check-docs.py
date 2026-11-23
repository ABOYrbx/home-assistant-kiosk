#!/usr/bin/env python3
"""Validate the planning repository before a push or pull request.

This check is intentionally dependency-free on GitHub-hosted Python. It does
not validate Rust code; the Rust gate arrives in Wave 0.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

REQUIRED_FILES = [
    "AGENTS.md",
    "CODE_OF_CONDUCT.md",
    "CONTRIBUTING.md",
    "DESIGN.md",
    "PLAN.md",
    "README.md",
    "SECURITY.md",
    "config.example.toml",
    "tokens.css",
    "styles.css",
    "index.html",
    "package.json",
    ".github/CODEOWNERS",
    ".github/PULL_REQUEST_TEMPLATE.md",
    ".github/ISSUE_TEMPLATE/bug_report.yml",
    ".github/ISSUE_TEMPLATE/feature_request.yml",
    ".github/ISSUE_TEMPLATE/config.yml",
    ".github/workflows/docs.yml",
    "docs/departures-entity-contract.md",
    "docs/extension-catalog.md",
    "docs/extensions.md",
    "docs/github-rules.md",
    "docs/issues/index.md",
    "docs/plan-review.md",
    "scripts/apply-github-settings.sh",
]

REQUIRED_WAVES = [f"docs/issues/wave-{i}.md" for i in range(7)]

FORBIDDEN = [
    "Dohlenweg",
    "fahrplanauskunft-mv",
    "VMV_EFA",
    "10.0.5.",
    "192.168.",
]

LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")

errors: list[str] = []
warnings: list[str] = []


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def check_required() -> None:
    for rel in REQUIRED_FILES + REQUIRED_WAVES:
        if not (ROOT / rel).is_file():
            errors.append(f"missing required file: {rel}")


def check_toml() -> None:
    path = ROOT / "config.example.toml"
    if not path.is_file():
        return
    try:
        try:
            import tomllib  # Python 3.11+
        except ImportError:  # Python 3.9/3.10 on a developer machine
            import toml  # type: ignore

            data = toml.load(path)
        else:
            with path.open("rb") as handle:
                data = tomllib.load(handle)
        if not isinstance(data, dict):
            errors.append("config.example.toml did not parse to a table")
            return
        for key in ("ha", "display", "pages", "widgets", "tiles", "cache", "extensions"):
            if key not in data:
                errors.append(f"config.example.toml is missing [{key}]")
        if data.get("extensions", {}).get("enabled") is not False:
            errors.append("config.example.toml must ship with extensions.enabled = false")
    except Exception as exc:  # noqa: BLE001 - report the parser error as-is
        errors.append(f"config.example.toml is not valid TOML: {exc}")


def check_markdown() -> None:
    files = list(ROOT.glob("*.md")) + list((ROOT / "docs").rglob("*.md"))
    files += list((ROOT / ".github").rglob("*.md"))
    for path in files:
        rel = path.relative_to(ROOT)
        text = read(path)
        if text.count("```") % 2:
            errors.append(f"{rel}: unbalanced triple-backtick code fence")
        for match in LINK_RE.finditer(text):
            target = match.group(1).strip()
            if target.startswith(("http://", "https://", "#", "mailto:")):
                continue
            local = target.split("#", 1)[0]
            if not local:
                continue
            resolved = (path.parent / local).resolve()
            try:
                resolved.relative_to(ROOT.resolve())
            except ValueError:
                errors.append(f"{rel}: link escapes the repository: {target}")
                continue
            if not resolved.exists():
                errors.append(f"{rel}: broken local link: {target}")


def check_content() -> None:
    plan = read(ROOT / "PLAN.md")
    if "Wave 6" not in plan or "Community extension shop" not in plan:
        errors.append("PLAN.md is missing Wave 6 / community extension shop")
    agents = read(ROOT / "AGENTS.md")
    for needle in ("English only", "ABOYrbx", "Extension rules"):
        if needle not in agents:
            errors.append(f"AGENTS.md is missing: {needle}")
    readme = read(ROOT / "README.md")
    if "Repository rules" not in readme:
        errors.append("README.md is missing the repository rules section")
    design = read(ROOT / "DESIGN.md")
    if "Status: adopted" not in design:
        errors.append("DESIGN.md is missing its adopted status")
    tokens = read(ROOT / "tokens.css")
    for needle in ("--touch-target", "--muted", "--success", "--warn"):
        if needle not in tokens:
            errors.append(f"tokens.css is missing: {needle}")
    for wave in REQUIRED_WAVES:
        text = read(ROOT / wave)
        if "Acceptance gate" not in text:
            errors.append(f"{wave}: missing Acceptance gate")


def check_forbidden() -> None:
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        if ".git" in path.parts or "target" in path.parts:
            continue
        if path.resolve() == Path(__file__).resolve():
            continue
        if path.suffix.lower() not in {".md", ".toml", ".yml", ".yaml", ".sh", ".py", ".rs"}:
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for needle in FORBIDDEN:
            if needle in text:
                errors.append(f"{path.relative_to(ROOT)}: forbidden personal/provider marker: {needle}")


def main() -> int:
    check_required()
    check_toml()
    check_markdown()
    check_content()
    check_forbidden()

    for warning in warnings:
        print(f"warning: {warning}")
    for error in errors:
        print(f"error: {error}")
    if errors:
        print(f"\n{len(errors)} error(s)")
        return 1
    print("docs check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
