#!/usr/bin/env python3
"""Validate the GitHub protection payloads and the settings script."""

from __future__ import annotations

import json
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "apply-github-settings.sh"
RULESET = ROOT / "scripts" / "github-protection.json"
CLASSIC = ROOT / "scripts" / "github-protection-classic.json"

EXPECTED_RULES = {
    "deletion",
    "non_fast_forward",
    "required_linear_history",
    "pull_request",
    "required_status_checks",
}


def load(path: Path) -> dict:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


class GithubSettingsTest(unittest.TestCase):
    def test_script_has_valid_syntax(self) -> None:
        subprocess.run(["bash", "-n", str(SCRIPT)], check=True)

    def test_script_uses_versioned_payloads(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("github-protection.json", text)
        self.assertIn("github-protection-classic.json", text)
        self.assertIn("set -euo pipefail", text)

    def test_ruleset_payload(self) -> None:
        data = load(RULESET)
        self.assertEqual(data["name"], "main-protection")
        self.assertEqual(data["target"], "branch")
        self.assertEqual(data["enforcement"], "active")
        self.assertEqual(
            data["conditions"]["ref_name"],
            {"include": ["~DEFAULT_BRANCH"], "exclude": []},
        )
        self.assertEqual(data["bypass_actors"], [])

        rules = {rule["type"]: rule for rule in data["rules"]}
        self.assertEqual(set(rules), EXPECTED_RULES)

        pull_request = rules["pull_request"]["parameters"]
        self.assertEqual(pull_request["required_approving_review_count"], 0)
        self.assertTrue(pull_request["required_review_thread_resolution"])
        self.assertTrue(pull_request["dismiss_stale_reviews_on_push"])
        self.assertEqual(pull_request["allowed_merge_methods"], ["squash"])
        self.assertFalse(pull_request["require_extra_approval_for_unattributed_changes"])

        checks = rules["required_status_checks"]["parameters"]
        self.assertTrue(checks["strict_required_status_checks_policy"])
        self.assertEqual(
            [check["context"] for check in checks["required_status_checks"]],
            ["docs"],
        )

    def test_classic_payload(self) -> None:
        data = load(CLASSIC)
        self.assertTrue(data["enforce_admins"])
        self.assertTrue(data["required_status_checks"]["strict"])
        self.assertEqual(data["required_status_checks"]["contexts"], ["docs"])
        self.assertEqual(
            data["required_pull_request_reviews"]["required_approving_review_count"],
            0,
        )
        self.assertTrue(data["required_linear_history"])
        self.assertFalse(data["allow_force_pushes"])
        self.assertFalse(data["allow_deletions"])
        self.assertTrue(data["required_conversation_resolution"])
        self.assertIsNone(data["restrictions"])


if __name__ == "__main__":
    unittest.main()
