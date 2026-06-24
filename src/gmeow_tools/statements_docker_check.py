"""Jena/ROBOT-backed statement checks kept out of pytest.

The ``classic-cross-check`` Jena oracle for the **native** RDF 1.2 statement lead
writer (#667). The committed ``gmeow.rdf12.ttl`` is produced natively by the
gmeow-rdf Rust codec on the primary path; this lane re-reads it with **Apache
Jena** and proves the two engines agree by RDF 1.2 graph isomorphism. The lossless
check therefore binds to Jena directly (:func:`assert_lossless_jena`), NOT to the
native ``gmeow-rdf`` normalizer — otherwise the "oracle" would silently re-run
the native engine and prove nothing.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph
from gmeow_rdf.compat.rdflib.compare import graph_diff, isomorphic
from gmeow_rdf.compat.rdflib.namespace import OWL

from gmeow_tools.config import PROJECT_ROOT, STATEMENT_RDF12_FILE
from gmeow_tools.rdf12 import normalize_rdf12_to_owl


def _native_statement_owl_graph() -> Graph:
    """Compile the statement OWL downcast through the native Rust stage."""
    import gmeow_native.pipeline as _pipeline

    artifacts = _pipeline.compile_statements(str(PROJECT_ROOT))
    graph = Graph()
    graph.parse(data=artifacts["owl_ttl"], format="turtle")
    return graph


def assert_lossless_jena(owl_graph: Graph, rdf12_path: Path) -> list[str]:
    """Prove the RDF 1.2 form round-trips to the OWL form **via Jena** (empty == ok).

    The independent Java/Docker oracle: it normalizes the committed
    (native-gmeow-rdf-written) RDF 1.2 artifact back to OWL normal form with Apache
    Jena and compares it to the authored OWL graph by isomorphism. Binding to Jena
    here (not the native codec) is what makes this lane a genuine cross-engine
    check of the native lead writer (#667).
    """
    normalized = normalize_rdf12_to_owl(rdf12_path)
    if isomorphic(owl_graph, normalized):
        return []
    _, only_owl, only_rdf12 = graph_diff(owl_graph, normalized)
    problems: list[str] = []
    for triple in sorted(only_owl, key=str):
        problems.append(f"OWL form has, RDF 1.2 lost: {triple}")
    for triple in sorted(only_rdf12, key=str):
        problems.append(f"RDF 1.2 form has, OWL lacks: {triple}")
    return problems


def assert_committed_artifacts_match_dsl() -> None:
    """The committed statement artifacts must match the canonical DSL.

    Drift-checks via the Rust ``gmeow-pipeline`` executor (the build authority
    since #861 P7) and filters its drift findings to the statement artifacts.
    """
    import os

    import gmeow_native.pipeline as _pipeline

    from gmeow_tools.config import PROJECT_ROOT

    report = _pipeline.run_pipeline(str(PROJECT_ROOT), os.cpu_count() or 1, True)
    statement_drift = [
        d for d in report.get("drifted", []) if "statement" in d or "gmeow.rdf12" in d
    ]
    if statement_drift:
        raise AssertionError(
            "committed statement artifacts are stale; run `gmeow regenerate`:\n  "
            + "\n  ".join(statement_drift)
        )


def assert_committed_rdf12_round_trips_to_owl() -> None:
    """The committed RDF 1.2 lead artifact normalizes back to the OWL form (Jena)."""
    owl = _native_statement_owl_graph()
    problems = assert_lossless_jena(owl, STATEMENT_RDF12_FILE)
    if problems:
        raise AssertionError(
            "committed RDF 1.2 artifact is not lossless:\n  " + "\n  ".join(problems)
        )


def assert_lossless_gate_detects_a_dropped_annotation() -> None:
    """The Jena lossless gate must report a deliberately removed annotation."""
    owl = _native_statement_owl_graph()
    dropped = next((t for t in owl if str(t[1]).endswith("/confidence")), None)
    if dropped is None:
        raise AssertionError(
            "negative control setup failed: no confidence annotation found"
        )
    owl.remove(dropped)
    problems = assert_lossless_jena(owl, STATEMENT_RDF12_FILE)
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
