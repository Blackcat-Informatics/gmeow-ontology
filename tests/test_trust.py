"""Retained pytest guards for the trust (Web-of-Trust) slice.

TBox asserted-structure tests have been migrated to the declarative slicetest DSL
in slices/core/trust/tests/structural.ttl (#867).

SHACL conformance test migrated to Rust (#867):
- test_contested_certification_coexists →
  conformance_trust.rs::contested_certification_coexists

Retained here (not migratable):
- test_three_axes_are_orthogonal_in_trust — cross-slice: subjects
  gmeow:accordingTo, gmeow:wasAttributedTo, gmeow:confidence are defined in the
  standpoint module, not trust/module.ttl; a scopeModule cell would miss them.
  Uses _graph() / load_merged_graph.
- test_no_preferred_or_primary_trust_term — dynamic whole-graph sweep over the
  merged graph (_graph()); cannot be expressed as a module-scoped ASK without
  narrowing the live subject set.
"""

from __future__ import annotations

from itertools import combinations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_three_axes_are_orthogonal_in_trust() -> None:
    """accordingTo ⟂ wasAttributedTo ⟂ confidence: no inferential bridge in the
    trust module (mirrors test_three_axes_are_orthogonal in test_standpoint.py)."""
    g = _graph()
    axes = [
        URIRef(GMEOW + "accordingTo"),
        URIRef(GMEOW + "wasAttributedTo"),
        URIRef(GMEOW + "confidence"),
    ]
    for a, b in combinations(axes, 2):
        assert (a, RDFS.subPropertyOf, b) not in g
        assert (b, RDFS.subPropertyOf, a) not in g
        assert (a, OWL.equivalentProperty, b) not in g
        assert (b, OWL.equivalentProperty, a) not in g


def test_no_preferred_or_primary_trust_term() -> None:
    """Principle 9: no single slot to win — trust mints no preferred/primary
    selector for a contested certification or trust level."""
    g = _graph()
    prop_types = (OWL.ObjectProperty, OWL.DatatypeProperty, OWL.AnnotationProperty)
    for banned in (
        "primaryCertification",
        "preferredCertification",
        "primaryTrust",
        "preferredTrust",
        "preferredRank",
    ):
        node = URIRef(GMEOW + banned)
        for pt in prop_types:
            assert (node, RDF.type, pt) not in g, f"{banned} must not exist"
        assert (node, RDF.type, OWL.Class) not in g
