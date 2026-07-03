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

from purrdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GM = Namespace("https://blackcatinformatics.ca/gmeow/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Per-slice part-like relations (Task 8)
# --------------------------------------------------------------------------- #


def test_existing_part_like_relations_specialize_the_spine() -> None:
    g = _graph()
    part_subproperties = {
        GM.containedInLocation,
        GM.containedInPlace,
        GM.rcc8tpp,
        GM.rcc8ntpp,
        GM.subOrganizationOf,
        GM.subEventOf,
        GM.partOfThread,
    }
    has_part_subproperties = {
        GM.rcc8tppi,
        GM.rcc8ntppi,
        GM.hasSubEvent,
        GM.hasNamePart,
        GM.hasBodyPart,
        GM.hasAttachment,
    }

    for prop in part_subproperties:
        assert (prop, RDFS.subPropertyOf, GM.partOf) in g
    for prop in has_part_subproperties:
        assert (prop, RDFS.subPropertyOf, GM.hasPart) in g


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
