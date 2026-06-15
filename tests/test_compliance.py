# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Compliance-report tests (#285).

``build_report`` is pure (manifest + gate results in, Turtle out), so the
rendering invariants are pinned with fake gate runs; one smoke test parses
the output as RDF.
"""

from __future__ import annotations

from rdflib import Graph, Literal, Namespace, URIRef

from gmeow_tools.compliance import META as META_IRI
from gmeow_tools.compliance import (
    RUNNERS,
    GateRun,
    assumed_passed_gate_runs,
    build_report,
)
from gmeow_tools.constitution import load_manifest

META = Namespace(META_IRI)

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


def _graph(report: str) -> Graph:
    return Graph().parse(data=report, format="turtle")


def test_report_is_valid_turtle_covering_every_principle() -> None:
    graph = _graph(_report())
    meta = "https://blackcatinformatics.ca/gmeow/meta#"
    from rdflib import RDF, URIRef

    results = list(graph.subjects(RDF.type, URIRef(meta + "PrincipleResult")))
    assert len(results) == len(load_manifest().principles)


def test_runnable_gates_report_passed_and_failures_propagate() -> None:
    passed_statuses = set(_graph(_report()).objects(None, META.status))
    assert Literal("passed") in passed_statuses
    assert Literal("failed") not in passed_statuses

    failing = _graph(_report({**_FAKES, "validate": GateRun(errors=2, warnings=0)}))
    assert Literal("failed") in set(failing.objects(None, META.status))


def test_out_of_process_enforcement_is_gated_in_ci_never_silent() -> None:
    report = _report()
    assert '"gated-in-ci"' in report  # pytest suites, Docker reasoners
    assert '"declared"' in report  # review practice


def test_report_carries_provenance() -> None:
    report = _report()
    assert "deadbeef" in report
    assert "2026-06-12T00:00:00+00:00" in report


def test_prior_gate_evidence_mode_marks_runnable_gates_passed() -> None:
    report = build_report(
        load_manifest(),
        assumed_passed_gate_runs(),
        generated_at="2026-06-12T00:00:00+00:00",
        source_commit="deadbeef",
        evidence_mode="prior-successful-gates",
    )
    graph = _graph(report)
    assert (META.report, META.evidenceMode, Literal("prior-successful-gates")) in graph
    assert Literal("failed") not in set(graph.objects(None, META.status))

    runnable_enforcements = [
        URIRef(iri)
        for iri, enforcement in load_manifest().enforcements.items()
        if any(
            citation in RUNNERS
            for citation in (*enforcement.make_targets, *enforcement.cli_commands)
        )
    ]
    assert runnable_enforcements
    for enforcement_iri in runnable_enforcements:
        results = list(graph.subjects(META.enforcement, enforcement_iri))
        assert results, f"missing compliance result for {enforcement_iri}"
        for result in results:
            assert (result, META.status, Literal("passed")) in graph
            assert (result, META.errorCount, Literal(0)) in graph
            assert (result, META.warningCount, None) not in graph
