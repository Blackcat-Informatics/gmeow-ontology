# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The mapping-compile diagnostics surface (#809/#854).

Three legs fold into one ``mapping-compile`` report: the DSL/compile path
(``mapping-compile.dsl-error``), the native SSSOM validator
(``mapping-compile.sssom``), and the native ``gmeow_slice.lint_projection`` trio
(``mapping-compile.{fno-type,fno-ref,spec-drift}``). The SSSOM + projection-lint
checks are now subsumed natively into ``gmeow-rdf``/``gmeow-slice`` (#848/#854), so
they surface here as canonical findings rather than being deferred.
"""

from __future__ import annotations

import gmeow_rdf
import gmeow_slice
import pytest

import gmeow_tools.mapping_compile as mapping_compile
from gmeow_tools.mapping_dsl import CompileError


def _codes(report: object) -> list[str]:
    return [f["code"] for f in report.findings]  # type: ignore[attr-defined]


def test_clean_committed_mappings_compile_to_an_ok_report() -> None:
    """The committed mapping DSL + projection stack compile with no findings."""
    report = mapping_compile.compile_diagnostics_report()

    assert report.ok
    assert report.error_count == 0
    # None of the three subsumed leg codes fire on the clean committed tree.
    assert not any(
        c.startswith("mapping-compile.")
        and c.split(".", 1)[1] in {"sssom", "fno-type", "fno-ref", "spec-drift"}
        for c in _codes(report)
    )


def test_dsl_compile_error_becomes_one_finding(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A CompileError on the DSL/artifact path maps to one dsl-error finding."""

    def _boom(*_args: object, **_kwargs: object) -> object:
        raise CompileError("value-class pattern has no value-binding predicate")

    monkeypatch.setattr(mapping_compile, "_artifacts", _boom)

    report = mapping_compile.compile_diagnostics_report()

    assert not report.ok
    assert "mapping-compile.dsl-error" in _codes(report)
    item = next(f for f in report.findings if f["code"] == "mapping-compile.dsl-error")
    assert "value-binding predicate" in item["message"]


def test_sssom_validation_problem_becomes_a_finding(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A blocking SSSOM diagnostic maps to a mapping-compile.sssom finding (#854)."""
    monkeypatch.setattr(
        gmeow_slice,
        "emit_sssom",
        lambda _root: {"gmeow-bad.sssom.tsv": "irrelevant"},
    )
    monkeypatch.setattr(
        gmeow_rdf,
        "validate_sssom",
        lambda _text: [
            {
                "severity": "ERROR",
                "code": "RequiredSlot",
                "message": "missing subject_id",
                "check": "RequiredSlot",
                "instance": "row-3",
            },
            # A WARNING is non-blocking — it must NOT surface as a finding.
            {
                "severity": "WARNING",
                "code": "X",
                "message": "noise",
                "check": "X",
                "instance": None,
            },
        ],
    )

    report = mapping_compile.compile_diagnostics_report()

    sssom = [f for f in report.findings if f["code"] == "mapping-compile.sssom"]
    assert len(sssom) == 1
    assert sssom[0]["severity"] == "error"
    assert "missing subject_id" in sssom[0]["message"]


def test_projection_lint_problems_become_findings(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The native lint's check maps to mapping-compile.<check> findings (#854)."""
    monkeypatch.setattr(
        gmeow_slice,
        "lint_projection",
        lambda _root: [
            {
                "severity": "ERROR",
                "code": "fno-type",
                "message": "p: predicate range disagreement",
                "check": "fno-type",
                "instance": "gmeow:pX",
            },
            {
                "severity": "ERROR",
                "code": "spec-drift",
                "message": "schema-org: term is a dead cell",
                "check": "spec-drift",
                "instance": "https://schema.org/dead",
            },
        ],
    )

    report = mapping_compile.compile_diagnostics_report()

    codes = _codes(report)
    assert "mapping-compile.fno-type" in codes
    assert "mapping-compile.spec-drift" in codes


def test_projection_lint_loader_failure_degrades_to_warning(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """A native-lint loader failure becomes one warning, not a crash (#854)."""

    def _boom(_root: str) -> object:
        raise RuntimeError("native ext unavailable")

    monkeypatch.setattr(gmeow_slice, "lint_projection", _boom)

    report = mapping_compile.compile_diagnostics_report()

    assert "mapping-compile.projection-lint-skipped" in _codes(report)
