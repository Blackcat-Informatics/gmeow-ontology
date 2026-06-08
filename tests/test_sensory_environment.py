"""Competency and reasoning tests for the Sensory Environment module (#104).

The sensory environment module models ambient perceivable conditions at a
Location x time. Measured conditions are expressed as CoordinateMatrix values
in measurement reference frames; perceived conditions are standpoint-indexed
values in MentalReferenceFrames. These tests verify:

1. The TBox is well-formed (classes, properties, value vocabularies).
2. EL axioms fire (SensoryEnvironment mediates at least one location).
3. Specialisation hierarchy (SensoryPerception ⊑ StandpointClaim ⊑ Observation).
4. Frame inheritance via property chain (isResultOf ∘ hasReferenceFrame).
5. Bridge properties (perceptionEnvironment ⊑ observedFeature).
6. Reference frame seeds exist (CIEXYZ, CIELAB, AudioSpectrum, ThermalComfort).
7. SOSA alignments are present in the mapping set.
"""

from __future__ import annotations

import owlrl
from rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace

from gmeow_tools.config import NAMESPACE, ONTOLOGY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def test_sensory_environment_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.SensoryEnvironment, RDF.type, OWL.Class) in graph
    assert (GMEOW.SensoryEnvironment, RDFS.subClassOf, GUFO.Object) in graph


def test_coordinate_matrix_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.CoordinateMatrix, RDF.type, OWL.Class) in graph
    assert (GMEOW.CoordinateMatrix, RDFS.subClassOf, GUFO.Object) in graph


def test_mental_reference_frame_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.MentalReferenceFrame, RDF.type, OWL.Class) in graph
    assert (GMEOW.MentalReferenceFrame, RDFS.subClassOf, GMEOW.ReferenceFrame) in graph


def test_sensory_perception_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.SensoryPerception, RDF.type, OWL.Class) in graph
    assert (GMEOW.SensoryPerception, RDFS.subClassOf, GMEOW.StandpointClaim) in graph


def test_sensory_modality_value_vocabulary_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.SensoryModality, RDF.type, OWL.Class) in graph
    for term in (
        "sensoryModalityVisual",
        "sensoryModalityAuditory",
        "sensoryModalityOlfactory",
        "sensoryModalityGustatory",
        "sensoryModalityTactile",
        "sensoryModalityThermal",
        "sensoryModalityAirQuality",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.SensoryModality) in graph


def test_sensory_environment_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for prop in (
        "environmentAtLocation",
        "environmentAtInstant",
        "environmentDuringInterval",
        "hasMeasuredCondition",
        "hasPerceivedCondition",
        "sensoryModality",
        "perceptionEnvironment",
        "perceptionModality",
    ):
        assert (GMEOW[prop], RDF.type, OWL.ObjectProperty) in graph


def test_coordinate_matrix_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.matrixValue, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.matrixShape, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.coordinateMatrixFrame, RDF.type, OWL.ObjectProperty) in graph


def test_sensory_environment_el_axioms_fire() -> None:
    """A SensoryEnvironment individual with a location stays consistent."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "sensory-environment.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "places.ttl", format="turtle")

    graph.add((EX.env1, GMEOW.environmentAtLocation, EX.place1))
    graph.add((EX.place1, RDF.type, GMEOW.Place))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.env1, RDF.type, GMEOW.SensoryEnvironment) in graph


def test_sensory_perception_specialises_standpoint_claim() -> None:
    """SensoryPerception is inferred as a StandpointClaim."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "sensory-environment.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "standpoint.ttl", format="turtle")

    graph.add((EX.perc1, RDF.type, GMEOW.SensoryPerception))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.perc1, RDF.type, GMEOW.StandpointClaim) in graph
    assert (EX.perc1, RDF.type, GMEOW.Observation) in graph


def test_mental_reference_frame_specialises_reference_frame() -> None:
    """MentalReferenceFrame is inferred as a ReferenceFrame."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "sensory-environment.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "places.ttl", format="turtle")

    graph.add((EX.mrf1, RDF.type, GMEOW.MentalReferenceFrame))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.mrf1, RDF.type, GMEOW.ReferenceFrame) in graph


def test_frame_inheritance_via_coordinate_matrix() -> None:
    """A CoordinateMatrix result inherits the observation's reference frame."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "sensory-environment.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "observations.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "places.ttl", format="turtle")

    graph.add((EX.obs1, RDF.type, GMEOW.SensoryObservation))
    graph.add((EX.obs1, GMEOW.observationResult, EX.matrix1))
    graph.add((EX.obs1, GMEOW.hasReferenceFrame, EX.frameCIEXYZ))
    graph.add((EX.matrix1, RDF.type, GMEOW.CoordinateMatrix))
    graph.add((EX.frameCIEXYZ, RDF.type, GMEOW.ReferenceFrame))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert (EX.matrix1, GMEOW.hasReferenceFrame, EX.frameCIEXYZ) in graph


