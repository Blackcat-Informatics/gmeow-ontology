"""Tests for Wikidata fixture auditing."""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.wikidata_audit import audit_file, audit_files, render_audit

SAMPLE_FIXTURE = """
@prefix ex: <http://example.org/> .
@prefix wd: <http://www.wikidata.org/entity/> .
@prefix wdt: <http://www.wikidata.org/prop/direct/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

ex:validItem owl:sameAs wd:Q42 .
ex:badItem owl:sameAs wd:Q0 .
ex:httpsItem owl:sameAs <https://www.wikidata.org/entity/Q42> .
"""


def test_audit_file_valid(tmp_path: Path) -> None:
    path = tmp_path / "valid.ttl"
    path.write_text("""
@prefix ex: <http://example.org/> .
@prefix wd: <http://www.wikidata.org/entity/> .
ex:item ex:ref wd:Q42 .
""")
    findings = audit_file(path)
    assert len(findings) == 0


def test_audit_file_bad_syntax(tmp_path: Path) -> None:
    path = tmp_path / "bad.ttl"
    path.write_text("""
@prefix ex: <http://example.org/> .
@prefix wd: <http://www.wikidata.org/entity/> .
ex:item ex:ref wd:Q0 .
""")
    findings = audit_file(path)
    assert len(findings) == 1
    assert findings[0].severity == "error"
    assert "malformed" in findings[0].message.lower()


def test_audit_file_https_url(tmp_path: Path) -> None:
    path = tmp_path / "https.ttl"
    path.write_text("""
@prefix ex: <http://example.org/> .
ex:item ex:ref <https://www.wikidata.org/entity/Q42> .
""")
    findings = audit_file(path)
    assert len(findings) == 1
    assert findings[0].severity == "warning"
    assert "should be written as wd:Q42" in findings[0].message


def test_audit_file_owl_sameas_not_reported_here(tmp_path: Path) -> None:
    # The universal owl:sameAs ban lives in validate.py (Principle 5, #284).
    # wikidata_audit no longer duplicates it.
    path = tmp_path / "sameas.ttl"
    path.write_text("""
@prefix ex: <http://example.org/> .
@prefix wd: <http://www.wikidata.org/entity/> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
ex:item owl:sameAs wd:Q42 .
""")
    findings = audit_file(path)
    sameas_findings = [
        f for f in findings if f.predicate == "http://www.w3.org/2002/07/owl#sameAs"
    ]
    assert len(sameas_findings) == 0


def test_render_audit_empty() -> None:
    report = audit_files([])
    text = render_audit(report)
    assert "No issues found" in text
