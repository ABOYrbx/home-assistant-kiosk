#!/usr/bin/env python3
"""Regression tests for the documentation gate in scripts/check-docs.py."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "check-docs.py"

_SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
assert _SPEC is not None and _SPEC.loader is not None
check_docs = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(check_docs)


def valid_readme() -> str:
    return """# Home Assistant Kiosk

Pre-release status: no v1 release is published.

## Features

Home Assistant 2023.9 or newer is required.

## Supported hardware

Cross-built binaries require glibc 2.35 or newer.

## Installation

```bash
sudo deploy/install.sh --binary dist/ha-kiosk-example
sudo deploy/kiosk-setup.sh --rotation 0
```

## Configuration

### Configuration reference

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `ha.url` | string | required | Home Assistant URL. |

## Troubleshooting

See the troubleshooting guide.

## Security

See the security policy.

## Releases and changelog

Performance is not verified on hardware.

## Repository rules

Changes go through pull requests.

## License

The project license is not chosen yet.
"""


def content_fixtures(readme: str) -> dict[str, str]:
    fixtures = {
        "PLAN.md": "Wave 6 Community extension shop",
        "AGENTS.md": "English only ABOYrbx Extension rules",
        "README.md": readme,
        "troubleshooting.md": "glibc SLINT_BACKEND rotation 0600 2023.9 cross",
        "release-checklist.md": (
            "cargo fmt --check cargo clippy --all-targets -- -D warnings cargo test "
            "cargo audit SHA256SUMS CHANGELOG.md Known limitations Rollback"
        ),
        "CHANGELOG.md": "## [Unreleased]",
        "DESIGN.md": "Status: adopted",
        "tokens.css": "--touch-target --muted --success --warn",
        "release-builds.md": "glibc 2.35 Bookworm native build SOURCE_DATE_EPOCH",
    }
    for wave in check_docs.REQUIRED_WAVES:
        fixtures[Path(wave).name] = "Acceptance gate"
    return fixtures


class ReadmeStructureTest(unittest.TestCase):
    def setUp(self) -> None:
        check_docs.errors.clear()

    def run_content_check(self, readme: str) -> list[str]:
        fixtures = content_fixtures(readme)
        with patch.object(check_docs, "read", side_effect=lambda path: fixtures[path.name]):
            check_docs.check_content()
        return list(check_docs.errors)

    def test_duplicate_operator_section_is_rejected(self) -> None:
        readme = valid_readme() + "\n## Installation\n"
        errors = self.run_content_check(readme)
        self.assertTrue(any("duplicate README.md section" in error for error in errors), errors)

    def test_missing_home_assistant_minimum_is_rejected(self) -> None:
        readme = valid_readme().replace("Home Assistant 2023.9 or newer is required.", "")
        errors = self.run_content_check(readme)
        self.assertTrue(any("2023.9" in error for error in errors), errors)

    def test_unverified_hardware_status_is_required(self) -> None:
        readme = valid_readme().replace("Performance is not verified on hardware.", "")
        errors = self.run_content_check(readme)
        self.assertTrue(any("not verified on hardware" in error for error in errors), errors)

    def test_obsolete_positional_installer_is_rejected(self) -> None:
        readme = valid_readme().replace(
            "sudo deploy/install.sh --binary dist/ha-kiosk-example",
            "sudo ./deploy/install.sh /path/to/ha-kiosk-example",
        )
        errors = self.run_content_check(readme)
        self.assertTrue(any("deploy/install.sh" in error for error in errors), errors)


class ConfigReferenceTest(unittest.TestCase):
    def setUp(self) -> None:
        check_docs.errors.clear()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "config.example.toml").write_text("", encoding="utf-8")

    def check_reference(self, config: dict, readme: str) -> list[str]:
        with (
            patch.object(check_docs, "ROOT", self.root),
            patch.object(check_docs, "load_toml", return_value=config),
            patch.object(check_docs, "read", return_value=readme),
        ):
            check_docs.check_config_reference()
        return list(check_docs.errors)

    def test_key_mentioned_only_in_prose_is_not_a_table_row(self) -> None:
        config = {"ha": {"url": "http://example.test"}, "widgets": {"hero_light": {"enabled": True}}}
        readme = valid_readme() + "\n`widgets.hero_light.enabled` is mentioned here.\n"
        errors = self.check_reference(config, readme)
        self.assertTrue(
            any("configuration reference is missing: widgets.hero_light.enabled" in error for error in errors),
            errors,
        )

    def test_page_specific_keys_accept_generic_table_rows(self) -> None:
        config = {"pages": {"overview": {"title": "Home", "widgets": ["clock"]}}}
        readme = valid_readme().replace(
            "| `ha.url` | string | required | Home Assistant URL. |",
            "| `pages.<name>.title` | string | empty | Page title. |\n"
            "| `pages.<name>.widgets` | array of strings | per page | Widgets. |",
        )
        errors = self.check_reference(config, readme)
        self.assertEqual(errors, [])


if __name__ == "__main__":
    unittest.main()