def test_perception_environment_bridge() -> None:
    """perceptionEnvironment is a subPropertyOf observedFeature."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.perceptionEnvironment,
        RDFS.subPropertyOf,
        GMEOW.observedFeature,
    ) in graph


def test_coordinate_matrix_frame_is_subproperty() -> None:
    """coordinateMatrixFrame is a subPropertyOf hasReferenceFrame."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.coordinateMatrixFrame,
        RDFS.subPropertyOf,
        GMEOW.hasReferenceFrame,
    ) in graph


def test_reference_frame_seeds_exist() -> None:
    """Seed reference frames for sensory measurement are present."""
    graph = load_merged_graph(include_imports=False)
    for frame, expected_type in (
        ("referenceFrameCIEXYZ", GMEOW.ReferenceFrame),
        ("referenceFrameCIELAB", GMEOW.ReferenceFrame),
        ("referenceFrameAudioSpectrum", GMEOW.ReferenceFrame),
        ("referenceFrameThermalComfort", GMEOW.MentalReferenceFrame),
    ):
        assert (GMEOW[frame], RDF.type, expected_type) in graph


def test_thermal_comfort_is_mental_reference_frame() -> None:
    """referenceFrameThermalComfort is typed as a MentalReferenceFrame and
    requires a host, matching the MentalReferenceFrame class contract."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.referenceFrameThermalComfort,
        RDF.type,
        GMEOW.MentalReferenceFrame,
    ) in graph
    assert (
        GMEOW.referenceFrameThermalComfort,
        GMEOW.requiresHost,
        Literal(True),
    ) in graph


def test_new_axes_exist() -> None:
    """Sensory-environment axes are present in the places module."""
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
    """frameRealmPerceptual is present for mental reference frames."""
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
    """Issue #87: MentalReferenceFrame carries an existential restriction on
    isHostedBy; verify the axiom exists and that a hosted instance is consistent
    under OWL 2 RL (necessary-but-not-sufficient: check structure + consistency)."""
    graph = Graph()
    graph.parse(ONTOLOGY_DIR / "modules" / "sensory-environment.ttl", format="turtle")
    graph.parse(ONTOLOGY_DIR / "modules" / "places.ttl", format="turtle")

    # Structural: the restriction axiom must exist on MentalReferenceFrame.
    restrictions = list(graph.objects(GMEOW.MentalReferenceFrame, RDFS.subClassOf))
    assert any(
        (r, OWL.onProperty, GMEOW.isHostedBy) in graph
        and (r, OWL.someValuesFrom, GMEOW.Entity) in graph
        for r in restrictions
    ), "MentalReferenceFrame must restrict isHostedBy some Entity"

    # Consistency: a hosted MentalReferenceFrame instance does not contradict
    # the ontology under OWL 2 RL.
    ex_host = EX["host1"]
    ex_frame = EX["mrf1"]
    graph.add((ex_host, RDF.type, GMEOW.Agent))
    graph.add((ex_frame, RDF.type, GMEOW.MentalReferenceFrame))
    graph.add((ex_frame, GMEOW.isHostedBy, ex_host))

    owlrl.DeductiveClosure(owlrl.OWLRL_Semantics).expand(graph)
    assert not any(True for _ in graph.subjects(RDF.type, OWL.Nothing)), (
        "Ontology + hosted instance must be consistent"
    )
    assert (ex_frame, RDF.type, GMEOW.ReferenceFrame) in graph


def test_psychological_reference_frame_seeds_exist() -> None:
    """Issue #87: Seed mental reference frames for the
    psychological realm are present."""
    graph = load_merged_graph(include_imports=False)
    for frame in (
        "referenceFrameAffectiveCircumplex",
        "referenceFrameConceptualSpace",
        "referenceFrameCognitiveMapEgocentric",
        "referenceFrameCognitiveMapAllocentric",
        "referenceFrameImaginedSpace",
    ):
        assert (GMEOW[frame], RDF.type, GMEOW.MentalReferenceFrame) in graph


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
