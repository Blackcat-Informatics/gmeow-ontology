"""Structural + DL-safety guards for the cross-cutting trust (Web-of-Trust) module.

These tests pin the decisions that keep "model the Web of Trust" from quietly
turning the reasoner into a trust calculator: trust is perspectival (reified with
an explicit trustor, never a global relation), endorsement never propagates
(non-transitive, non-symmetric), and only within-reification endpoints are
functional while conflict-prone multi-source datatype values are not.
"""

from __future__ import annotations

from itertools import combinations
from pathlib import Path

from rdflib import Graph, Namespace, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_key_model_uses_scheme_value_not_subclasses() -> None:
    # A single CryptographicKey class; the scheme is a value (an open enumeration
    # of KeyScheme individuals), NOT a per-scheme subclass.
    graph = _graph()
    assert (
        URIRef(GMEOW + "CryptographicKey"),
        RDFS.subClassOf,
        URIRef(GMEOW + "InformationObject"),
    ) in graph
    # keyScheme is functional (a key has exactly one scheme).
    key_scheme = URIRef(GMEOW + "keyScheme")
    assert (key_scheme, RDF.type, OWL.FunctionalProperty) in graph
    # Standard schemes are individuals of gmeow:KeyScheme, not classes.
    for scheme in ("keySchemePGP", "keySchemeX509", "keySchemeSSH", "keySchemeNostr"):
        assert (URIRef(GMEOW + scheme), RDF.type, URIRef(GMEOW + "KeyScheme")) in graph
    # The rejected subclass abstraction must not exist.
    for rejected in ("PGPKey", "X509Certificate", "SSHKey", "NostrKey"):
        assert (URIRef(GMEOW + rejected), RDF.type, OWL.Class) not in graph


def test_certification_and_trust_are_relators() -> None:
    graph = _graph()
    for cls in ("Certification", "TrustAssertion"):
        assert (
            URIRef(GMEOW + cls),
            RDFS.subClassOf,
            URIRef(GUFO + "Relator"),
        ) in graph


def test_endorses_is_not_transitive_or_symmetric() -> None:
    # Trust must NOT propagate inside the reasoner; endorsement is directional.
    graph = _graph()
    endorses = URIRef(GMEOW + "endorses")
    assert (endorses, RDF.type, OWL.ObjectProperty) in graph
    assert (endorses, RDF.type, OWL.TransitiveProperty) not in graph
    assert (endorses, RDF.type, OWL.SymmetricProperty) not in graph


def test_no_global_trusts_property() -> None:
    # Perspectival trust lives only on TrustAssertion via the functional trustor;
    # there is deliberately no global Agent→Agent "trusts" relation.
    graph = _graph()
    assert (URIRef(GMEOW + "trusts"), RDF.type, OWL.ObjectProperty) not in graph


def test_reification_endpoints_are_functional() -> None:
    graph = _graph()
    for prop in (
        "certifier",
        "certifiedKey",
        "certifiedIdentity",
        "trustor",
        "trustee",
        "signingKey",
    ):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} should be functional within its reification"


def test_conflict_prone_key_data_is_not_functional() -> None:
    # Multi-source values must coexist without forcing a merge or inconsistency.
    graph = _graph()
    for prop in ("fingerprint", "keyId", "keyAlgorithm", "keyMaterial"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) not in graph, f"{prop} must stay non-functional (evidence-centric)"


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
