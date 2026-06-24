"""Retained tests for the Sensory Environment module (#104).

Structural (asserted TBox) invariants have been migrated to the declarative
slice-test DSL in
slices/extensions/sensory-environment/tests/structural.ttl (cells 1-15,
#867 batch 8). The following tests are retained here because they cannot be
expressed as module-scoped SPARQL ASK cells:

- test_sosa_alignments_loaded: load_mappings() / SOSA SSSOM cross-slice.
- test_psychological_mappings_loaded: load_mappings() / MF+MFOEM SSSOM.
- test_new_axes_exist: axisTristimulusX etc. defined in places, cross-slice.
- test_perceptual_frame_realm_exists: frameRealmPerceptual in places, cross.
- test_mental_reference_frame_requires_host: OWL-RL consistency arm
  (native_rl_closure + ABox construction); structural restriction arm
  migrated to cell 14 in structural.ttl.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.native_rl_rdflib import native_rl_closure
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/test/")


def test_new_axes_exist() -> None:
    """Sensory-environment axes are present in the places module.

    Retained: gmeow:axisTristimulusX etc. are defined as subjects in
    slices/core/places/module.ttl, not in sensory-environment; cross-slice.
    """
    graph = load_merged_graph(include_imports=False)
    for axis in (
        "axisTristimulusX",
        "axisTristimulusY",
        "axisTristimulusZ",
        "axisLightness",
        "axisAstar",
        "axisBstar",
        "axisFrequency",
        "axisMagnitude",
        "axisPredictedMeanVote",
        "axisPredictedPercentageDissatisfied",
    ):
        assert (GMEOW[axis], RDF.type, GMEOW.Axis) in graph


def test_perceptual_frame_realm_exists() -> None:
    """frameRealmPerceptual is present for mental reference frames.

    Retained: gmeow:frameRealmPerceptual is defined as a subject in
    slices/core/places/module.ttl, not in sensory-environment; cross-slice.
    """
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.frameRealmPerceptual, RDF.type, GMEOW.FrameRealm) in graph


def test_sosa_alignments_loaded() -> None:
    """The sensory-environment mapping set contains SOSA alignments."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()

    # At least one SensoryEnvironment → sosa:FeatureOfInterest alignment
    env_matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:SensoryEnvironment"
        and m.predicate_id == "skos:closeMatch"
        and m.object_id == "sosa:FeatureOfInterest"
    ]
    assert env_matches, "SensoryEnvironment must map to sosa:FeatureOfInterest"

    # At least one CoordinateMatrix → sosa:Result alignment
    matrix_matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:CoordinateMatrix"
        and m.predicate_id == "skos:closeMatch"
        and m.object_id == "sosa:Result"
    ]
    assert matrix_matches, "CoordinateMatrix must map to sosa:Result"

    # Verify these mappings come from the sensory-environment SSSOM file
    assert any("sensory-environment" in str(m.source) for m in env_matches)
    assert any("sensory-environment" in str(m.source) for m in matrix_matches)


def test_mental_reference_frame_requires_host() -> None:
    """Issue #87: a hosted MentalReferenceFrame instance is consistent under
    OWL 2 RL.

    Retained (consistency arm only): the OWL-RL closure + ABox construction
    is a reasoning test, not expressible as a module-scoped structural cell.
    The structural blank-node restriction-existence check has been migrated to
    cell ex:saMentalReferenceFrameRestriction in structural.ttl (#867).
    """
    graph = Graph()
    graph.parse(module_path("sensory-environment"), format="turtle")
    graph.parse(module_path("places"), format="turtle")

    # Consistency: a hosted MentalReferenceFrame instance does not contradict
    # the ontology under OWL 2 RL.
    ex_host = EX["host1"]
    ex_frame = EX["mrf1"]
    graph.add((ex_host, RDF.type, GMEOW.Agent))
    graph.add((ex_frame, RDF.type, GMEOW.MentalReferenceFrame))
    graph.add((ex_frame, GMEOW.isHostedBy, ex_host))

    native_rl_closure(graph)
    assert not any(True for _ in graph.subjects(RDF.type, OWL.Nothing)), (
        "Ontology + hosted instance must be consistent"
    )
    assert (ex_frame, RDF.type, GMEOW.ReferenceFrame) in graph


def test_psychological_mappings_loaded() -> None:
    """Issue #87: The sensory-environment mapping set
    contains MF and MFOEM alignments."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()

    # At least one MentalReferenceFrame → mf:mental process alignment
    mental_matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:MentalReferenceFrame"
        and m.predicate_id == "skos:relatedMatch"
        and m.object_id == "bfo:MF_0000020"
    ]
    assert mental_matches, (
        "MentalReferenceFrame must map to bfo:MF_0000020 (mental process)"
    )

    # At least one referenceFrameAffectiveCircumplex → mfoem:affective process alignment
    affective_matches = [
        m
        for m in mappings
        if m.subject_id == "gmeow:referenceFrameAffectiveCircumplex"
        and m.predicate_id == "skos:relatedMatch"
        and m.object_id == "bfo:MFOEM_000195"
    ]
    assert affective_matches, (
        "referenceFrameAffectiveCircumplex must map to "
        "bfo:MFOEM_000195 (affective process)"
    )

    # Verify these mappings come from the sensory-environment SSSOM file
    assert any("sensory-environment" in str(m.source) for m in mental_matches)
    assert any("sensory-environment" in str(m.source) for m in affective_matches)
