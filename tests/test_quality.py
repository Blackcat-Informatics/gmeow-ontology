"""Data-quality layer — whole-ontology Principle-9 sweep (#99).

The quality-module TBox structural assertions (QualityAssessment/QualityDimension
class shape, the assessedEntity/qualityDimension property shape, the seven ISO
19157 + lineage dimension seeds) were migrated to the slice-resident declarative
test-DSL — ``slices/core/quality/tests/structural.ttl``, run by the native Rust
slicetest harness (#867). See ``dsl/tests/MIGRATION-LEDGER.md``.

What remains is the **whole-ontology** dynamic sweep below: it iterates the entire
merged graph's subject set, so it is NOT a quality-module-scoped assertion and a
module-scoped slicetest cell would silently narrow it. It is retained here as a
dynamic-set sweep (the #867 "Keep" category).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_no_preferred_or_primary_term_is_declared() -> None:
    """No GMEOW vocabulary term is a preferred/primary selector (Principle 9).

    A whole-ontology invariant: it sweeps every gmeow:-namespaced subject across
    the merged graph, not just the quality module — so it stays in pytest rather
    than narrowing to a module-scoped cell.
    """
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"
