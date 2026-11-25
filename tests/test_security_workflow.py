#!/usr/bin/env python3
"""Regression tests for the CI and dependency-audit workflow contracts.

The repository only permits GitHub-owned actions (selected-actions policy in
`docs/github-rules.md`). A third-party action makes the whole workflow fail at
startup, so the workflows use the runner's rustup instead.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WORKFLOWS = ROOT / ".github" / "workflows"


def read(name: str) -> str:
    return (WORKFLOWS / name).read_text(encoding="utf-8")


def uses_lines(workflow: str) -> list[str]:
    return re.findall(r"(?m)^\s*uses:\s*(\S+)\s*$", workflow)


class SecurityWorkflowTest(unittest.TestCase):
    def setUp(self) -> None:
        self.workflow = read("security.yml")

    def test_runs_for_pull_requests_and_weekly(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^  pull_request:\s*$")
        self.assertIn('    - cron: "0 6 * * 1"', self.workflow)

    def test_workflow_token_is_read_only(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^permissions:\n  contents: read\n")

    def test_uses_only_github_owned_actions(self) -> None:
        for action in uses_lines(self.workflow):
            self.assertTrue(
                action.startswith("actions/"),
                f"third-party action {action!r} is not allowed by the repository policy",
            )

    def test_sets_up_rust_before_installing_audit(self) -> None:
        self.assertIn("rustup toolchain install stable", self.workflow)
        setup = self.workflow.index("rustup toolchain install stable")
        install = self.workflow.index("cargo install cargo-audit")
        self.assertLess(setup, install)

    def test_audit_tool_is_pinned(self) -> None:
        self.assertIn(
            "run: cargo install cargo-audit --locked --version 0.23.0",
            self.workflow,
        )

    def test_audit_reads_committed_lockfile(self) -> None:
        self.assertRegex(self.workflow, r"(?m)^        run: cargo audit --file Cargo\.lock$")


class CiWorkflowTest(unittest.TestCase):
    def setUp(self) -> None:
        self.workflow = read("ci.yml")

    def test_uses_only_github_owned_actions(self) -> None:
        for action in uses_lines(self.workflow):
            self.assertTrue(
                action.startswith("actions/"),
                f"third-party action {action!r} is not allowed by the repository policy",
            )

    def test_installs_the_required_rust_components_with_rustup(self) -> None:
        self.assertIn("rustup toolchain install stable", self.workflow)
        self.assertIn("rustup component add rustfmt clippy", self.workflow)


if __name__ == "__main__":
    unittest.main()
