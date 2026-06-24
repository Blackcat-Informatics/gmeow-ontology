"""Pitch collection and spelling structural guards (issue #309).

Principles 4, 5, 6, 9, 11, 15, 16.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test-music-collections/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_collection_properties_are_functional() -> None:
    graph = _graph()
    for prop in ("collectionKind", "collectionPartOrder"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} must be functional"


def test_membership_constituents_are_functional() -> None:
    graph = _graph()
    for prop in (
        "membershipCollection",
        "membershipPitch",
        "membershipRole",
        "membershipDegreeIndex",
    ):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} must be functional"


def test_membership_context_is_not_functional() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "membershipContext"),
        RDF.type,
        OWL.FunctionalProperty,
    ) not in graph


def test_spelling_constituents_are_functional() -> None:
    graph = _graph()
    for prop in ("spellingPitch", "spellingSystem", "spelledName"):
        assert (
            URIRef(GMEOW + prop),
            RDF.type,
            OWL.FunctionalProperty,
        ) in graph, f"{prop} must be functional"


def test_enharmonic_spellings_coexist() -> None:
    """C♯4 and D♭4 are two co-equal spellings of the same pitch value (Principle 9)."""
    graph = _graph()
    pitch = URIRef(GMEOW + "pitchValue12EDOCSharp4")
    csharp = URIRef(GMEOW + "pitchSpellingCSharp4CMN")
    dflat = URIRef(GMEOW + "pitchSpellingDFlat4CMN")

    for spelling in (csharp, dflat):
        assert (spelling, RDF.type, URIRef(GMEOW + "PitchSpelling")) in graph
        assert (
            spelling,
            URIRef(GMEOW + "spellingPitch"),
            pitch,
        ) in graph

    assert (
        csharp,
        URIRef(GMEOW + "spelledName"),
        Literal("C♯4", datatype=XSD.string),
    ) in graph
    assert (
        dflat,
        URIRef(GMEOW + "spelledName"),
        Literal("D♭4", datatype=XSD.string),
    ) in graph


def test_rast_maqam_seeds_exist() -> None:
    graph = _graph()
    for iri in (
        "pitchCollectionRastMaqam",
        "pitchCollectionRastJinsC",
        "pitchCollectionWustaJinsG",
        "pitchCollectionRastJinsHighC",
    ):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "PitchCollection"),
        ) in graph, f"missing {iri}"

    maqam = URIRef(GMEOW + "pitchCollectionRastMaqam")
    jins_c = URIRef(GMEOW + "pitchCollectionRastJinsC")
    assert (
        maqam,
        URIRef(GMEOW + "hasPart"),
        jins_c,
    ) in graph
    assert (
        jins_c,
        URIRef(GMEOW + "collectionPartOrder"),
        Literal(0, datatype=XSD.nonNegativeInteger),
    ) in graph


def test_yaman_raga_seeds_exist() -> None:
    graph = _graph()
    raga = URIRef(GMEOW + "pitchCollectionYamanRaga")
    assert (raga, RDF.type, URIRef(GMEOW + "PitchCollection")) in graph
    assert (
        raga,
        URIRef(GMEOW + "collectionKind"),
        URIRef(GMEOW + "pitchCollectionKindRaga"),
    ) in graph

    # Vādī = G, samvādī = D.
    assert (
        URIRef(GMEOW + "membershipYamanG"),
        URIRef(GMEOW + "membershipRole"),
        URIRef(GMEOW + "collectionMemberRoleVadi"),
    ) in graph
    assert (
        URIRef(GMEOW + "membershipYamanD"),
        URIRef(GMEOW + "membershipRole"),
        URIRef(GMEOW + "collectionMemberRoleSamvadi"),
    ) in graph


def test_messiaen_mode_seed_exists() -> None:
    graph = _graph()
    mode = URIRef(GMEOW + "pitchCollectionMessiaenMode1")
    assert (mode, RDF.type, URIRef(GMEOW + "PitchCollection")) in graph
    assert (
        mode,
        URIRef(GMEOW + "collectionKind"),
        URIRef(GMEOW + "pitchCollectionKindModeOfLimitedTransposition"),
    ) in graph


def test_pcset_seed_exists() -> None:
    graph = _graph()
    pcset = URIRef(GMEOW + "pitchCollectionPCSet027")
    assert (pcset, RDF.type, URIRef(GMEOW + "PitchCollection")) in graph
    assert (
        pcset,
        URIRef(GMEOW + "collectionKind"),
        URIRef(GMEOW + "pitchCollectionKindPCSet"),
    ) in graph
    for iri in ("membershipPCSet0", "membershipPCSet2", "membershipPCSet7"):
        assert (
            URIRef(GMEOW + iri),
            RDF.type,
            URIRef(GMEOW + "PitchCollectionMembership"),
        ) in graph


def test_standpoint_memberships_coexist() -> None:
    """Two contradictory membership claims coexist with distinct accordingTo."""
    graph = _graph()
    arabic = URIRef(GMEOW + "membershipRastThirdArabic")
    turkish = URIRef(GMEOW + "membershipRastThirdTurkish")
    for membership in (arabic, turkish):
        assert (
            membership,
            RDF.type,
            URIRef(GMEOW + "PitchCollectionMembership"),
        ) in graph

    assert (
        arabic,
        URIRef(GMEOW + "accordingTo"),
        URIRef(GMEOW + "standpointArabicTheory"),
    ) in graph
    assert (
        turkish,
        URIRef(GMEOW + "accordingTo"),
        URIRef(GMEOW + "standpointTurkishTheory"),
    ) in graph

    # The two memberships assert different pitches for degree 2.
    assert (
        arabic,
        URIRef(GMEOW + "membershipPitch"),
        URIRef(GMEOW + "pitchValue24EDOEHalfFlat4"),
    ) in graph
    assert (
        turkish,
        URIRef(GMEOW + "membershipPitch"),
        URIRef(GMEOW + "pitchValue24EDOE4"),
    ) in graph
