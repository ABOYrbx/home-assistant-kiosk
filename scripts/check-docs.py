#!/usr/bin/env python3
"""Validate documentation, configuration, and release metadata before a push.

The check is intentionally dependency-free on GitHub-hosted Python. Rust code
is validated by the separate Cargo gates in the release checklist.
"""

from __future__ import annotations

import importlib.util
import ipaddress
import os
import re
import sys
from pathlib import Path
from urllib.parse import urlsplit

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
    "docs/deployment.md",
    "docs/extension-catalog.md",
    "docs/extensions.md",
    "schema/index.schema.json",
    "schema/manifest.schema.json",
    "docs/kiosk-display.md",
    "docs/agent-prompts/implement-issue.md",
    "docs/github-rules.md",
    "docs/herdr-agent.md",
    "docs/issues/index.md",
    "docs/performance.md",
    "docs/plan-review.md",
    "docs/release-checklist.md",
    "docs/troubleshooting.md",
    "scripts/apply-github-settings.sh",
    "scripts/github-protection.json",
    "scripts/github-protection-classic.json",
    "scripts/build-all.sh",
    "scripts/create-subtask-issues.py",
    "tests/deploy_scripts_test.sh",
    "tests/test_build_all.py",
    "herdr.sh",
]

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

RELEASE_PROFILE = {
    "opt-level": "s",
    "lto": True,
    "strip": True,
    "panic": "abort",
}

REQUIRED_WAVES = [f"docs/issues/wave-{i}.md" for i in range(7)]

FORBIDDEN = [
    "Dohlenweg",
    "fahrplanauskunft-mv",
    "VMV_EFA",
    "10.0.5.",
    "192.168.",
]

REVIEW_AREAS = (
    "License",
    "Permissions",
    "Secrets",
    "Home Assistant access",
    "UI",
    "Tests",
    "Removal",
)

REVIEW_EVIDENCE_ITEMS = (
    "License",
    "Permissions",
    "Secrets",
    "Home Assistant access",
    "UI",
    "Tests",
    "Removal",
)

REVIEW_DECISION_FIELDS = (
    "Reviewer",
    "Commit or package",
    "Decision",
    "Evidence",
    "Follow-up",
)

REVIEW_TEST_CASES = (
    "Successful refresh",
    "API failure",
    "Empty or missing data",
    "Forbidden host or entity",
    "Malformed output",
    "Resource limits",
    "Removal",
)

REMOVAL_ACTIONS = (
    "Disable",
    "Remove",
    "Catalog withdrawal",
    "Security withdrawal",
)

LINK_RE = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
URL_RE = re.compile(r"^(https?|wss?)://", re.IGNORECASE)

errors: list[str] = []
warnings: list[str] = []


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def markdown_sections(text: str) -> dict[str, str]:
    """Split Markdown by heading, retaining each section's own body."""
    sections: dict[str, list[str]] = {}
    current: str | None = None
    for line in text.splitlines():
        heading = re.fullmatch(r"(#{1,6})\s+(.+?)\s*#*\s*", line)
        if heading:
            current = heading.group(2).strip()
            sections.setdefault(current, [])
        elif current is not None:
            sections[current].append(line)
    return {heading: "\n".join(lines) for heading, lines in sections.items()}


def markdown_table_rows(text: str) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    for line in text.splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) < 2 or not cells[1]:
            continue
        label = cells[0]
        if not label or set(label) <= {"-", ":"} or label.lower() in {
            "area",
            "scenario",
            "action",
        }:
            continue
        rows.setdefault(label, []).append(cells[1])
    return rows


