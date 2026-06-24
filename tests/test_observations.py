"""Structural TBox tests for the Observation module (#66, #69).

The observation stack unifies spatial measurement, temporal dating, sensory
reading, standpoint claims, identity claims, naming claims, rights claims, and
kinship claims into one gufo:Relator structure. These tests verify, over the
ASSERTED graph, that the TBox is well-formed (classes, properties, value
vocabularies, Stream/property characteristics, asserted sub-property bridges).

The OWL 2 RL ENTAILMENT tests — EL consistency, the frame-inheritance / isResultOf
property chains, and the universal-claim-construct subsumptions (Measurement,
SensoryObservation, StandpointClaim, NameUsage, IdentityFacet, RightsStatement,
KinRelationship, SpatialMeasurement, CoordinateObservation ⊑ Observation) — were
migrated to the native Rust RL harness
(``crates/logic/tests/ontology_entailments.rs``, issue #896). See
``dsl/tests/MIGRATION-LEDGER.md``.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")
EX = Namespace("https://example.org/test/")


def test_observation_class_exists() -> None:
    """After #694 migration: gufo:Relator → logic:Relator."""
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.Observation, RDF.type, OWL.Class) in graph
    assert (GMEOW.Observation, RDFS.subClassOf, LOGIC.Relator) in graph


def test_observation_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for prop in (
        "observedFeature",
        "observationResult",
        "isResultOf",
        "vantage",
        "observationMethod",
        "observationType",
        "observationEvent",
    ):
        assert (GMEOW[prop], RDF.type, OWL.ObjectProperty) in graph


def test_observation_value_vocabularies_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "ObservationMethod",
        "ObservationType",
        "Measurement",
        "SensoryObservation",
        "StandpointClaim",
        "ScalarQuantity",
        "Quantity",
        "MeasuredValue",
    ):
        assert (GMEOW[term], RDF.type, OWL.Class) in graph


def test_observation_type_seeds_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "observationTypeMeasurement",
        "observationTypeSensory",
        "observationTypeStandpoint",
        "observationTypeDerived",
        "observationTypeSimulation",
        "observationTypeIdentity",
        "observationTypeNaming",
        "observationTypeRights",
        "observationTypeKinship",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.ObservationType) in graph


def test_observation_method_seeds_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "methodDirectObservation",
        "methodInstrumentalReading",
        "methodSurvey",
        "methodRemoteSensing",
        "methodComputationalModel",
        "methodExpertJudgement",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.ObservationMethod) in graph


def test_scalar_quantity_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.quantityValue, RDF.type, OWL.DatatypeProperty) in graph
    assert (GMEOW.quantityUncertainty, RDF.type, OWL.DatatypeProperty) in graph


def test_property_bridges_fire() -> None:
    """Subproperty bridges expose domain-specific properties as observation roles."""
    graph = load_merged_graph(include_imports=False)
    # NameUsage bridges
    assert (GMEOW.usageNamer, RDFS.subPropertyOf, GMEOW.vantage) in graph
    assert (GMEOW.usageNamed, RDFS.subPropertyOf, GMEOW.observedFeature) in graph
    assert (
        GMEOW.usageAppellation,
        RDFS.subPropertyOf,
        GMEOW.observationResult,
    ) in graph
    # RightsStatement bridge
    assert (GMEOW.statementAbout, RDFS.subPropertyOf, GMEOW.observedFeature) in graph
    # IdentityFacet bridges
    assert (GMEOW.facetSubject, RDFS.subPropertyOf, GMEOW.observedFeature) in graph
    assert (GMEOW.facetVantage, RDFS.subPropertyOf, GMEOW.vantage) in graph
    # KinRelationship bridges
    assert (
        GMEOW.relationshipParent,
        RDFS.subPropertyOf,
        GMEOW.observedFeature,
    ) in graph
    assert (
        GMEOW.relationshipChild,
        RDFS.subPropertyOf,
        GMEOW.observedFeature,
    ) in graph
    assert (GMEOW.hasPartner, RDFS.subPropertyOf, GMEOW.observedFeature) in graph
    # VersionMembership bridges
    assert (
        GMEOW.versionMember,
        RDFS.subPropertyOf,
        GMEOW.observedFeature,
    ) in graph
    assert (
        GMEOW.membershipAuthority,
        RDFS.subPropertyOf,
        GMEOW.vantage,
    ) in graph


