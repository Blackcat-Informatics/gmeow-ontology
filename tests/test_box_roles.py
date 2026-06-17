"""Tests for graph-box role coverage auditing."""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.box_roles import audit_box_roles, render_text


def test_box_role_audit_passes_for_explicit_typed_role(tmp_path: Path) -> None:
    source = tmp_path / "vocab.ttl"
    source.write_text(
        """@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

ex:tbox a gmeow:GraphBoxRole .
gmeow:Documented
    a owl:Class ;
    gmeow:graphBoxRole ex:tbox .
""",
        encoding="utf-8",
    )

    report = audit_box_roles([source])
    assert report.ok
    assert report.role_counts["https://example.org/tbox"] == 1


def test_box_role_audit_reports_missing_and_invalid_roles(tmp_path: Path) -> None:
    source = tmp_path / "vocab.ttl"
    source.write_text(
        """@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex: <https://example.org/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

gmeow:MissingRole a owl:Class .
gmeow:InvalidRole
    a owl:Class ;
    gmeow:graphBoxRole ex:notTypedAsRole .
""",
        encoding="utf-8",
    )

    report = audit_box_roles([source])
    assert not report.ok
    assert [finding.term for finding in report.missing] == [
        "https://blackcatinformatics.ca/gmeow/MissingRole"
    ]
    assert [finding.term for finding in report.invalid] == [
        "https://blackcatinformatics.ca/gmeow/InvalidRole"
    ]
    text = render_text(report)
    assert "Missing roles (1)" in text
    assert "Invalid roles (1)" in text
