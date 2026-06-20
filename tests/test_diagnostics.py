"""Tests for the shared diagnostics reporting facade."""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from html.parser import HTMLParser
from io import StringIO
from pathlib import Path
from typing import Any, cast

import gmeow_diagnostics
from rich.console import Console

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


class _TableParser(HTMLParser):
    """Counts <tr>/<table> tags so HTML validity is checked structurally."""

    def __init__(self) -> None:
        super().__init__()
        self.tr_open = 0
        self.tr_close = 0
        self.table_open = 0
        self.table_close = 0

    def handle_starttag(self, tag: str, attrs: object) -> None:
        if tag == "tr":
            self.tr_open += 1
        elif tag == "table":
            self.table_open += 1

    def handle_endtag(self, tag: str) -> None:
        if tag == "tr":
            self.tr_close += 1
        elif tag == "table":
            self.table_close += 1


def test_html_is_well_formed_with_one_row_per_finding() -> None:
    # Criterion #11 asks for HTML *validity*, not just substring presence: parse
    # the rendered HTML and assert balanced table markup with exactly one data
    # row per finding (plus the header row).
    report = gmeow_diagnostics.Report("validate")
    for i in range(3):
        report.add(gmeow_diagnostics.Finding("error", f"code.{i}", f"message {i}"))

    parser = _TableParser()
    parser.feed(report.to_html())

    # Balanced <table> and <tr> tags (well-formed markup).
    assert parser.table_open == parser.table_close == 1
    assert parser.tr_open == parser.tr_close
    # One header row + one row per finding.
    assert parser.tr_open == 1 + report.finding_count


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


def test_gmeow_rdf_projection_parses_in_gmeow_rdf() -> None:
    """The gmeow: RDF projection is valid N-Quads with one Finding per finding."""
    import gmeow_rdf

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
        gmeow_rdf.parse(nquads.encode("utf-8"), format=gmeow_rdf.RdfFormat.N_QUADS)
    )

    diagnostics_graph = gmeow_rdf.NamedNode(
        "https://blackcatinformatics.ca/gmeow/graph/diagnostics"
    )
    rdf_type = gmeow_rdf.NamedNode("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")
    finding_class = gmeow_rdf.NamedNode("https://blackcatinformatics.ca/gmeow/Finding")

    # Everything lands in the diagnostics named graph.
    assert quads, "projection must emit quads"
    assert all(q.graph_name == diagnostics_graph for q in quads)
    # Exactly one gmeow:Finding individual.
    findings = [
        q for q in quads if q.predicate == rdf_type and q.object == finding_class
    ]
    assert len(findings) == 1


def test_report_from_findings_folds_pre_built_findings() -> None:
    findings = [
        diagnostics.finding(
            severity="error",
            code="surface.bad",
            message="a real problem",
            tool="surface",
        ),
        diagnostics.finding(
            severity="warning",
            code="surface.iffy",
            message="a softer problem",
            tool="surface",
        ),
    ]

    report = diagnostics.report_from_findings(tool="surface", findings=findings)

    assert report.tool == "surface"
    assert report.error_count == 1
    assert report.warning_count == 1
    codes = {item["code"] for item in report.findings}
    assert codes == {"surface.bad", "surface.iffy"}


def test_report_from_findings_empty_is_ok() -> None:
    report = diagnostics.report_from_findings(tool="surface", findings=[])

    assert report.ok
    assert report.finding_count == 0


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


def _two_finding_report() -> Any:
    report = diagnostics.report("validate")
    report.add(diagnostics.finding(severity="error", code="b.err", message="boom"))
    report.add(
        diagnostics.finding(severity="warning", code="a.warn", message="careful")
    )
    return report


def test_write_report_artifacts_selection_writes_only_requested(tmp_path: Path) -> None:
    report = _two_finding_report()

    paths = diagnostics.write_report_artifacts(
        report, output_dir=tmp_path, stem="feedback", artifacts={"json"}
    )

    assert set(paths) == {"json"}
    assert sorted(p.name for p in tmp_path.iterdir()) == ["feedback.json"]


def test_write_report_artifacts_none_selection_writes_nothing(tmp_path: Path) -> None:
    report = _two_finding_report()

    paths = diagnostics.write_report_artifacts(
        report, output_dir=tmp_path, stem="feedback", artifacts=frozenset()
    )

    assert paths == {}
    assert list(tmp_path.iterdir()) == []


def test_write_report_artifacts_is_byte_identical_on_repeat(tmp_path: Path) -> None:
    report = _two_finding_report()

    first = diagnostics.write_report_artifacts(
        report, output_dir=tmp_path, stem="feedback"
    )
    digest_one = {k: v.read_bytes() for k, v in first.items()}
    second = diagnostics.write_report_artifacts(
        report, output_dir=tmp_path, stem="feedback"
    )
    digest_two = {k: v.read_bytes() for k, v in second.items()}

    assert digest_one == digest_two


def test_emit_console_silent_prints_nothing() -> None:
    from gmeow_tools.diagnostics_config import DiagnosticsConfig

    config = DiagnosticsConfig.resolve(console="silent", is_tty=False)
    console = Console(file=StringIO(), force_terminal=False)
    diagnostics.emit_console(_two_finding_report(), config, console)

    assert cast(StringIO, console.file).getvalue() == ""


def test_emit_console_text_matches_render_text() -> None:
    from gmeow_tools.diagnostics_config import DiagnosticsConfig

    report = _two_finding_report()
    config = DiagnosticsConfig.resolve(console="text", is_tty=False)
    console = Console(file=StringIO(), force_terminal=False, width=200)
    diagnostics.emit_console(report, config, console)

    rendered = cast(StringIO, console.file).getvalue()
    assert (
        report.render_text().strip() in rendered.strip() or report.render_text() == ""
    )


def test_emit_console_jsonl_is_one_valid_json_object_per_finding() -> None:
    from gmeow_tools.diagnostics_config import DiagnosticsConfig

    report = _two_finding_report()
    config = DiagnosticsConfig.resolve(console="jsonl", is_tty=False)
    console = Console(file=StringIO(), force_terminal=False, width=10_000)
    diagnostics.emit_console(report, config, console)

    lines = [
        ln for ln in cast(StringIO, console.file).getvalue().splitlines() if ln.strip()
    ]
    parsed = [json.loads(ln) for ln in lines]
    # One object per finding, ordered exactly as the canonical to_json projection.
    canonical = json.loads(report.to_json())["findings"]
    assert len(parsed) == len(canonical)
    assert [item["code"] for item in parsed] == [item["code"] for item in canonical]
