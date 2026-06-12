# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Compliance-report tests (#285).

``build_report`` is pure (manifest + gate results in, Turtle out), so the
rendering invariants are pinned with fake gate runs; one smoke test parses
the output as RDF.
"""

from __future__ import annotations

from rdflib import Graph

from gmeow_tools.compliance import GateRun, build_report
from gmeow_tools.constitution import load_manifest

_FAKES = {
    "validate": GateRun(errors=0, warnings=3),
    "constitution-check": GateRun(errors=0, warnings=3),
    "lint-alignment": GateRun(errors=0, warnings=0),
    "check-generated": GateRun(errors=0, warnings=0),
}


def _report(fakes: dict[str, GateRun] | None = None) -> str:
    return build_report(
        load_manifest(),
        fakes if fakes is not None else _FAKES,
        generated_at="2026-06-12T00:00:00+00:00",
        source_commit="deadbeef",
    )


def test_report_is_valid_turtle_covering_every_principle() -> None:
    graph = Graph().parse(data=_report(), format="turtle")
    meta = "https://blackcatinformatics.ca/gmeow/meta#"
    from rdflib import RDF, URIRef

    results = list(graph.subjects(RDF.type, URIRef(meta + "PrincipleResult")))
    assert len(results) == 16


def test_runnable_gates_report_passed_and_failures_propagate() -> None:
    passed = _report()
    assert '"passed"' in passed and '"failed"' not in passed
    failing = _report({**_FAKES, "validate": GateRun(errors=2, warnings=0)})
    assert '"failed"' in failing


def test_out_of_process_enforcement_is_gated_in_ci_never_silent() -> None:
    report = _report()
    assert '"gated-in-ci"' in report  # pytest suites, Docker reasoners
    assert '"declared"' in report  # review practice


def test_report_carries_provenance() -> None:
    report = _report()
    assert "deadbeef" in report
    assert "2026-06-12T00:00:00+00:00" in report
