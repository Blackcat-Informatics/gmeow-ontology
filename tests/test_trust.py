"""Structural + DL-safety guards for the cross-cutting trust (Web-of-Trust) module.

These tests pin the decisions that keep "model the Web of Trust" from quietly
turning the reasoner into a trust calculator: trust is perspectival (reified with
an explicit trustor, never a global relation), endorsement never propagates
(non-transitive, non-symmetric), and only within-reification endpoints are
functional while conflict-prone multi-source datatype values are not.
"""

from __future__ import annotations

from rdflib import Graph, URIRef
from rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

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
