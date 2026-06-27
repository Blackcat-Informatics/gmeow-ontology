"""Acceptance tests for the repo-free ``gmeow validate <data>`` RDF path.

These drive the public CLI through :class:`~typer.testing.CliRunner` against the
bundled ``GTS_SNAPSHOT_FILE`` snapshot only — the same path an installed wheel
uses, with no ``slices/`` access — so they pin the V2 acceptance criteria:
exactly two errors and one warning with locations, all three output formats, and
a clean file exiting zero. The engine itself is pinned by the Rust tests in
``crates/validate/tests/data_validate.rs``; this file pins the wheel-resolution
and rendering layered on top.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest
from typer.testing import CliRunner

from gmeow_tools.cli import app

FIXTURES = Path(__file__).parent / "fixtures" / "validate"
FAIL = FIXTURES / "fail.nq"
CLEAN = FIXTURES / "clean.ttl"


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


def _findings(result_output: str) -> list[dict[str, Any]]:
    findings: list[dict[str, Any]] = json.loads(result_output)["findings"]
    return findings


def test_validate_rdf_reports_two_errors_one_warning_with_locations(
    runner: CliRunner,
) -> None:
    result = runner.invoke(app, ["validate", str(FAIL), "--format", "json"])
    assert result.exit_code == 1, result.output

    findings = _findings(result.output)
    severities = [f["severity"] for f in findings]
    assert severities.count("error") == 2, findings
    assert severities.count("warning") == 1, findings

    # Assert stable rule identity via (code, source-shape IRI) — not prose.
    # Shapes discovered from the Rust engine (data_validate.rs Step 1):
    #   disjointness: shacl.SPARQLConstraintComponent
    #                 + IdentityAxisOrthogonalityShape (error)
    #   commitment:   shacl.MinCountConstraintComponent + CommitmentShape (error)
    #   frame:        shacl.MinCountConstraintComponent
    #                 + EventFrameRequirementShape (warning)
    identity = [(f["severity"], f["code"], f.get("detail", "")) for f in findings]
    assert any(
        sev == "error"
        and code == "shacl.SPARQLConstraintComponent"
        and "IdentityAxisOrthogonalityShape" in detail
        for sev, code, detail in identity
    ), f"missing P9 disjointness error (IdentityAxisOrthogonalityShape): {identity}"
    assert any(
        sev == "error"
        and code == "shacl.MinCountConstraintComponent"
        and "CommitmentShape" in detail
        for sev, code, detail in identity
    ), f"missing Commitment-mediation error (CommitmentShape): {identity}"
    assert any(
        sev == "warning"
        and code == "shacl.MinCountConstraintComponent"
        and "EventFrameRequirementShape" in detail
        for sev, code, detail in identity
    ), f"missing frame-relativity warning (EventFrameRequirementShape): {identity}"

    # Every finding carries the data file as its physical location and a logical
    # (focus-node) anchor — the basis for SARIF artifact/logical locations.
    for finding in findings:
        locations = finding.get("locations") or []
        assert locations, finding
        primary = locations[0]
        assert primary.get("path") == str(FAIL)
        assert primary.get("logical")


def test_validate_rdf_human_format_exits_nonzero(runner: CliRunner) -> None:
    result = runner.invoke(app, ["validate", str(FAIL)])
    assert result.exit_code == 1
    # Human output names the offending constraints.
    assert "identity axis" in result.output or "reference frame" in result.output


def test_validate_rdf_sarif_is_well_formed(runner: CliRunner) -> None:
    result = runner.invoke(app, ["validate", str(FAIL), "--format", "sarif"])
    assert result.exit_code == 1
    sarif = json.loads(result.output)
    assert sarif["runs"], sarif
    results = sarif["runs"][0]["results"]
    assert len(results) == 3  # two errors + one warning


def test_validate_rdf_clean_file_passes(runner: CliRunner) -> None:
    result = runner.invoke(app, ["validate", str(CLEAN)])
    assert result.exit_code == 0, result.output
    assert "validation passed" in result.output


def test_validate_unknown_extension_hard_fails(
    runner: CliRunner, tmp_path: Path
) -> None:
    bogus = tmp_path / "data.csv"
    bogus.write_text("a,b,c\n")
    result = runner.invoke(app, ["validate", str(bogus)])
    assert result.exit_code != 0
    assert "cannot infer format" in result.output