def test_standpoint_claim_aligned_to_sosa_observation() -> None:
    """The standpoint-indexed statement is aligned to sosa:Observation (#68)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    observation_mappings = [
        m for m in mappings if m.subject_id == "gmeow:StandpointClaim"
    ]
    assert observation_mappings, "StandpointClaim must have at least one mapping"
    sosa_matches = [
        m for m in observation_mappings if m.object_id == "sosa:Observation"
    ]
    assert sosa_matches, "StandpointClaim must map to sosa:Observation"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_agent_aligned_to_sosa_sensor_as_standpoint() -> None:
    """Agent-as-vantage is a standpoint, bridged to sosa:Sensor (#68)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    agent_mappings = [m for m in mappings if m.subject_id == "gmeow:Agent"]
    sosa_matches = [m for m in agent_mappings if m.object_id == "sosa:Sensor"]
    assert sosa_matches, (
        "Agent must map to sosa:Sensor (observer/sensor/perceiver as standpoint)"
    )
    assert sosa_matches[0].predicate_id == "skos:broadMatch"


def test_quantity_equivalent_to_scalar_quantity() -> None:
    """Quantity is equivalent to ScalarQuantity (#77)."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.Quantity,
        OWL.equivalentClass,
        GMEOW.ScalarQuantity,
    ) in graph


def test_measured_value_equivalent_to_quantity() -> None:
    """MeasuredValue is equivalent to Quantity (#77)."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.MeasuredValue,
        OWL.equivalentClass,
        GMEOW.Quantity,
    ) in graph


def test_is_result_of_is_inverse_of_observation_result() -> None:
    """isResultOf is declared as the inverse of observationResult (#77)."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.isResultOf,
        OWL.inverseOf,
        GMEOW.observationResult,
    ) in graph


def test_stream_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.Stream, RDF.type, OWL.Class) in graph
    assert (GMEOW.Stream, RDFS.subClassOf, GMEOW.Entity) in graph


def test_stream_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for prop in (
        "streamOf",
        "hasStream",
        "streamSample",
        "streamPlatform",
        "streamSensor",
        "streamInterval",
    ):
        assert (GMEOW[prop], RDF.type, OWL.ObjectProperty) in graph


def test_stream_of_is_functional() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.streamOf, RDF.type, OWL.FunctionalProperty) in graph
    assert (GMEOW.streamOf, RDFS.domain, GMEOW.Stream) in graph
    assert (GMEOW.streamOf, RDFS.range, GMEOW.Entity) in graph


def test_has_stream_is_inverse_of_stream_of() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.hasStream, OWL.inverseOf, GMEOW.streamOf) in graph


def test_stream_sample_is_non_functional() -> None:
    graph = load_merged_graph(include_imports=False)
    prop = GMEOW.streamSample
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph
    assert (prop, RDFS.domain, GMEOW.Stream) in graph
    assert (prop, RDFS.range, GMEOW.Entity) in graph


def test_stream_platform_is_non_functional() -> None:
    graph = load_merged_graph(include_imports=False)
    prop = GMEOW.streamPlatform
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph
    assert (prop, RDFS.range, GMEOW.Agent) in graph


def test_stream_sensor_is_non_functional() -> None:
    graph = load_merged_graph(include_imports=False)
    prop = GMEOW.streamSensor
    assert (prop, RDF.type, OWL.ObjectProperty) in graph
    assert (prop, RDF.type, OWL.FunctionalProperty) not in graph
    assert (prop, RDFS.range, GMEOW.Agent) in graph


def test_stream_interval_is_functional() -> None:
    graph = load_merged_graph(include_imports=False)
    prop = GMEOW.streamInterval
    assert (prop, RDF.type, OWL.FunctionalProperty) in graph
    assert (prop, RDFS.domain, GMEOW.Stream) in graph
    assert (prop, RDFS.range, GMEOW.TimeInterval) in graph


def test_streaming_observation_type_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.observationTypeStreaming, RDF.type, GMEOW.ObservationType) in graph


def test_streaming_method_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.methodStreaming, RDF.type, GMEOW.ObservationMethod) in graph


def test_coordinate_observation_mapped_to_sosa() -> None:
    """CoordinateObservation is aligned to sosa:Observation in the mappings."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    co_mappings = [m for m in mappings if m.subject_id == "gmeow:CoordinateObservation"]
    assert co_mappings, "CoordinateObservation must have at least one mapping"
    sosa_matches = [m for m in co_mappings if m.object_id == "sosa:Observation"]
    assert sosa_matches, "CoordinateObservation must map to sosa:Observation"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_spatial_measurement_mapped_to_sosa() -> None:
    """SpatialMeasurement is aligned to sosa:Observation in the mappings."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    sm_mappings = [m for m in mappings if m.subject_id == "gmeow:SpatialMeasurement"]
    assert sm_mappings, "SpatialMeasurement must have at least one mapping"
    sosa_matches = [m for m in sm_mappings if m.object_id == "sosa:Observation"]
    assert sosa_matches, "SpatialMeasurement must map to sosa:Observation"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"
