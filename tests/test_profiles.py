"""Tests for the Profile meta-pattern (issue #75)."""

from __future__ import annotations

from rdflib import Graph, Literal, Namespace
from rdflib.namespace import OWL, RDF, RDFS, SKOS

from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=True)


def test_profile_class_exists() -> None:
    g = _graph()
    assert (GMEOW.Profile, RDF.type, OWL.Class) in g
    assert (GMEOW.Profile, RDFS.subClassOf, GMEOW.InformationObject) in g


def test_profile_meta_properties_exist() -> None:
    g = _graph()
    for prop in (
        "hasProfile",
        "profileAppliesTo",
        "profileOpenValue",
    ):
        p = GMEOW[prop]
        assert (p, RDF.type, OWL.ObjectProperty) in g, f"{p} missing"
    assert (GMEOW.profileDescriptor, RDF.type, OWL.AnnotationProperty) in g


def test_reference_frame_profile_exists_with_descriptors() -> None:
    g = _graph()
    profile = GMEOW.profileReferenceFrame
    assert (profile, RDF.type, GMEOW.Profile) in g
    assert (profile, GMEOW.profileAppliesTo, GMEOW.ReferenceFrame) in g
    # At least a handful of descriptor properties are declared
    descriptors = set(g.objects(profile, GMEOW.profileDescriptor))
    assert GMEOW.frameRealm in descriptors
    assert GMEOW.hasAxis in descriptors
    assert GMEOW.dimensionCount in descriptors
    open_values = set(g.objects(profile, GMEOW.profileOpenValue))
    assert GMEOW.FrameRealm in open_values
    assert GMEOW.Axis in open_values


def test_temporal_frame_profile_exists() -> None:
    g = _graph()
    profile = GMEOW.profileTemporalFrame
    assert (profile, RDF.type, GMEOW.Profile) in g
    assert (profile, GMEOW.profileAppliesTo, GMEOW.TemporalFrame) in g
    descriptors = set(g.objects(profile, GMEOW.profileDescriptor))
    assert GMEOW.frameTimeScale in descriptors
    assert GMEOW.frameCalendarSystem in descriptors


def test_temporal_provenance_profile_exists() -> None:
    g = _graph()
    profile = GMEOW.profileTemporalProvenance
    assert (profile, RDF.type, GMEOW.Profile) in g
    descriptors = set(g.objects(profile, GMEOW.profileDescriptor))
    assert GMEOW.validFrom in descriptors
    assert GMEOW.validUntil in descriptors
    assert GMEOW.assertedAt in descriptors
    assert GMEOW.recordedNoLaterThan in descriptors


def test_profile_shape_passes_for_wellformed_profile() -> None:
    ok = Graph()
    ok.add((EX.myProfile, RDF.type, GMEOW.Profile))
    ok.add((EX.myProfile, RDFS.label, Literal("My profile")))
    ok.add((EX.myProfile, SKOS.definition, Literal("A test profile.")))
    ok.add((EX.myProfile, GMEOW.profileDescriptor, GMEOW.hasProfile))
    result = run_shacl(ok)
    assert result.ok, "\n".join(result.errors)


def test_profile_shape_fails_for_invalid_profile_applies_to() -> None:
    bad = Graph()
    bad.add((EX.myProfile, RDF.type, GMEOW.Profile))
    bad.add((EX.myProfile, RDFS.label, Literal("Bad profile")))
    bad.add(
        (EX.myProfile, SKOS.definition, Literal("profileAppliesTo must be a class."))
    )
    bad.add((EX.myProfile, GMEOW.profileDescriptor, GMEOW.hasProfile))
    bad.add((EX.myProfile, GMEOW.profileAppliesTo, Literal("not-a-class")))
    result = run_shacl(bad)
    assert not result.ok
    assert any(
        "profileAppliesTo" in e or "ProfileShape" in e or "class" in e
        for e in result.errors
    )


def test_profile_open_value_guard_warns_on_orphan() -> None:
    bad = Graph()
    bad.add((GMEOW.profileReferenceFrame, RDF.type, GMEOW.Profile))
    bad.add(
        (GMEOW.profileReferenceFrame, RDFS.label, Literal("Reference Frame Profile"))
    )
    bad.add(
        (
            GMEOW.profileReferenceFrame,
            SKOS.definition,
            Literal("Closed descriptor schema for reference frames."),
        )
    )
    bad.add((GMEOW.profileReferenceFrame, GMEOW.profileDescriptor, GMEOW.frameRealm))
    bad.add((GMEOW.profileReferenceFrame, GMEOW.profileOpenValue, GMEOW.FrameRealm))
    bad.add((EX.orphanRealm, RDF.type, GMEOW.FrameRealm))
    result = run_shacl(bad)
    assert result.ok  # Warning only
    assert any(
        "Open value individuals must be referenced by at least one profile descriptor"
        in w
        for w in result.warnings
    )
