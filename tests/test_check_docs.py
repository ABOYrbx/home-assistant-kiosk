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
        self.assertTrue(any("duplicate section" in error for error in errors), errors)

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


VALID_CARGO = """[package]
name = "profile-fixture"
version = "0.0.0"
edition = "2024"

[profile.release]
opt-level = "s"
lto = true
strip = true
panic = "abort"
"""

DRIFTED_PROFILE_VALUES = {
    "opt-level": '"z"',
    "lto": "false",
    "strip": "false",
    "panic": '"unwind"',
}

VALID_PROFILE_LINES = {
    "opt-level": '"s"',
    "lto": "true",
    "strip": "true",
    "panic": '"abort"',
}


class ReleaseProfileTest(unittest.TestCase):
    def setUp(self) -> None:
        check_docs.errors.clear()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.cargo = self.root / "Cargo.toml"
        self.cargo.write_text(VALID_CARGO, encoding="utf-8")

    def run_check(self) -> list[str]:
        with patch.object(check_docs, "ROOT", self.root):
            check_docs.check_cargo_profile()
        return list(check_docs.errors)

    def test_valid_profile_is_accepted(self) -> None:
        self.assertEqual(self.run_check(), [])

    def test_missing_profile_table_fails(self) -> None:
        self.cargo.write_text(
            VALID_CARGO.split("[profile.release]")[0], encoding="utf-8"
        )
        errors = self.run_check()
        self.assertTrue(
            any("missing [profile.release]" in error for error in errors), errors
        )

    def test_malformed_cargo_manifest_fails(self) -> None:
        self.cargo.write_text(VALID_CARGO + "\nnot valid toml\n", encoding="utf-8")
        errors = self.run_check()
        self.assertTrue(any("not valid TOML" in error for error in errors), errors)

    def test_each_drifted_profile_value_fails(self) -> None:
        for key, wrong_value in DRIFTED_PROFILE_VALUES.items():
            with self.subTest(key=key):
                content = VALID_CARGO.replace(
                    f"{key} = {VALID_PROFILE_LINES[key]}",
                    f"{key} = {wrong_value}",
                )
                self.cargo.write_text(content, encoding="utf-8")
                errors = self.run_check()
                self.assertTrue(
                    any(
                        f"[profile.release] {key} must be" in error
                        for error in errors
                    ),
                    errors,
                )
                check_docs.errors.clear()


BASE_EXAMPLE_CONFIG = """
[ha]
url = "http://homeassistant.example:8123"
token_file = "/etc/ha-kiosk/ha_token"
allow_invalid_certs = false
extra_entity_ids = []

[pages.overview]
title = "Home"
widgets = ["clock"]

[tiles]
columns = 3

[[tiles.items]]
entity = "light.example"
label = "Example light"
icon = "light"

[extensions]
enabled = false
catalog_url = "https://catalog.example/index.json"
public_keys = []
store_path = "/var/lib/ha-kiosk/extensions"
"""


class ExampleConfigSafetyTest(unittest.TestCase):
    def setUp(self) -> None:
        check_docs.errors.clear()
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)

    def run_check(self, content: str) -> list[str]:
        (self.root / "config.example.toml").write_text(content, encoding="utf-8")
        check_docs.errors.clear()
        with patch.object(check_docs, "ROOT", self.root):
            check_docs.check_example_config()
        return list(check_docs.errors)

    def test_repository_example_is_safe(self) -> None:
        content = (ROOT / "config.example.toml").read_text(encoding="utf-8")
        self.assertEqual(self.run_check(content), [])

    def test_active_token_is_rejected(self) -> None:
        content = BASE_EXAMPLE_CONFIG.replace(
            'token_file = "/etc/ha-kiosk/ha_token"',
            'token_file = "/etc/ha-kiosk/ha_token"\ntoken = "test-token"',
        )
        errors = self.run_check(content)
        self.assertTrue(
            any("must not set ha.token" in error for error in errors), errors
        )

    def test_non_placeholder_url_is_rejected(self) -> None:
        content = BASE_EXAMPLE_CONFIG.replace(
            "http://homeassistant.example:8123",
            "https://homeassistant.internal:8123",
        )
        errors = self.run_check(content)
        self.assertTrue(
            any("placeholder host" in error for error in errors), errors
        )

    def test_personal_path_is_rejected(self) -> None:
        content = BASE_EXAMPLE_CONFIG.replace(
            'token_file = "/etc/ha-kiosk/ha_token"',
            'token_file = "/home/alice/ha.token"',
        )
        errors = self.run_check(content)
        self.assertTrue(any("personal path" in error for error in errors), errors)

    def test_personal_identifier_is_rejected(self) -> None:
        content = BASE_EXAMPLE_CONFIG.replace(
            'title = "Home"', 'title = "alice@example.com"'
        )
        errors = self.run_check(content)
        self.assertTrue(
            any("personal identifier" in error for error in errors), errors
        )

    def test_active_extension_settings_are_rejected(self) -> None:
        content = BASE_EXAMPLE_CONFIG + """
[extensions.settings."org.example.bus-departures"]
api_key = "test-key"
"""
        errors = self.run_check(content)
        self.assertTrue(
            any("extensions.settings must remain empty" in error for error in errors),
            errors,
        )


if __name__ == "__main__":
    unittest.main()
