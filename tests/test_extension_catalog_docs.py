#!/usr/bin/env python3
"""Tests for the community-review documentation contract validator."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHECK_DOCS = ROOT / "scripts" / "check-docs.py"

VALID_GUIDE = """# Extension Catalog Specification

## Community review

| Area | What is checked |
| --- | --- |
| License | SPDX identifier and license text. |
| Permissions | Minimal exact allowlists. |
| Secrets | No credentials or personal data. |
| Home Assistant access | Read-only declared entities; no service calls. |
| UI | Data-only output; no arbitrary UI. |
| Tests | Fixture-based evidence. |
| Removal | Disable, remove, and withdrawal behavior. |

### Submission evidence

- **License:** SPDX identifier and full license text.
- **Permissions:** Justification for every host and entity.
- **Secrets:** Redacted source, fixtures, docs, and screenshots.
- **Home Assistant access:** Read-only entity list; no service calls or token.
- **UI:** Declared data-only output kind.
- **Tests:** Commands, fixture results, and limitations.
- **Removal:** Operator steps and catalog withdrawal behavior.

### Reviewer checklist

- [ ] Review every area above.

### Review decision and record

- Reviewer: maintainer
- Commit or package: reviewed revision
- Decision: approve, request changes, or reject
- Evidence: CI and manual results
- Follow-up: required changes or none

### Test evidence

Use recorded fixtures; never require a live Home Assistant instance or credentials.

| Scenario | Expected evidence |
| --- | --- |
| Successful refresh | Expected data-only output. |
| API failure | Visible error and last good data retained. |
| Empty or missing data | Explicit empty or unavailable state. |
| Forbidden host or entity | Request rejected before access. |
| Malformed output | Output rejected without crashing the kiosk. |
| Resource limits | Operation and time limits enforced. |
| Removal | Installed files removed and retained data explained. |

## Catalog removal and deprecation

