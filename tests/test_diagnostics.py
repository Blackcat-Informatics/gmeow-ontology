"""Tests for the shared diagnostics reporting facade."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

import gmeow_diagnostics

from gmeow_tools import diagnostics


@dataclass(slots=True)
class SyntheticValidationResult:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    timings: list[dict[str, object]] = field(default_factory=list)


def test_native_report_renders_json_sarif_and_html() -> None:
    report = gmeow_diagnostics.Report("validate")
    report.add(
        gmeow_diagnostics.Finding(
            "warning",
            "validate.example",
            "synthetic warning",
            path="slices/core/example/module.ttl",
            line=7,
            column=2,
        )
    )

    payload = json.loads(report.to_json())
    sarif = json.loads(report.to_sarif())
    html = report.to_html()

    assert payload["findings"][0]["code"] == "validate.example"
    assert sarif["version"] == "2.1.0"
    assert sarif["runs"][0]["results"][0]["ruleId"] == "validate.example"
    assert "synthetic warning" in html


def test_validation_result_facade_preserves_legacy_lists() -> None:
    result = SyntheticValidationResult(
        errors=["missing skos:definition"],
        warnings=["docs.md has no anchors"],
        timings=[{"phase": "synthetic", "elapsed_ms": 1}],
    )

    report = diagnostics.report_from_validation_result(result)
    payload = json.loads(report.to_json())

    assert report.errors == ["missing skos:definition"]
    assert report.warnings == ["docs.md has no anchors"]
    assert payload["metadata"]["timings"][0]["phase"] == "synthetic"


@dataclass(slots=True)
class ReportJsonValidationResult:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    timings: list[dict[str, object]] = field(default_factory=list)
    report_json: str | None = None


def test_facade_uses_report_json_with_wire_coords() -> None:
    # A structured report carrying a GTS wire coordinate, as the Rust
    # orchestration emits it.
    source = gmeow_diagnostics.Report("validate")
    source.add(
        gmeow_diagnostics.Finding(
            "error",
            "shacl.MinCount",
            "missing property",
            logical="gts:quad",
        )
    )
    result = ReportJsonValidationResult(
        errors=["missing property"],
        warnings=[],
        report_json=source.to_json(),
    )

    report = diagnostics.report_from_validation_result(result)
    sarif = json.loads(report.to_sarif())

    # report_json is authoritative: the structured finding survives round-trip.
    assert report.errors == ["missing property"]
    assert sarif["runs"][0]["results"][0]["ruleId"] == "shacl.MinCount"


def test_gmeow_rdf_projection_parses_in_pyoxigraph() -> None:
    """The gmeow: RDF projection is valid N-Quads with one Finding per finding."""
    import pyoxigraph

    report = gmeow_diagnostics.Report("validate")
    report.add(
        gmeow_diagnostics.Finding(
            "error",
            "shacl.MinCount",
            "missing property",
            tool="shacl",
            logical="gts:quad",
        )
    )

    nquads = report.to_gmeow_rdf()
    quads = list(
        pyoxigraph.parse(nquads.encode("utf-8"), format=pyoxigraph.RdfFormat.N_QUADS)
    )

    diagnostics_graph = pyoxigraph.NamedNode(
        "https://blackcatinformatics.ca/gmeow/graph/diagnostics"
    )
    rdf_type = pyoxigraph.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
    finding_class = pyoxigraph.NamedNode("https://blackcatinformatics.ca/gmeow/Finding")

    # Everything lands in the diagnostics named graph.
    assert quads, "projection must emit quads"
    assert all(q.graph_name == diagnostics_graph for q in quads)
    # Exactly one gmeow:Finding individual.
    findings = [
        q for q in quads if q.predicate == rdf_type and q.object == finding_class
    ]
    assert len(findings) == 1


def test_write_report_artifacts(tmp_path: Path) -> None:
    report = diagnostics.report_from_messages(
        tool="validate",
        errors=[],
        warnings=["synthetic warning"],
    )

    paths = diagnostics.write_report_artifacts(
        report,
        output_dir=tmp_path,
        stem="feedback",
    )

    assert set(paths) == {"html", "json", "sarif"}
    assert json.loads(paths["json"].read_text(encoding="utf-8"))["tool"] == "validate"
    assert json.loads(paths["sarif"].read_text(encoding="utf-8"))["version"] == "2.1.0"
    assert "synthetic warning" in paths["html"].read_text(encoding="utf-8")
