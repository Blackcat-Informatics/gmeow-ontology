"""Tests for the Dublin Core coverage reporter."""

from __future__ import annotations

from collections.abc import Set as AbstractSet

from gmeow_tools.dc_coverage import run_coverage


def test_dc_coverage_runs_without_error() -> None:
    """The DC coverage report must run without exception."""
    report = run_coverage()
    assert report.total_dcterms > 0
    assert report.total_dcmitype > 0


def test_dc_coverage_has_some_mappings() -> None:
    """After issue #60, at least some DC terms should be mapped."""
    report = run_coverage()
    assert len(report.mapped_dcterms) > 0
    assert len(report.mapped_dcmitype) > 0


def test_dc_coverage_gaps_are_sets() -> None:
    """Gap methods return sets."""
    report = run_coverage()
    assert isinstance(report.gap_dcterms(), AbstractSet)
    assert isinstance(report.gap_dcmitype(), AbstractSet)


def test_dc_coverage_render_text() -> None:
    """Text rendering must produce non-empty output."""
    from gmeow_tools.dc_coverage import render_report

    report = run_coverage()
    text = render_report(report, json_mode=False)
    assert "Dublin Core Mapping Coverage" in text


def test_dc_coverage_render_json() -> None:
    """JSON rendering must produce valid JSON."""
    import json

    from gmeow_tools.dc_coverage import render_report

    report = run_coverage()
    text = render_report(report, json_mode=True)
    data = json.loads(text)
    assert "totals" in data
    assert "mapped" in data
    assert "coverage" in data
    assert "gaps" in data
