"""Cross-slice guards for the spatial aggregation module (#101).

The module-local TBox invariants (class hierarchy, value-vocabulary shape,
property types and cardinalities, solver-layer chain prohibition) have been
migrated to slices/extensions/aggregation/tests/structural.ttl as declarative
slicetest cells (issue #867).

RETAINED here (cross-slice subject — defined in slices/core/places/module.ttl,
not in the aggregation module):
  test_contains_place_exists_and_is_inverse
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_contains_place_exists_and_is_inverse() -> None:
    graph = _graph()
    prop = URIRef(GMEOW + "containsPlace")
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (
        prop,
        OWL.inverseOf,
        URIRef(GMEOW + "containedInPlace"),
    ) in graph
