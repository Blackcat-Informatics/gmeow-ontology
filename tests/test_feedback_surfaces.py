# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The `gmeow-dev feedback` surface fold loop (#654).

Guards that the fold table (``_surface_reports()``) stays in sync with the
expected-surface set (``_EXPECTED_SURFACES``), that a single surface failing
is isolated (the bundle still self-attests), and — the drift regression this
slice exists to prevent — that a row added to or removed from the fold table
without a matching update to the expected set fails the test.  The guard does
NOT (and cannot, given the non-1:1 surface↔target mapping) derive the surface
set from the Makefile.
"""

from __future__ import annotations

from typing import Any

import pytest

from gmeow_tools import cli_dev, diagnostics
from gmeow_tools.feedback_bundle import build_feedback_bundle, verify_feedback_bundle

#: The migrated surfaces this slice folds (alignment, coverage, acceptance,
#: wikidata, constitution, crate-layering, box-roles, audit, generator drift,
#: classic/engine cross-check, the logic/statement/mapping compilers, and the
#: native slice-ownership report — #809).
#: validate + native reason/verify are folded separately in `feedback` itself.
_EXPECTED_SURFACES = {
    "alignment",
    "coverage",
    "acceptance",
    "wikidata",
    "constitution",
    "crate-layering",
    "box-roles",
    "audit",
    "generated",
    "classic-cross-check",
    "engine-cross-check",
    "logic-compile",
    "statement-compile",
    "mapping-compile",
    "slice-ownership",
}


def test_surface_reports_covers_every_migrated_surface() -> None:
    """The fold table must list exactly the migrated surfaces — no drift.

    The test fails if ``_surface_reports()`` and ``_EXPECTED_SURFACES`` fall
    out of sync: a surface added to or removed from the fold table without a
    matching update to the expected set is caught here, pinning the two sibling
    declarations together.  It does not promise that every ``make check``
    target is present — the surface↔target mapping is not 1:1.
    """
    labels = {label for label, _ in cli_dev._surface_reports()}
    assert labels == _EXPECTED_SURFACES


def _surface_report(label: str) -> Any:
    """A synthetic one-finding report uniquely coded for a surface."""
    report = diagnostics.report(label)
    report.add(
        diagnostics.finding(
            severity="warning",
            code=f"{label}.synthetic",
            message=f"synthetic {label} finding",
            tool=label,
        )
    )
    return report


def test_feedback_folds_all_surface_findings(monkeypatch: pytest.MonkeyPatch) -> None:
    """`_fold_surfaces` merges every surface's findings into the report."""
    table = [
        (label, (lambda lbl=label: _surface_report(lbl)))
        for label in sorted(_EXPECTED_SURFACES)
    ]
    monkeypatch.setattr(cli_dev, "_surface_reports", lambda: table)

    report = diagnostics.report("validate")
    cli_dev._fold_surfaces(report)

    codes = {item["code"] for item in report.findings}
    for label in _EXPECTED_SURFACES:
        assert f"{label}.synthetic" in codes


def test_feedback_surface_failure_is_isolated(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """One surface raising leaves the others intact, surfaces the error loudly,
    and the bundle still self-attests."""

    def _boom() -> Any:
        raise RuntimeError("synthetic acceptance explosion")

    table: list[tuple[str, Any]] = [
        ("acceptance", _boom),
        ("coverage", lambda: _surface_report("coverage")),
    ]
    monkeypatch.setattr(cli_dev, "_surface_reports", lambda: table)

    report = diagnostics.report("validate")
    cli_dev._fold_surfaces(report)

    by_code = {item["code"]: item for item in report.findings}
    # The healthy surface still landed.
    assert "coverage.synthetic" in by_code
    # The failed surface is a visible warning carrying the real exception text.
    skipped = by_code["feedback.acceptance-skipped"]
    assert skipped["severity"] == "warning"
    assert "synthetic acceptance explosion" in skipped["message"]
    # The bundle built from the partial report still self-attests.
    assert verify_feedback_bundle(build_feedback_bundle(report))
