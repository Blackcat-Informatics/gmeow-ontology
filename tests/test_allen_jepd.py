"""Tests for Allen interval relations and JEPD disjointness (issue #67).

The module-local structural invariants have been migrated to the declarative
slicetest DSL in slices/core/temporal/tests/structural.ttl (cells 10-13).

RETAINED here: test_no_owl_all_disjoint_properties_over_interval_relations —
a whole-graph sweep over every owl:AllDisjointProperties to ensure no
interval-level Allen relation is grouped into an OWL disjoint-properties axiom.
Not expressible as a finite module-scoped SPARQL ASK.
"""

from __future__ import annotations

from purrdf.compat.rdflib import Graph
from purrdf.compat.rdflib.namespace import OWL, RDF

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"

_INTERVAL_ALLEN = [
    "intervalBefore",
    "intervalAfter",
    "intervalMeets",
    "intervalMetBy",
    "intervalOverlaps",
    "intervalOverlappedBy",
    "intervalStarts",
    "intervalStartedBy",
    "intervalDuring",
    "intervalContains",
    "intervalFinishes",
    "intervalFinishedBy",
    "intervalCoincidesWith",
]


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_no_owl_all_disjoint_properties_over_interval_relations() -> None:
    """OWL 2 DL forbids DisjointObjectProperties over non-simple (transitive)
    properties.  JEPD enforcement lives in SHACL / solver instead."""
    g = _graph()
    for subject in g.subjects(RDF.type, OWL.AllDisjointProperties):
        for list_head in g.objects(subject, OWL.members):
            members = {str(m) for m in g.items(list_head)}
            interval_members = {GMEOW + rel for rel in _INTERVAL_ALLEN}
            overlap = members & interval_members
            assert not overlap, (
                "owl:AllDisjointProperties must not cover interval relations: "
                f"{overlap}"
            )
