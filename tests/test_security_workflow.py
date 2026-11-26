#!/usr/bin/env python3
"""Regression tests for the dependency audit workflow contract."""

from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = ROOT / ".github" / "workflows" / "security.yml"


class SecurityWorkflowTest(unittest.TestCase):
    def setUp(self) -> None:
        self.workflow = WORKFLOW.read_text(encoding="utf-8")

    def test_runs_for_pull_requests_and_weekly(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^  pull_request:\s*$")
        self.assertIn('    - cron: "0 6 * * 1"', self.workflow)

    def test_workflow_token_is_read_only(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^permissions:\n  contents: read\n")

    def test_sets_up_rust_before_installing_audit(self) -> None:
        self.assertIn("uses: dtolnay/rust-toolchain@stable", self.workflow)
        setup = self.workflow.index("uses: dtolnay/rust-toolchain@stable")
        install = self.workflow.index("cargo install cargo-audit")
        self.assertLess(setup, install)

    def test_audit_tool_is_pinned(self) -> None:
        self.assertIn(
            "run: cargo install cargo-audit --locked --version 0.23.0",
            self.workflow,
        )

    def test_audit_reads_committed_lockfile(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^        run: cargo audit --file Cargo\.lock$")


if __name__ == "__main__":
    unittest.main()
