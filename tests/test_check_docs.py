from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_docs", ROOT / "scripts" / "check-docs.py"
)
assert SPEC is not None and SPEC.loader is not None
check_docs = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(check_docs)

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