| Action | Effect |
| --- | --- |
| Disable | Stops execution while installed files and settings remain. |
| Remove | Removes all installed versions; settings and the secret file are retained. |
| Catalog withdrawal | Prevents new installs; installed extensions remain installed. |
| Security withdrawal | Publish a signed index and notice; operators disable or remove the extension. |
"""


def load_check_docs():
    spec = importlib.util.spec_from_file_location("check_docs", CHECK_DOCS)
    if spec is None or spec.loader is None:
        raise ImportError("cannot load scripts/check-docs.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ExtensionCatalogDocsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.check_docs = load_check_docs()

    def validate(self, guide: str) -> list[str]:
        return self.check_docs.validate_extension_review_doc(guide)

    def test_complete_review_contract_is_accepted(self) -> None:
        self.assertEqual(self.validate(VALID_GUIDE), [])

    def test_every_required_review_area_is_enforced(self) -> None:
        for area in (
            "License",
            "Permissions",
            "Secrets",
            "Home Assistant access",
            "UI",
            "Tests",
            "Removal",
        ):
            with self.subTest(area=area):
                rows = [
                    line
                    for line in VALID_GUIDE.splitlines()
                    if not line.startswith(f"| {area} |")
                ]
                errors = self.validate("\n".join(rows))
                self.assertIn(f"missing review area: {area}", errors)

    def test_review_area_cannot_be_satisfied_by_a_test_scenario(self) -> None:
        removal_review_row = "| Removal | Disable, remove, and withdrawal behavior. |"
        text = VALID_GUIDE.replace(removal_review_row, "")
        self.assertIn("missing review area: Removal", self.validate(text))

    def test_submission_and_decision_evidence_is_enforced(self) -> None:
        for label in (
            "License",
            "Permissions",
            "Secrets",
            "Home Assistant access",
            "UI",
            "Tests",
            "Removal",
        ):
            with self.subTest(evidence=label):
                lines = [
                    line
                    for line in VALID_GUIDE.splitlines()
                    if not line.startswith(f"- **{label}:**")
                ]
                errors = self.validate("\n".join(lines))
                self.assertIn(f"missing submission evidence: {label}", errors)

        for field in ("Reviewer", "Commit or package", "Decision", "Evidence", "Follow-up"):
            with self.subTest(decision=field):
                lines = [
                    line
                    for line in VALID_GUIDE.splitlines()
                    if not line.startswith(f"- {field}:")
                ]
                errors = self.validate("\n".join(lines))
                self.assertIn(f"missing decision field: {field}", errors)

    def test_fixture_based_test_matrix_is_enforced(self) -> None:
        for scenario in (
            "Successful refresh",
            "API failure",
            "Empty or missing data",
            "Forbidden host or entity",
            "Malformed output",
            "Resource limits",
            "Removal",
        ):
            with self.subTest(scenario=scenario):
                rows = [
                    line
                    for line in VALID_GUIDE.splitlines()
                    if not line.startswith(f"| {scenario} |")
                ]
                errors = self.validate("\n".join(rows))
                self.assertIn(f"missing test scenario: {scenario}", errors)

        without_isolation = VALID_GUIDE.replace(
            "Use recorded fixtures; never require a live Home Assistant instance or credentials.",
            "Run the tests.",
        )
        self.assertIn("test evidence must use fixtures without live Home Assistant access", self.validate(without_isolation))

    def test_removal_lifecycle_is_enforced(self) -> None:
        for action in ("Disable", "Remove", "Catalog withdrawal", "Security withdrawal"):
            with self.subTest(action=action):
                rows = [
                    line
                    for line in VALID_GUIDE.splitlines()
                    if not line.startswith(f"| {action} |")
                ]
                errors = self.validate("\n".join(rows))
                self.assertIn(f"missing removal action: {action}", errors)

    def test_removal_must_explain_retained_and_installed_data(self) -> None:
        without_retained_data = VALID_GUIDE.replace(
            "Removes all installed versions; settings and the secret file are retained.",
            "Removes the extension.",
        )
        self.assertIn(
            'removal action "Remove" must explain retained settings and secrets',
            self.validate(without_retained_data),
        )

        negated_retention = VALID_GUIDE.replace(
            "Removes all installed versions; settings and the secret file are retained.",
            "Removes all installed versions; settings and the secret file are not retained.",
        )
        self.assertIn(
            'removal action "Remove" must explain retained settings and secrets',
            self.validate(negated_retention),
        )

        without_installed_behavior = VALID_GUIDE.replace(
            "Prevents new installs; installed extensions remain installed.",
            "Prevents new installs.",
        )
        self.assertIn(
            'removal action "Catalog withdrawal" must explain installed-extension behavior',
            self.validate(without_installed_behavior),
        )

        negated_withdrawal = VALID_GUIDE.replace(
            "Prevents new installs; installed extensions remain installed.",
            "Prevents new installs; installed extensions do not remain installed.",
        )
        self.assertIn(
            'removal action "Catalog withdrawal" must explain installed-extension behavior',
            self.validate(negated_withdrawal),
        )

    def test_required_table_cells_must_not_be_empty(self) -> None:
        empty_review_cell = VALID_GUIDE.replace(
            "| License | SPDX identifier and license text. |",
            "| License | |",
        )
        self.assertIn("missing review area: License", self.validate(empty_review_cell))

        empty_test_cell = VALID_GUIDE.replace(
            "| Successful refresh | Expected data-only output. |",
            "| Successful refresh | |",
        )
        self.assertIn("missing test scenario: Successful refresh", self.validate(empty_test_cell))

        empty_removal_cell = VALID_GUIDE.replace(
            "| Disable | Stops execution while installed files and settings remain. |",
            "| Disable | |",
        )
        self.assertIn("missing removal action: Disable", self.validate(empty_removal_cell))


class ExtensionCatalogDocsIntegrationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.check_docs = load_check_docs()

    def test_actual_extension_catalog_doc_passes_review_contract(self) -> None:
        guide = (ROOT / "docs" / "extension-catalog.md").read_text()
        self.assertEqual(self.check_docs.validate_extension_review_doc(guide), [])


if __name__ == "__main__":
    unittest.main()
