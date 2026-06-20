# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Tests for wrapping external gate tools as canonical findings (#662)."""

from __future__ import annotations

import json
import sys
from pathlib import Path

from typer.testing import CliRunner

from gmeow_tools import external_tool
from gmeow_tools.cli_dev import app as dev_app

runner = CliRunner()


def test_success_yields_an_empty_report() -> None:
    report = external_tool.to_diagnostics_report("mypy", ["mypy", "src"], 0, "ok", "")

    assert report.ok
    assert report.finding_count == 0


def test_failure_yields_one_error_finding_with_raw_log() -> None:
    report = external_tool.to_diagnostics_report(
        "mypy",
        ["mypy", "src"],
        1,
        "src/x.py:3: error: bad type",
        "Found 1 error",
    )

    assert not report.ok
    assert report.error_count == 1
    item = report.findings[0]
    assert item["code"] == "external.mypy"
    assert item["tool"] == "mypy"
    # The raw log is preserved in the detail.
    assert "src/x.py:3: error: bad type" in item["detail"]
    assert "Found 1 error" in item["detail"]


def test_large_log_is_digested_deterministically() -> None:
    big = "E" * 20_000
    first = external_tool.to_diagnostics_report("pytest", ["pytest"], 1, big, "")
    second = external_tool.to_diagnostics_report("pytest", ["pytest"], 1, big, "")

    detail = first.findings[0]["detail"]
    # Bounded and stamped with a deterministic full-content hash.
    assert "sha256=" in detail
    assert "elided" in detail
    assert len(detail) < len(big)
    # Same input -> identical detail (determinism).
    assert detail == second.findings[0]["detail"]


def test_small_log_is_kept_verbatim() -> None:
    report = external_tool.to_diagnostics_report(
        "clippy", ["cargo", "clippy"], 1, "boom", ""
    )
    detail = report.findings[0]["detail"]
    assert "sha256=" not in detail
    assert "boom" in detail


def test_run_external_tool_captures_a_real_failure() -> None:
    report = external_tool.run_external_tool(
        "probe",
        [sys.executable, "-c", "import sys; sys.stderr.write('nope'); sys.exit(2)"],
    )

    assert not report.ok
    assert report.findings[0]["code"] == "external.probe"
    assert "nope" in report.findings[0]["detail"]


def test_run_external_tool_missing_binary_is_a_finding_not_a_crash() -> None:
    report = external_tool.run_external_tool(
        "ghost", ["this-binary-does-not-exist-xyz"]
    )

    assert not report.ok
    assert report.findings[0]["code"] == "external.ghost"


def test_cli_external_tool_failure_exit_code_and_sarif(tmp_path: Path) -> None:
    result = runner.invoke(
        dev_app,
        [
            "external-tool",
            "--name",
            "probe",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-category",
            "python",
            "--",
            sys.executable,
            "-c",
            "import sys; sys.exit(3)",
        ],
    )

    # Exit code mirrors the wrapped tool's failure.
    assert result.exit_code == 1
    sarif = json.loads((tmp_path / "gmeow-feedback.sarif").read_text(encoding="utf-8"))
    assert sarif["runs"][0]["automationDetails"]["id"] == "python"
    assert sarif["runs"][0]["results"][0]["ruleId"] == "external.probe"


def test_cli_external_tool_success_exit_zero(tmp_path: Path) -> None:
    result = runner.invoke(
        dev_app,
        [
            "external-tool",
            "--name",
            "probe",
            "--diagnostics-dir",
            str(tmp_path),
            "--",
            sys.executable,
            "-c",
            "pass",
        ],
    )

    assert result.exit_code == 0
