"""Retained pytest guards for the trust (Web-of-Trust) slice.

TBox asserted-structure tests have been migrated to the declarative slicetest DSL
in slices/core/trust/tests/structural.ttl (#867).

Retained here (not migratable as module-scoped SPARQL ASK cells):
- test_contested_certification_coexists — ExampleConformance: calls run_shacl()
  over a fixture graph; not a TBox structural assertion.
- test_three_axes_are_orthogonal_in_trust — cross-slice: subjects
  gmeow:accordingTo, gmeow:wasAttributedTo, gmeow:confidence are defined in the
  standpoint module, not trust/module.ttl; a scopeModule cell would miss them.
- test_no_preferred_or_primary_trust_term — dynamic whole-graph sweep over the
  merged graph (_graph()); cannot be expressed as a module-scoped ASK without
  narrowing the live subject set.
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from gmeow_rdf.compat.rdflib import Graph, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Standpoint coexistence — contested certifications + three-axis separation (#51)
# --------------------------------------------------------------------------- #

EX_TRUST = Namespace("https://blackcatinformatics.ca/gmeow/examples/trust/")
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def test_contested_certification_coexists() -> None:
    """A contested key↔identity binding: one standpoint affirms, another refutes.
    Both claims load, SHACL-pass, and are retained — the refutation is first-class."""
    g = Graph().parse(COVERAGE_FIXTURES / "trust-contested.ttl", format="turtle")
    result = run_shacl(g)
    assert result.ok, "\n".join(result.errors)
    # The certification itself exists.
    assert (EX_TRUST.contestedCert, RDF.type, URIRef(GMEOW + "Certification")) in g
    # Both standpoint axioms coexist: affirmation and refutation.
    assert (EX_TRUST.claimCertAffirmed, RDF.type, OWL.Axiom) in g
    assert (EX_TRUST.claimCertRefuted, RDF.type, OWL.Axiom) in g


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
