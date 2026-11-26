#!/usr/bin/env python3
"""Regression tests for the documentation and release gate."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "check-docs.py"

_SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
assert _SPEC is not None and _SPEC.loader is not None
check_docs = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(check_docs)


class CheckDocsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.addCleanup(setattr, check_docs, "ROOT", check_docs.ROOT)
        self.addCleanup(setattr, check_docs, "errors", check_docs.errors)
        check_docs.ROOT = self.tmp
        check_docs.errors = []

    def write(self, relative: str, text: str) -> Path:
        path = self.tmp / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def test_release_profile_accepts_the_planned_values(self) -> None:
        self.write(
            "Cargo.toml",
            """[package]
name = "fixture"
version = "1.0.0"

[profile.release]
opt-level = "s"
lto = true
strip = true
panic = "abort"
""",
        )

        check_docs.check_cargo_profile()

        self.assertEqual(check_docs.errors, [])

    def test_release_profile_rejects_drift(self) -> None:
        self.write(
            "Cargo.toml",
            """[package]
name = "fixture"
version = "1.0.0"

[profile.release]
opt-level = 3
lto = true
strip = true
panic = "abort"
""",
        )

        check_docs.check_cargo_profile()

        self.assertTrue(
            any("[profile.release] opt-level" in error for error in check_docs.errors),
            check_docs.errors,
        )

    def test_main_runs_the_release_profile_and_performance_checks(self) -> None:
        calls: list[str] = []
        checks = (
            "check_required",
            "check_toml",
            "check_config_reference",
            "check_cargo_profile",
            "check_performance_doc",
            "check_markdown",
            "check_content",
            "check_deploy",
            "check_forbidden",
        )
        patches = [
            mock.patch.object(
                check_docs,
                name,
                side_effect=lambda name=name: calls.append(name),
            )
            for name in checks
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

        with contextlib.redirect_stdout(io.StringIO()):
            result = check_docs.main()

        self.assertEqual(result, 0)
        self.assertEqual(
            calls,
            [
                "check_required",
                "check_toml",
                "check_config_reference",
                "check_cargo_profile",
                "check_performance_doc",
                "check_markdown",
                "check_content",
                "check_deploy",
                "check_forbidden",
            ],
        )

    def test_config_paths_includes_nested_page_and_extension_keys(self) -> None:
        paths = check_docs.config_paths(
            {
                "pages": {"overview": {"title": "Home", "widgets": ["clock"]}},
                "extensions": {"store_path": "/var/lib/fixture"},
            }
        )

        self.assertIn("pages.overview.title", paths)
        self.assertIn("pages.overview.widgets", paths)
        self.assertIn("extensions.store_path", paths)

    def test_page_keys_use_generic_reference_names(self) -> None:
        self.write("config.example.toml", '[pages.overview]\ntitle = "Home"\n')
        self.write("README.md", "# Fixture\n\n`pages.<name>.title`\n")

        check_docs.check_config_reference()

        self.assertEqual(check_docs.errors, [])

    def test_missing_readme_key_is_reported(self) -> None:
        self.write("config.example.toml", '[widgets.clock]\nenabled = true\n')
        self.write("README.md", "# Fixture\n")

        check_docs.check_config_reference()

        self.assertIn(
            "README.md configuration reference is missing: widgets.clock.enabled",
            check_docs.errors,
        )

    def test_performance_budget_drift_is_reported(self) -> None:
        shutil.copyfile(ROOT / "scripts" / "perf-budget.py", self.tmp / "perf-budget.py")
        (self.tmp / "scripts").mkdir()
        shutil.move(self.tmp / "perf-budget.py", self.tmp / "scripts" / "perf-budget.py")
        self.write("docs/performance.md", "# Performance\n")

        check_docs.check_performance_doc()

        self.assertTrue(
            any("does not state the budget" in error for error in check_docs.errors),
            check_docs.errors,
        )



BASE_CONFIG = """
[ha]
url = "http://homeassistant.example:8123"
token_file = "/etc/ha-kiosk/ha_token"
allow_invalid_certs = false
extra_entity_ids = []

[display]
width = 800
height = 480
rotation = 0
fullscreen = true
theme = "dark"

[pages.overview]
title = "Home"
widgets = ["clock"]

[pages.media]
widgets = ["media"]

[pages.agenda]
widgets = ["calendar"]

[pages.status]
widgets = ["status"]

[widgets.clock]
enabled = true

[tiles]
columns = 3

[[tiles.items]]
entity = "light.example"
label = "Example light"
icon = "light"

[cache]
path = "/var/lib/ha-kiosk/state.json"
write_debounce_seconds = 10
max_bytes = 262144

[extensions]
enabled = false
catalog_url = "https://catalog.example/index.json"
public_keys = []
allow_unsigned = false
dev_mode = false
refresh_hours = 24
max_catalog_bytes = 2097152
max_package_bytes = 5242880
max_script_operations = 1000000
store_path = "/var/lib/ha-kiosk/extensions"
"""


class ExampleConfigSafetyTest(unittest.TestCase):
    def assert_unsafe(self, content: str, expected_error: str) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            (root / "config.example.toml").write_text(content, encoding="utf-8")
            with (
                patch.object(check_docs, "ROOT", root),
                patch.object(check_docs, "errors", []),
            ):
                check_docs.check_toml()
                self.assertTrue(
                    any(expected_error in error for error in check_docs.errors),
                    check_docs.errors,
                )

    def test_example_rejects_active_literal_token(self) -> None:
        content = BASE_CONFIG.replace(
            'token_file = "/etc/ha-kiosk/ha_token"',
            'token = "test-token"',
        )
        self.assert_unsafe(content, "must not set ha.token")

    def test_example_rejects_non_placeholder_url(self) -> None:
        content = BASE_CONFIG.replace(
            "http://homeassistant.example:8123",
            "https://homeassistant.example.org:8123",
        )
        self.assert_unsafe(content, "placeholder URL")

    def test_example_rejects_non_placeholder_catalog_url(self) -> None:
        content = BASE_CONFIG.replace(
            "https://catalog.example/index.json",
            "https://catalog.real-vendor.example.org/index.json",
        )
        self.assert_unsafe(content, "placeholder URL")

    def test_example_rejects_personal_path(self) -> None:
        content = BASE_CONFIG.replace(
            'token_file = "/etc/ha-kiosk/ha_token"',
            'token_file = "/home/alice/ha.token"',
        )
        self.assert_unsafe(content, "personal path")

    def test_example_rejects_personal_email(self) -> None:
        content = BASE_CONFIG.replace('title = "Home"', 'title = "alice@example.com"')
        self.assert_unsafe(content, "personal identifier")

    def test_example_rejects_active_extension_settings(self) -> None:
        content = BASE_CONFIG + """
[extensions.settings."org.example.bus-departures"]
api_key = "test-key"
"""
        self.assert_unsafe(content, "extensions.settings must remain empty")


if __name__ == "__main__":
    unittest.main()
