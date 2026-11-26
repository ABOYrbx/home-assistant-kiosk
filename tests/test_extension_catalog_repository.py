#!/usr/bin/env python3
"""Contract tests for the separate public extension catalog repository."""

from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG_URL = (
    "https://raw.githubusercontent.com/ABOYrbx/"
    "home-assistant-kiosk-extensions/main/index.json"
)
REPOSITORY_URL = "https://github.com/ABOYrbx/home-assistant-kiosk-extensions"


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def extension_config() -> str:
    match = re.search(r"(?ms)^\[extensions\]\s*\n(.*?)(?=^\[|\Z)", read("config.example.toml"))
    if match is None:
        raise AssertionError("config.example.toml has no [extensions] section")
    return match.group(1)


class ExtensionCatalogRepositoryTest(unittest.TestCase):
    def test_example_config_uses_the_canonical_public_catalog(self) -> None:
        config = extension_config()
        self.assertIn(f'catalog_url = "{CATALOG_URL}"', config)
        self.assertRegex(config, r"(?m)^enabled = false$")
        self.assertRegex(config, r"(?m)^public_keys = \[\]$")

    def test_plan_records_public_visibility_and_offline_key_custody(self) -> None:
        plan = read("PLAN.md")
        section = plan.split("## 12. Open decisions", 1)[1].split("## 13.", 1)[0]
        item = next(line for line in section.splitlines() if line.startswith("7. **Catalog"))
        self.assertIn("public", item.lower())
        self.assertIn("ABOYrbx", item)
        self.assertIn("offline", item.lower())
        self.assertIn("Minisign private key", item)

    def test_docs_link_to_the_created_repository(self) -> None:
        for relative in ("README.md", "CONTRIBUTING.md", "docs/extension-catalog.md"):
            with self.subTest(path=relative):
                self.assertIn(REPOSITORY_URL, read(relative))
        for relative in ("docs/extension-catalog.md", "docs/github-rules.md"):
            with self.subTest(path=relative):
                self.assertNotIn("provisional name", read(relative).lower())
        self.assertNotIn("example/home-assistant-kiosk-extensions", read("docs/extension-catalog.md"))

    def test_bootstrap_signature_is_not_a_production_trust_anchor(self) -> None:
        spec = read("docs/extension-catalog.md").lower()
        self.assertIn("ephemeral bootstrap key", spec)
        self.assertIn("not a production trust anchor", spec)
        config = extension_config()
        self.assertRegex(config, r"(?m)^#.*bootstrap key")
        self.assertRegex(config, r"(?m)^public_keys = \[\]$")

    def test_catalog_data_and_secret_keys_are_not_tracked_here(self) -> None:
        tracked = subprocess.run(
            ["git", "ls-files"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.splitlines()
        forbidden = [
            path
            for path in tracked
            if path in {"index.json", "index.json.minisig", "catalog.pub"}
            or path.startswith("extensions/")
            or (path.endswith(".key") and not path.startswith("tests/fixtures/"))
        ]
        self.assertEqual(forbidden, [])


if __name__ == "__main__":
    unittest.main()
