"""Pitch collection and spelling structural guards (issue #309).

Principles 4, 5, 6, 9, 11, 15, 16.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import ValidationResult
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test-music-collections/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _error_text(result: ValidationResult) -> str:
    return "\n".join(result.errors)


def test_pitch_collection_kind_is_quality_value() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "PitchCollectionKind"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph


def test_collection_member_role_is_quality_value() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "CollectionMemberRole"),
        RDFS.subClassOf,
        URIRef(GUFO + "QualityValue"),
    ) in graph


def test_pitch_spelling_system_is_information_object_kind() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "PitchSpellingSystem"),
        RDF.type,
        OWL.Class,
    ) in graph
    assert (
        URIRef(GMEOW + "PitchSpellingSystem"),
        RDF.type,
        URIRef(GUFO + "Kind"),
    ) in graph


def test_pitch_collection_membership_is_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "PitchCollectionMembership"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


def test_pitch_spelling_is_relator() -> None:
    graph = _graph()
    assert (
        URIRef(GMEOW + "PitchSpelling"),
        RDFS.subClassOf,
        URIRef(GUFO + "Relator"),
    ) in graph


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


def test_pitch_collection_shape_requires_kind() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    col = EX.badCollection
    g.add((col, RDF.type, GMEOW.PitchCollection))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchCollection must have exactly one collectionKind (Principle 9)."
        in _error_text(result)
    )


def test_pitch_collection_membership_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    membership = EX.membershipValid
    g.add((membership, RDF.type, GMEOW.PitchCollectionMembership))
    g.add((membership, GMEOW.membershipCollection, GMEOW.pitchCollectionPCSet027))
    g.add((membership, GMEOW.membershipPitch, GMEOW.pitchValue12EDOOrigin))
    g.add((membership, GMEOW.membershipRole, GMEOW.collectionMemberRoleMember))
    g.add((membership, GMEOW.membershipDegreeIndex, Literal(0, datatype=XSD.integer)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_pitch_collection_membership_missing_collection_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    membership = EX.membershipNoCollection
    g.add((membership, RDF.type, GMEOW.PitchCollectionMembership))
    g.add((membership, GMEOW.membershipPitch, GMEOW.pitchValue12EDOOrigin))
    g.add((membership, GMEOW.membershipRole, GMEOW.collectionMemberRoleMember))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchCollectionMembership must belong to exactly one PitchCollection."
        in _error_text(result)
    )


def test_pitch_collection_membership_missing_pitch_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    membership = EX.membershipNoPitch
    g.add((membership, RDF.type, GMEOW.PitchCollectionMembership))
    g.add((membership, GMEOW.membershipCollection, GMEOW.pitchCollectionPCSet027))
    g.add((membership, GMEOW.membershipRole, GMEOW.collectionMemberRoleMember))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchCollectionMembership must name exactly one PitchValue."
        in _error_text(result)
    )


def test_pitch_collection_membership_missing_role_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    membership = EX.membershipNoRole
    g.add((membership, RDF.type, GMEOW.PitchCollectionMembership))
    g.add((membership, GMEOW.membershipCollection, GMEOW.pitchCollectionPCSet027))
    g.add((membership, GMEOW.membershipPitch, GMEOW.pitchValue12EDOOrigin))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchCollectionMembership must declare exactly one CollectionMemberRole."
        in _error_text(result)
    )


def test_pitch_spelling_valid_passes_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    spelling = EX.spellingValid
    g.add((spelling, RDF.type, GMEOW.PitchSpelling))
    g.add((spelling, GMEOW.spellingPitch, GMEOW.pitchValue12EDOCSharp4))
    g.add((spelling, GMEOW.spellingSystem, GMEOW.pitchSpellingSystemCMN))
    g.add((spelling, GMEOW.spelledName, Literal("C♯4", datatype=XSD.string)))
    result = run_shacl(g)
    assert result.ok, _error_text(result)


def test_pitch_spelling_missing_pitch_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    spelling = EX.spellingNoPitch
    g.add((spelling, RDF.type, GMEOW.PitchSpelling))
    g.add((spelling, GMEOW.spellingSystem, GMEOW.pitchSpellingSystemCMN))
    g.add((spelling, GMEOW.spelledName, Literal("C♯4", datatype=XSD.string)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "A PitchSpelling must name exactly one PitchValue." in _error_text(result)


def test_pitch_spelling_missing_system_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    spelling = EX.spellingNoSystem
    g.add((spelling, RDF.type, GMEOW.PitchSpelling))
    g.add((spelling, GMEOW.spellingPitch, GMEOW.pitchValue12EDOCSharp4))
    g.add((spelling, GMEOW.spelledName, Literal("C♯4", datatype=XSD.string)))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert "A PitchSpelling must use exactly one PitchSpellingSystem." in _error_text(
        result
    )


def test_pitch_spelling_missing_name_fails_shacl() -> None:
    g = Graph()
    g.bind("gmeow", GMEOW)
    g.bind("ex", EX)
    spelling = EX.spellingNoName
    g.add((spelling, RDF.type, GMEOW.PitchSpelling))
    g.add((spelling, GMEOW.spellingPitch, GMEOW.pitchValue12EDOCSharp4))
    g.add((spelling, GMEOW.spellingSystem, GMEOW.pitchSpellingSystemCMN))
    result = run_shacl(g)
    assert not result.ok
    assert result.errors
    assert (
        "A PitchSpelling must provide exactly one spelled name string."
        in _error_text(result)
    )


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


def test_standpoint_memberships_pass_shacl() -> None:
    """The two contested Rast-third memberships both validate individually."""
    cases = (
        (
            GMEOW.membershipRastThirdArabic,
            GMEOW.pitchValue24EDOEHalfFlat4,
            GMEOW.standpointArabicTheory,
        ),
        (
            GMEOW.membershipRastThirdTurkish,
            GMEOW.pitchValue24EDOE4,
            GMEOW.standpointTurkishTheory,
        ),
    )
    for membership_iri, pitch_iri, standpoint_iri in cases:
        graph = Graph()
        graph.bind("gmeow", GMEOW)
        graph.add((membership_iri, RDF.type, GMEOW.PitchCollectionMembership))
        graph.add(
            (membership_iri, GMEOW.membershipCollection, GMEOW.pitchCollectionRastMaqam)
        )
        graph.add((membership_iri, GMEOW.membershipPitch, pitch_iri))
        graph.add(
            (membership_iri, GMEOW.membershipRole, GMEOW.collectionMemberRoleMember)
        )
        graph.add(
            (
                membership_iri,
                GMEOW.membershipDegreeIndex,
                Literal(2, datatype=XSD.integer),
            )
        )
        graph.add((membership_iri, GMEOW.accordingTo, standpoint_iri))
        result = run_shacl(graph)
        assert result.ok, f"{membership_iri} failed SHACL: " + _error_text(result)
