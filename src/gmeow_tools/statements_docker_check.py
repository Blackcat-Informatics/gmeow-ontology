"""Jena/ROBOT-backed statement checks kept out of pytest."""

from __future__ import annotations

from rdflib import RDF, Graph
from rdflib.namespace import OWL

from gmeow_tools.config import STATEMENT_RDF12_FILE
from gmeow_tools.generator import run
from gmeow_tools.statement_compile import assert_lossless, emit_owl
from gmeow_tools.statement_dsl import load_statement_dsl


def assert_committed_artifacts_match_dsl() -> None:
    """The committed statement artifacts must match the canonical DSL."""
    report = run("statements", check=True)
    if report.drifted:
        raise AssertionError(
            "committed statement artifacts are stale; run `gmeow regenerate`:\n  "
            + "\n  ".join(report.drifted)
        )
    if report.orphans:
        raise AssertionError(
            "committed statement artifacts include orphaned generated files:\n  "
            + "\n  ".join(report.orphans)
        )


def assert_committed_rdf12_round_trips_to_owl() -> None:
    """The committed RDF 1.2 lead artifact normalizes back to the OWL form."""
    owl = emit_owl(load_statement_dsl())
    problems = assert_lossless(owl, STATEMENT_RDF12_FILE)
    if problems:
        raise AssertionError(
            "committed RDF 1.2 artifact is not lossless:\n  " + "\n  ".join(problems)
        )


def assert_lossless_gate_detects_a_dropped_annotation() -> None:
    """The lossless gate must report a deliberately removed annotation."""
    owl = emit_owl(load_statement_dsl())
    dropped = next((t for t in owl if str(t[1]).endswith("/confidence")), None)
    if dropped is None:
        raise AssertionError(
            "negative control setup failed: no confidence annotation found"
        )
    owl.remove(dropped)
    problems = assert_lossless(owl, STATEMENT_RDF12_FILE)
    if not problems or not any("confidence" in p for p in problems):
        raise AssertionError("lossless gate did not report the dropped confidence")


def assert_committed_rdf12_uses_triple_term_syntax() -> None:
    """The committed lead artifact must use native RDF 1.2 triple terms."""
    text = STATEMENT_RDF12_FILE.read_text(encoding="utf-8")
    if "rdf:reifies" not in text or "<<(" not in text:
        raise AssertionError("committed RDF 1.2 artifact lacks triple-term syntax")


def assert_reason_consumes_generated_owl_downcast() -> None:
    """Reasoning must consume the generated OWL downcast and stay coherent."""
    from gmeow_tools import reason as reasoning

    merged = reasoning.merge_release()
    merged_graph = Graph().parse(merged, format="turtle")
    if not any(merged_graph.triples((None, RDF.type, OWL.Axiom))):
        raise AssertionError(
            "merged ontology did not include the statement OWL downcast"
        )
    reasoning.validate_profile("DL", merged=merged)
    reasoning.reason("ELK", merged=merged)


def run_all() -> list[str]:
    """Run all Jena/ROBOT-backed statement checks."""
    cases = [
        ("statement artifact drift", assert_committed_artifacts_match_dsl),
        ("RDF 1.2 round-trip", assert_committed_rdf12_round_trips_to_owl),
        (
            "lossless gate negative control",
            assert_lossless_gate_detects_a_dropped_annotation,
        ),
        ("RDF 1.2 triple-term syntax", assert_committed_rdf12_uses_triple_term_syntax),
        ("OWL downcast reasoning", assert_reason_consumes_generated_owl_downcast),
    ]
    completed: list[str] = []
    for name, check in cases:
        check()
        completed.append(name)
    return completed
