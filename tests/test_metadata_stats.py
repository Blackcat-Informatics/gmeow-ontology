"""VoID dataset statistics — the LOD-Cloud 'size' and schema census."""

from __future__ import annotations

from rdflib import URIRef
from rdflib.namespace import VOID

from gmeow_tools.config import VOID_DATASET_IRI
from gmeow_tools.metadata import build_void_graph


def test_void_dataset_carries_size_and_census() -> None:
    """The dataset node exposes void:triples (LOD-Cloud size) + the census stats.

    Counted from the published default graph, so all four are positive integers;
    void:triples is the metric the LOD Cloud scales the dataset bubble by.
    """
    graph = build_void_graph()
    dataset = URIRef(VOID_DATASET_IRI)
    for prop in (VOID.triples, VOID.classes, VOID.properties, VOID.entities):
        value = graph.value(dataset, prop)
        assert value is not None, f"VoID dataset missing {prop}"
        assert int(value) > 0

    # The ontology is non-trivial and properties outnumber classes (it is a
    # relation-rich vocabulary) — a cheap guard against a zero/degenerate count.
    assert int(graph.value(dataset, VOID.triples)) > 1000
