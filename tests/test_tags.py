"""Structural guards for the tags building block — RETAINED dynamic sweeps only.

The asserted-TBox invariants (Tag/TagScheme subClassOf InformationObject,
Tagging subClassOf logic:Relator, seed individuals, property shapes, functional
roles, property-bag ban, broaderTag/narrowerTag/relatedTag semantics, EL
restrictions) have been migrated to declarative slicetest cells in
slices/core/tags/tests/structural.ttl (issue #867).

RETAINED here (not expressible as scopeModule ASK cells):
  test_no_bridge_among_has_tag_is_about_and_rdf_type — iterates
    combinations(axes, 2) where one axis is rdf:type (not an owl:ObjectProperty
    in the ontology); the rdf:type-involving pairs require whole-graph
    closed-world semantics and cannot be narrowed to a single module scope
    without silently losing the guard.  The hasTag↔isAbout pairs are also
    covered in structural.ttl cells saHasTagNotSubPropIsAbout /
    saIsAboutNotSubPropHasTag / saHasTagIsAboutNotEquivalent for redundancy.
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Trichotomy guard: typing ⟂ aboutness ⟂ tagging — rdf:type-involving pairs
# --------------------------------------------------------------------------- #


def test_no_bridge_among_has_tag_is_about_and_rdf_type() -> None:
    """The three axes must remain orthogonal — no subPropertyOf or equivalentProperty
    bridge among them. OWL cannot express disjointness with rdf:type (not an
    ObjectProperty in the ontology), so the closed-world guard lives here."""
    graph = _graph()
    axes = {
        "hasTag": URIRef(GMEOW + "hasTag"),
        "isAbout": URIRef(GMEOW + "isAbout"),
        "rdf:type": RDF.type,
    }
    for a, b in combinations(axes, 2):
        na, nb = axes[a], axes[b]
        assert (na, RDFS.subPropertyOf, nb) not in graph, f"{a} ⊑ {b} forbidden"
        assert (nb, RDFS.subPropertyOf, na) not in graph, f"{b} ⊑ {a} forbidden"
        assert (na, OWL.equivalentProperty, nb) not in graph, f"{a} ≡ {b} forbidden"
        assert (nb, OWL.equivalentProperty, na) not in graph, f"{b} ≡ {a} forbidden"
