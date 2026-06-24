"""Tests for the Profile meta-pattern (issue #75).

The TBox structural assertions (the profile class/meta-property/seed-profile checks)
have been migrated to slices/core/profiles/tests/structural.ttl as declarative
gmeow:StructuralAssertion cells (cells ex:saProfileClassExists through
ex:saTemporalProvenanceProfileExists). Only the run_shacl / ExampleConformance
tests remain here — they build synthetic graphs and cannot be expressed as
scopeModule ASK cells.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace
from gmeow_rdf.compat.rdflib.namespace import RDF, RDFS, SKOS

from tests._graph_nt import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")


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
