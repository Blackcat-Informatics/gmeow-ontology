# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""CLI wiring for the ``gmeow-dev feedback`` diagnostics-output knobs (#662).

These pin the config wiring — console mode, artifact selection, category
metadata, env precedence, and the exit-code invariant — with the heavyweight
gate surfaces (validate / reason / verify / surface fold) mocked out, so the
test is fast and deterministic and exercises only the option plumbing.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import pytest
from typer.testing import CliRunner

from gmeow_tools import diagnostics
from gmeow_tools.cli_dev import app as dev_app

runner = CliRunner()


@dataclass(slots=True)
class _FakeValidationResult:
    ok: bool = True
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    timings: list[dict[str, object]] = field(default_factory=list)
    report_json: str | None = None


@pytest.fixture
def _mock_gate(monkeypatch: pytest.MonkeyPatch) -> Any:
    """Mock every heavy feedback surface; return a setter for the validation result."""
    state: dict[str, _FakeValidationResult] = {
        "result": _FakeValidationResult(warnings=["a soft warning"])
    }

    def fake_validate_all(*args: Any, **kwargs: Any) -> _FakeValidationResult:
        return state["result"]

    def empty_report(*args: Any, **kwargs: Any) -> Any:
        return diagnostics.report("validate")

    monkeypatch.setattr("gmeow_tools.validate.validate_all", fake_validate_all)
    monkeypatch.setattr("gmeow_tools.reason.reason_native", empty_report)
    monkeypatch.setattr("gmeow_tools.reason.verify_native", empty_report)
    monkeypatch.setattr("gmeow_tools.cli_dev._fold_surfaces", lambda report: None)
    monkeypatch.setattr(
        "gmeow_tools.feedback_bundle.build_feedback_bundle", lambda report: b"BUNDLE"
    )
    return state


def test_feedback_writes_all_artifacts_by_default(
    _mock_gate: Any, tmp_path: Path
) -> None:
    result = runner.invoke(dev_app, ["feedback", "--diagnostics-dir", str(tmp_path)])

    assert result.exit_code == 0
    written = sorted(p.name for p in tmp_path.iterdir())
    assert written == [
        "gmeow-feedback.gts",
        "gmeow-feedback.html",
        "gmeow-feedback.json",
        "gmeow-feedback.sarif",
    ]


def test_feedback_artifacts_none_writes_only_the_bundle(
    _mock_gate: Any, tmp_path: Path
) -> None:
    result = runner.invoke(
        dev_app,
        [
            "feedback",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-artifacts",
            "none",
        ],
    )

    assert result.exit_code == 0
    # The .gts bundle is the canonical record (always written); the three
    # projections are suppressed by the `none` selection.
    assert sorted(p.name for p in tmp_path.iterdir()) == ["gmeow-feedback.gts"]


def test_feedback_artifacts_none_preserves_exit_code_on_failure(
    _mock_gate: Any, tmp_path: Path
) -> None:
    _mock_gate["result"] = _FakeValidationResult(ok=False, errors=["boom"])

    passing = runner.invoke(dev_app, ["feedback", "--diagnostics-dir", str(tmp_path)])
    suppressed = runner.invoke(
        dev_app,
        [
            "feedback",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-artifacts",
            "none",
        ],
    )

    # Output selection never moves the gate: a failing validation exits non-zero
    # whether or not artifacts are written.
    assert passing.exit_code == 1
    assert suppressed.exit_code == 1


def test_feedback_category_lands_in_sarif_automation_details(
    _mock_gate: Any, tmp_path: Path
) -> None:
    result = runner.invoke(
        dev_app,
        [
            "feedback",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-category",
            "lint",
        ],
    )

    assert result.exit_code == 0
    sarif = json.loads((tmp_path / "gmeow-feedback.sarif").read_text(encoding="utf-8"))
    assert sarif["runs"][0]["automationDetails"]["id"] == "lint"


def test_feedback_env_category_is_honored(
    _mock_gate: Any, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("GMEOW_DIAGNOSTICS_CATEGORY", "rust")
    result = runner.invoke(dev_app, ["feedback", "--diagnostics-dir", str(tmp_path)])

    assert result.exit_code == 0
    sarif = json.loads((tmp_path / "gmeow-feedback.sarif").read_text(encoding="utf-8"))
    assert sarif["runs"][0]["automationDetails"]["id"] == "rust"


def test_feedback_silent_console_suppresses_finding_lines(
    _mock_gate: Any, tmp_path: Path
) -> None:
    pretty = runner.invoke(
        dev_app,
        [
            "feedback",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-console",
            "pretty",
        ],
    )
    silent = runner.invoke(
        dev_app,
        [
            "feedback",
            "--diagnostics-dir",
            str(tmp_path),
            "--diagnostics-console",
            "silent",
        ],
    )

    assert "a soft warning" in pretty.output
    assert "a soft warning" not in silent.output
    # Artifacts are still written under silent (output mode != gating).
    assert (tmp_path / "gmeow-feedback.json").exists()
