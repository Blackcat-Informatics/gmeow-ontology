"""Cross-cutting version-set and version-membership guards (#161).

GMEOW models version lineage through a thin spine (versionOf, editionOf,
supersedes, counterpartOf in coreference.ttl) and a reified VersionMembership
relator for standpoint-scoped role claims. This module tests that the reified
layer stays in OWL 2 DL, uses open value vocabularies, and never encodes
mutable roles as essential types.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Classes
# --------------------------------------------------------------------------- #


def test_versionset_is_information_object() -> None:
    graph = _graph()
    versionset = URIRef(GMEOW + "VersionSet")
    assert (versionset, RDF.type, OWL.Class) in graph
    assert (versionset, RDFS.subClassOf, URIRef(GMEOW + "InformationObject")) in graph


def test_versionmembership_is_observation_and_relat() -> None:
    graph = _graph()
    vm = URIRef(GMEOW + "VersionMembership")
    assert (vm, RDF.type, OWL.Class) in graph
    assert (vm, RDFS.subClassOf, URIRef(GMEOW + "Observation")) in graph
    assert (vm, RDFS.subClassOf, URIRef(GUFO + "Relator")) in graph


# --------------------------------------------------------------------------- #
# Functional role properties
# --------------------------------------------------------------------------- #


def test_version_membership_functional_roles() -> None:
    graph = _graph()
    for prop in ("versionMember", "versionSet"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) in graph


# --------------------------------------------------------------------------- #
# Non-functional role/scale properties (standpoint-indexed coexistence)
# --------------------------------------------------------------------------- #


def test_version_role_and_scale_are_not_functional() -> None:
    graph = _graph()
    for prop in ("versionRole", "versionScale"):
        node = URIRef(GMEOW + prop)
        assert (node, RDF.type, OWL.ObjectProperty) in graph
        assert (node, RDF.type, OWL.FunctionalProperty) not in graph


# --------------------------------------------------------------------------- #
# Interval and authority
# --------------------------------------------------------------------------- #


def test_membership_interval_is_object_property() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "membershipInterval")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (
        node,
        RDFS.range,
        URIRef(GMEOW + "TimeInterval"),
    ) in graph


def test_membership_authority_bridges_to_vantage() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "membershipAuthority")
    assert (node, RDF.type, OWL.ObjectProperty) in graph
    assert (node, RDFS.subPropertyOf, URIRef(GMEOW + "vantage")) in graph


# --------------------------------------------------------------------------- #
# Fingerprint
# --------------------------------------------------------------------------- #


def test_version_fingerprint_is_datatype_property_on_entity() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "versionFingerprint")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Entity")) in graph


# --------------------------------------------------------------------------- #
# Value vocabularies are QualityValue subclasses with individual seeds
# --------------------------------------------------------------------------- #


def test_version_role_value_vocabulary() -> None:
    graph = _graph()
    vocab = URIRef(GMEOW + "VersionRole")
    assert (vocab, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in (
        "roleCanonical",
        "roleVariant",
        "roleLatest",
        "roleStable",
        "roleLTS",
        "roleDeprecated",
        "roleYanked",
        "roleDraft",
        "rolePublished",
        "roleRevised",
        "roleCollected",
        "roleWithdrawn",
    ):
        assert (URIRef(GMEOW + seed), RDF.type, vocab) in graph


def test_version_scale_value_vocabulary() -> None:
    graph = _graph()
    vocab = URIRef(GMEOW + "VersionScale")
    assert (vocab, RDFS.subClassOf, URIRef(GUFO + "QualityValue")) in graph
    for seed in ("scaleTrivial", "scaleMinor", "scaleMajor"):
        assert (URIRef(GMEOW + seed), RDF.type, vocab) in graph


# --------------------------------------------------------------------------- #
# No hard-coded role classes (anti-subclass guard)
# --------------------------------------------------------------------------- #


def test_no_version_role_subclasses_exist() -> None:
    graph = _graph()
    for banned in (
        "CanonicalVersion",
        "LatestVersion",
        "StableVersion",
        "YankedVersion",
        "DeprecatedVersion",
        "DraftVersion",
        "VariantVersion",
        "LTSVersion",
    ):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.Class) not in graph


# --------------------------------------------------------------------------- #
# versionLabel domain broadened to Entity
# --------------------------------------------------------------------------- #


def test_version_label_domain_is_entity() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "versionLabel")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Entity")) in graph


# --------------------------------------------------------------------------- #
# EL axioms
# --------------------------------------------------------------------------- #


def test_versionmembership_has_el_restrictions() -> None:
    graph = _graph()
    vm = URIRef(GMEOW + "VersionMembership")
    restrictions = list(graph.objects(vm, RDFS.subClassOf))
    assert any(
        (r, OWL.onProperty, URIRef(GMEOW + "versionMember")) in graph
        and (r, OWL.someValuesFrom, URIRef(GMEOW + "Entity")) in graph
        for r in restrictions
    )
    assert any(
        (r, OWL.onProperty, URIRef(GMEOW + "versionSet")) in graph
        and (r, OWL.someValuesFrom, URIRef(GMEOW + "VersionSet")) in graph
        for r in restrictions
    )
    assert any(
        (r, OWL.onProperty, URIRef(GMEOW + "membershipAuthority")) in graph
        and (r, OWL.someValuesFrom, URIRef(GMEOW + "Agent")) in graph
        for r in restrictions
    )
