"""VoID dataset statistics — the LOD-Cloud 'size' and schema census."""

from __future__ import annotations

from rdflib import Literal, URIRef
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
    counts: dict[str, int] = {}
    for prop in (VOID.triples, VOID.classes, VOID.properties, VOID.entities):
        value = graph.value(dataset, prop)
        assert isinstance(value, Literal), f"VoID dataset missing {prop}"
        count = int(value)
        assert count > 0
        counts[str(prop)] = count

    # The ontology is non-trivial — a cheap guard against a degenerate count.
    assert counts[str(VOID.triples)] > 1000
