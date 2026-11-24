#!/usr/bin/env python3
"""Validate the planning repository before a push or pull request.

This check is intentionally dependency-free on GitHub-hosted Python. It does
not validate Rust code; the Rust gate arrives in Wave 0.
"""

from __future__ import annotations

import importlib.util
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

REQUIRED_FILES = [
    "AGENTS.md",
    "CHANGELOG.md",
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
    "docs/agent-prompts/implement-issue.md",
    "docs/github-rules.md",
    "docs/herdr-agent.md",
    "docs/issues/index.md",
    "docs/performance.md",
    "docs/plan-review.md",
    "docs/release-checklist.md",
    "docs/troubleshooting.md",
    "scripts/apply-github-settings.sh",
    "scripts/create-subtask-issues.py",
    "scripts/perf-budget.py",
    "tests/test_perf_budget.py",
    "herdr.sh",
]

RELEASE_PROFILE = {
    "opt-level": "s",
    "lto": True,
    "strip": True,
    "panic": "abort",
}

REQUIRED_WAVES = [f"docs/issues/wave-{i}.md" for i in range(7)]

REQUIRED_README_SECTIONS = [
    "## Installation",
    "### Configuration reference",
    "## Troubleshooting",
    "## Security",
    "## Releases and changelog",
    "## License",
]

REQUIRED_TROUBLESHOOTING_TOPICS = [
    "glibc",
    "SLINT_BACKEND",
    "rotation",
    "0600",
    "2023.9",
    "cross",
]

REQUIRED_RELEASE_CHECKLIST_ITEMS = [
    "cargo fmt --check",
    "cargo clippy --all-targets -- -D warnings",
    "cargo test",
    "cargo audit",
    "SHA256SUMS",
    "CHANGELOG.md",
    "Known limitations",
    "Rollback",
]

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


def load_toml(path: Path):
    try:
        import tomllib  # Python 3.11+
    except ImportError:  # Python 3.9/3.10 on a developer machine
        import toml  # type: ignore

        return toml.load(path)
    with path.open("rb") as handle:
        return tomllib.load(handle)


def check_required() -> None:
    for rel in REQUIRED_FILES + REQUIRED_WAVES:
        if not (ROOT / rel).is_file():
            errors.append(f"missing required file: {rel}")


def check_toml() -> None:
    path = ROOT / "config.example.toml"
    if not path.is_file():
        return
    try:
        data = load_toml(path)
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


def config_paths(value: object, prefix: str = "") -> list[str]:
    paths: list[str] = []
    if not isinstance(value, dict):
        return [prefix] if prefix else paths
    for key, child in value.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(child, dict):
            paths.extend(config_paths(child, path))
        elif isinstance(child, list) and child and isinstance(child[0], dict):
            paths.extend(config_paths(child[0], f"{path}[]"))
        else:
            paths.append(path)
    return paths


def check_config_reference() -> None:
    path = ROOT / "config.example.toml"
    if not path.is_file():
        return
    try:
        data = load_toml(path)
    except Exception:  # noqa: BLE001 - check_toml already reports this
        return
    if not isinstance(data, dict):
        return
    readme = read(ROOT / "README.md")
    for path in config_paths(data):
        if path.startswith("pages.") and path.endswith(".widgets"):
            path = "pages.<name>.widgets"
        if path not in readme:
            errors.append(f"README.md configuration reference is missing: {path}")


def check_cargo_profile() -> None:
    path = ROOT / "Cargo.toml"
    if not path.is_file():
        return
    try:
        data = load_toml(path)
    except Exception as exc:  # noqa: BLE001 - report the parser error as-is
        errors.append(f"Cargo.toml is not valid TOML: {exc}")
        return
    profile = data.get("profile", {}).get("release") if isinstance(data, dict) else None
    if not isinstance(profile, dict):
        errors.append("Cargo.toml is missing [profile.release]")
        return
    for key, expected in RELEASE_PROFILE.items():
        if profile.get(key) != expected:
            errors.append(
                f"Cargo.toml [profile.release] {key} must be {expected!r} "
                "(PLAN.md section 8, Wave 5)"
            )


def check_performance_doc() -> None:
    doc = ROOT / "docs" / "performance.md"
    script = ROOT / "scripts" / "perf-budget.py"
    if not doc.is_file() or not script.is_file():
        return
    try:
        spec = importlib.util.spec_from_file_location("perf_budget", script)
        if spec is None or spec.loader is None:
            raise ImportError("cannot load scripts/perf-budget.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    except Exception as exc:  # noqa: BLE001 - report the loader error as-is
        errors.append(f"scripts/perf-budget.py could not be loaded: {exc}")
        return
    text = read(doc)
    budgets = [
        f"{module.RSS_HARD_MB:.0f} MB",
        f"{module.RSS_TARGET_MB:.0f} MB",
        f"{module.FIRST_FRAME_HARD_MS:.0f} ms",
        f"{module.IDLE_CPU_HARD_PERCENT:.0f} %",
        f"{module.SOAK_MIN_HOURS:.0f} h",
        f"{module.SOAK_GROWTH_HARD_MB:.0f} MB",
    ]
    for budget in budgets:
        if budget not in text:
            errors.append(f"docs/performance.md does not state the budget: {budget}")


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
    for section in REQUIRED_README_SECTIONS:
        if section not in readme:
            errors.append(f"README.md is missing the section: {section}")
    troubleshooting = read(ROOT / "docs/troubleshooting.md")
    for topic in REQUIRED_TROUBLESHOOTING_TOPICS:
        if topic not in troubleshooting:
            errors.append(f"docs/troubleshooting.md is missing the topic: {topic}")
    checklist = read(ROOT / "docs/release-checklist.md")
    for item in REQUIRED_RELEASE_CHECKLIST_ITEMS:
        if item not in checklist:
            errors.append(f"docs/release-checklist.md is missing the item: {item}")
    changelog = read(ROOT / "CHANGELOG.md")
    if "## [Unreleased]" not in changelog:
        errors.append("CHANGELOG.md is missing an Unreleased section")
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
    check_config_reference()
    check_cargo_profile()
    check_performance_doc()
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
