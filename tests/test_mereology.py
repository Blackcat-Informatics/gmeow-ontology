"""Universal mereology spine — structural TBox well-formedness (#76).

The OWL 2 RL propagation tests that used to live here — specialized-part relations
entailing generic parthood, ``memberOf`` propagating through ``subOrganizationOf``,
event location propagating through spatial containment — were migrated to the
native Rust reasoning harness (``crates/logic/tests/ontology_entailments.rs``)
under issue #896. What remains are the **structural** checks for per-slice
part-like relations and no-winner/cardinality terms; the universal spine
invariants now live in `slices/core/kernel/tests/structural.ttl`.
"""

from __future__ import annotations

from purrdf.compat.rdflib import OWL, RDF, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GM = Namespace("https://blackcatinformatics.ca/gmeow/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_no_winner_or_cardinality_terms_for_parts() -> None:
    g = _graph()
    forbidden_locals = {
        "primaryPart",
        "preferredPart",
        "primaryWhole",
        "preferredWhole",
    }
    locals_seen = {
        str(s).removeprefix(str(GM)) for s in g.subjects() if str(s).startswith(str(GM))
    }
    assert forbidden_locals.isdisjoint(locals_seen)

    for prop in (GM.partOf, GM.hasPart, GM.subOrganizationOf, GM.subEventOf):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g