def validate_extension_review_doc(text: str) -> list[str]:
    """Return missing structural requirements in the community-review guide."""
    problems: list[str] = []
    sections = markdown_sections(text)
    review = sections.get("Community review")
    if review is None:
        problems.append("missing section: ## Community review")
        review = ""

    review_body = review.split("###", 1)[0]
    review_rows = markdown_table_rows(review_body)
    for area in REVIEW_AREAS:
        if area not in review_rows:
            problems.append(f"missing review area: {area}")

    evidence = sections.get("Submission evidence", "")
    evidence_labels = set(re.findall(r"^- \*\*([^*:]+):\*\*", evidence, re.MULTILINE))
    for item in REVIEW_EVIDENCE_ITEMS:
        if item not in evidence_labels:
            problems.append(f"missing submission evidence: {item}")

    decision = sections.get("Review decision and record", "")
    decision_labels = set(re.findall(r"^- ([^:\n]+):", decision, re.MULTILINE))
    for field in REVIEW_DECISION_FIELDS:
        if field not in decision_labels:
            problems.append(f"missing decision field: {field}")

    test_evidence = sections.get("Test evidence", "")
    test_rows = markdown_table_rows(test_evidence)
    for case in REVIEW_TEST_CASES:
        if case not in test_rows:
            problems.append(f"missing test scenario: {case}")

    isolation_text = test_evidence.lower()
    if not all(
        phrase in isolation_text
        for phrase in (
            "recorded fixtures",
            "never require a live home assistant",
            "credentials",
        )
    ):
        problems.append("test evidence must use fixtures without live Home Assistant access")

    removal_rows = markdown_table_rows(sections.get("Catalog removal and deprecation", ""))
    for action in REMOVAL_ACTIONS:
        if action not in removal_rows:
            problems.append(f"missing removal action: {action}")

    remove_effect = " ".join(removal_rows.get("Remove", [])).lower()
    retention_words = ("retained", "keep", "keeps", "kept", "left")
    if not (
        "settings" in remove_effect
        and "secret" in remove_effect
        and any(word in remove_effect for word in retention_words)
        and not re.search(r"\b(?:not|never)\s+(?:be\s+)?(?:retained|kept|left)\b", remove_effect)
    ):
        problems.append('removal action "Remove" must explain retained settings and secrets')

    withdrawal_effect = " ".join(removal_rows.get("Catalog withdrawal", [])).lower()
    if not (
        "installed" in withdrawal_effect
        and any(word in withdrawal_effect for word in ("remain", "kept", "keep"))
        and not re.search(r"\b(?:do|does|did|is|are|was|were)?\s*not\s+remain\b", withdrawal_effect)
    ):
        problems.append(
            'removal action "Catalog withdrawal" must explain installed-extension behavior'
        )

    return problems


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


def load_example_config() -> dict | None:
    path = ROOT / "config.example.toml"
    if not path.is_file():
        return None
    try:
        return load_toml(path)
    except Exception as exc:  # noqa: BLE001 - report the parser error as-is
        errors.append(f"config.example.toml is not valid TOML: {exc}")
        return None


def check_toml() -> None:
    data = load_example_config()
    if data is None:
        return
    if not isinstance(data, dict):
        errors.append("config.example.toml did not parse to a table")
        return
    for key in ("ha", "display", "pages", "widgets", "tiles", "cache", "extensions"):
        if key not in data:
            errors.append(f"config.example.toml is missing [{key}]")
    if data.get("extensions", {}).get("enabled") is not False:
        errors.append("config.example.toml must ship with extensions.enabled = false")
    check_example_safety(data)


def config_strings(value: object, prefix: str = ""):
    if isinstance(value, dict):
        for key, child in value.items():
            path = f"{prefix}.{key}" if prefix else key
            yield from config_strings(child, path)
    elif isinstance(value, list):
        for index, child in enumerate(value):
            path = f"{prefix}[{index}]" if prefix else f"[{index}]"
            yield from config_strings(child, path)
    elif isinstance(value, str):
        yield prefix, value


def is_placeholder_host(hostname: str) -> bool:
    host = hostname.rstrip(".").lower()
    if host == "localhost":
        return True
    try:
        ipaddress.ip_address(host)
    except ValueError:
        return host.endswith((".example", ".test", ".local"))
    return False


CANONICAL_CATALOG_URL = (
    "https://raw.githubusercontent.com/ABOYrbx/"
    "home-assistant-kiosk-extensions/main/index.json"
)


