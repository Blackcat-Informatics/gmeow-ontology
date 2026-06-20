# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The hallucination-resistant gates (#55): each engineered fixture case is
surfaced by exactly its gate, and the flat-JSON projection is stable."""

from __future__ import annotations

import json

import pytest

from gmeow_tools.audit import (
    AuditReport,
    audit_graph,
    render_json,
    to_diagnostics_report,
)
from gmeow_tools.config import FIXTURES_DIR

_FIXTURE = FIXTURES_DIR / "hallucination-kg.ttl"
_EX = "https://blackcatinformatics.ca/gmeow/examples/hallucination-kg/"


@pytest.fixture(scope="module")
def report() -> AuditReport:
    return audit_graph([_FIXTURE])


def test_to_diagnostics_report_maps_headlines_and_shacl() -> None:
    audit = AuditReport(
        findings={
            "claims-without-evidence": [("ex:claim-a", "evidence")],
            "claims-contradicted-by-higher-confidence": [("ex:claim-b", "x")],
            "stale-source-claims": [("ex:claim-c", "src")],
        },
        shacl_errors=["focus ex:x violates sh:minCount"],
        shacl_warnings=["focus ex:y soft warning"],
    )

    diag = to_diagnostics_report(audit)

    by_code: dict[str, str] = {item["code"]: item["severity"] for item in diag.findings}
    assert by_code["audit.ungrounded-claim"] == "warning"
    assert by_code["audit.contradicted-claim"] == "warning"
    assert by_code["audit.stale-source"] == "warning"
    assert by_code["audit.shacl-error"] == "error"
    assert by_code["audit.shacl-warning"] == "warning"
    # Only the SHACL error is gate-failing; the heuristic flags never block.
    assert diag.error_count == 1


def test_the_hallucination_is_flagged_not_deleted(report: AuditReport) -> None:
    rows = report.findings["claims-without-evidence"]
    assert [r[0] for r in rows] == [_EX + "claim-hallucinated"]
    # Flagged — and STILL PRESENT in the audited claim set (P10).
    audited = {c["claim"] for c in report.claims}
    assert _EX + "claim-hallucinated" in audited


def test_the_lower_confidence_side_of_the_contradiction_is_reported(
    report: AuditReport,
) -> None:
    rows = report.findings["claims-contradicted-by-higher-confidence"]
    assert len(rows) == 1
    claim, rival, low, high = rows[0]
    assert claim == _EX + "claim-low"
    assert rival == _EX + "claim-high"
    assert float(low) < float(high)


def test_the_stale_source_claim_is_exactly_one(report: AuditReport) -> None:
    rows = report.findings["stale-source-claims"]
    assert [r[0] for r in rows] == [_EX + "claim-stale"]


def test_the_grounded_claim_is_clean(report: AuditReport) -> None:
    flagged = {
        row[0]
        for name in (
            "claims-without-evidence",
            "claims-contradicted-by-higher-confidence",
            "stale-source-claims",
        )
        for row in report.findings[name]
    }
    assert _EX + "claim-grounded" not in flagged


def test_shacl_warns_never_errors_on_the_fixture(report: AuditReport) -> None:
    """The gates FLAG (warnings); nothing about a hallucination is an error."""
    assert not report.shacl_errors
    assert report.shacl_warnings  # hallucination + staleness, bundled


def test_flat_json_shape(report: AuditReport) -> None:
    payload = json.loads(render_json(report))
    by_iri = {c["claim"]: c for c in payload["claims"]}
    assert len(by_iri) == 5  # the four cases + claim-high

    grounded = by_iri[_EX + "claim-grounded"]
    assert grounded["flags"] == {
        "ungrounded": False,
        "contradicted": False,
        "stale": False,
    }
    assert grounded["confidence"] == 0.95
    span = grounded["evidence"][0]
    assert (span["start"], span["end"]) == (60, 141)
    assert span["polarity"] == "polaritySupports"

    hallucinated = by_iri[_EX + "claim-hallucinated"]
    assert hallucinated["flags"]["ungrounded"] is True
    assert hallucinated["evidence"] == []

    low = by_iri[_EX + "claim-low"]
    assert low["flags"]["contradicted"] is True
    assert low["contradicts"] == [_EX + "claim-high"]

    assert by_iri[_EX + "claim-stale"]["flags"]["stale"] is True
