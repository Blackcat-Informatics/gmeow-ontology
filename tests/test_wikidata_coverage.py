"""Tests for Wikidata coverage reporting."""

from __future__ import annotations

from gmeow_tools.wikidata_coverage import CoverageReport, render_report


def test_coverage_report_empty() -> None:
    report = CoverageReport()
    assert report.class_coverage == 0.0
    assert report.property_coverage == 0.0
    assert report.individual_coverage == 0.0


def test_render_report_text() -> None:
    report = CoverageReport(
        total_classes=10,
        total_properties=5,
        total_individuals=2,
        mapped_classes={"https://example.org/A"},
        mapped_properties=set(),
        mapped_individuals=set(),
        domain_counts={
            "wikidata": {
                "total": 3,
                "exactMatch": 1,
                "closeMatch": 2,
                "relatedMatch": 0,
            }
        },
        predicate_counts={"skos:exactMatch": 1, "skos:closeMatch": 2},
        low_confidence=[("gmeow:X", "wd:Q1", "skos:closeMatch", 0.3)],
        missing_labels=[("gmeow:Y", "wd:Q2")],
    )
    text = render_report(report, json_mode=False)
    assert "Wikidata Mapping Coverage" in text
    assert "classes" in text
    assert "wikidata" in text
    assert "skos:exactMatch" in text
    assert "Low confidence" in text
    assert "Missing objectLabel" in text


def test_render_report_json() -> None:
    report = CoverageReport(
        total_classes=10,
        mapped_classes={"https://example.org/A"},
    )
    text = render_report(report, json_mode=True)
    import json

    data = json.loads(text)
    assert data["totals"]["classes"] == 10
    assert data["mapped"]["classes"] == 1