def check_example_safety(data: dict) -> None:
    ha = data.get("ha", {})
    if isinstance(ha, dict) and "token" in ha:
        errors.append(
            "config.example.toml must not set ha.token; keep the literal token "
            "example commented and use token_file or HA_TOKEN"
        )

    extensions = data.get("extensions", {})
    if isinstance(extensions, dict) and extensions.get("settings"):
        errors.append(
            "config.example.toml extensions.settings must remain empty; keep "
            "extension examples commented and secrets in the separate secret file"
        )

    email_pattern = re.compile(r"[^\s@]+@[^\s@]+\.[^\s@]+")
    personal_path_prefixes = ("/home/", "/Users/", "/root/")
    for path, value in config_strings(data):
        if value.startswith(personal_path_prefixes):
            errors.append(
                f"config.example.toml {path} contains a personal path; use a "
                "service path under /etc/ha-kiosk or /var/lib/ha-kiosk"
            )
        if email_pattern.search(value):
            errors.append(
                f"config.example.toml {path} contains a personal identifier"
            )
        if URL_RE.match(value):
            if value == CANONICAL_CATALOG_URL:
                continue
            try:
                hostname = urlsplit(value).hostname
            except ValueError:
                hostname = None
            if not hostname or not is_placeholder_host(hostname):
                errors.append(
                    f"config.example.toml {path} must use a placeholder URL host "
                    "(localhost or a .example, .test, or .local name)"
                )


def config_paths(value: object, prefix: str = "") -> list[str]:
    paths: list[str] = []
    if not isinstance(value, dict):
        return [prefix] if prefix else paths
    for key, child in value.items():
        path = f"{prefix}.{key}" if prefix else key
        if isinstance(child, dict):
            paths.extend(config_paths(child, path))
        elif isinstance(child, list) and child and isinstance(child[0], dict):
            paths.extend(config_paths(child, f"{path}[]"))
        else:
            paths.append(path)
    return paths


def check_config_reference() -> None:
    data = load_example_config()
    if not isinstance(data, dict):
        return
    readme = read(ROOT / "README.md")
    for path in config_paths(data):
        parts = path.split(".")
        if len(parts) == 3 and parts[0] == "pages":
            path = f"pages.<name>.{parts[2]}"
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
    troubleshooting = read(ROOT / "docs" / "troubleshooting.md")
    for topic in REQUIRED_TROUBLESHOOTING_TOPICS:
        if topic not in troubleshooting:
            errors.append(f"docs/troubleshooting.md is missing the topic: {topic}")
    checklist = read(ROOT / "docs" / "release-checklist.md")
    for item in REQUIRED_RELEASE_CHECKLIST_ITEMS:
        if item not in checklist:
            errors.append(f"docs/release-checklist.md is missing the item: {item}")
    review_doc = read(ROOT / "docs/extension-catalog.md")
    for problem in validate_extension_review_doc(review_doc):
        errors.append(f"docs/extension-catalog.md: {problem}")
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
    release = read(ROOT / "docs" / "release-builds.md")
    for needle in ("glibc 2.35", "Bookworm", "native build", "SOURCE_DATE_EPOCH"):
        if needle not in release:
            errors.append(f"docs/release-builds.md is missing: {needle}")
    for wave in REQUIRED_WAVES:
        text = read(ROOT / wave)
        if "Acceptance gate" not in text:
            errors.append(f"{wave}: missing Acceptance gate")


def check_deploy() -> None:
    unit = read(ROOT / "deploy" / "ha-kiosk.service")
    for needle in (
        "User=ha-kiosk",
        "Restart=on-failure",
        "RestartSec=5",
        "ProtectSystem=strict",
        "ReadWritePaths=/var/lib/ha-kiosk",
        "Environment=SLINT_BACKEND=linuxkms-software",
    ):
        if needle not in unit:
            errors.append(f"deploy/ha-kiosk.service is missing: {needle}")
    if "User=root" in unit:
        errors.append("deploy/ha-kiosk.service must not run as root")
    for rel in (
        "deploy/install.sh",
        "deploy/kiosk-setup.sh",
        "deploy/lib.sh",
        "deploy/uninstall.sh",
        "tests/deploy_scripts_test.sh",
    ):
        if not os.access(ROOT / rel, os.X_OK):
            errors.append(f"{rel} is not executable")


def check_forbidden() -> None:
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        if any(
            part in {".git", ".openchamber", ".opencode", ".design-studio", "node_modules", "target"}
            for part in path.parts
        ):
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
    check_deploy()
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
