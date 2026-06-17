# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Durable parity guard for the Rust coverage analysis (#579).

The golden under ``tests/fixtures/lint-golden/coverage.json`` was captured from
the *original* pure-Python ``run_coverage`` over the real vendored fixtures
BEFORE the Rust port. This test asserts the Rust path reproduces each of the four
sorted IRI sets EXACTLY — so the covered / gap classification is pinned
independently of the Python body and survives Task 5's deletion of rdflib from
the validation path.

Two routes are checked:

* the direct ``gmeow_validate.coverage_analyze`` extension API over the real
  fixture paths + the SSSOM aligned set, and
* the ``gmeow_tools.coverage.run_coverage`` wrapper that assembles the
  ``CoverageReport``,

so neither the FFI boundary nor the Python adapter can drift from the golden.
"""

from __future__ import annotations

import json
from pathlib import Path

import gmeow_validate

from gmeow_tools.config import NAMESPACE
from gmeow_tools.coverage import covered_iris, fixture_paths, run_coverage

_GOLDEN = Path(__file__).parent / "fixtures" / "lint-golden" / "coverage.json"


def _golden() -> dict[str, list[str]]:
    payload = json.loads(_GOLDEN.read_text(encoding="utf-8"))
    assert isinstance(payload, dict)
    return payload


def test_rust_extension_reproduces_golden() -> None:
    golden = _golden()
    result = gmeow_validate.coverage_analyze(
        [str(p) for p in fixture_paths()],
        sorted(covered_iris()),
        str(NAMESPACE),
    )
    for field in (
        "covered_classes",
        "gap_classes",
        "covered_predicates",
        "gap_predicates",
    ):
        assert sorted(result[field]) == golden[field], f"{field} drifted from golden"


def test_run_coverage_reproduces_golden() -> None:
    golden = _golden()
    report = run_coverage()
    assert sorted(report.covered_classes) == golden["covered_classes"]
    assert sorted(report.gap_classes) == golden["gap_classes"]
    assert sorted(report.covered_predicates) == golden["covered_predicates"]
    assert sorted(report.gap_predicates) == golden["gap_predicates"]
