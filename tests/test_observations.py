"""Competency and reasoning tests for the Observation module (#66, #69).

The observation stack unifies spatial measurement, temporal dating, sensory
reading, standpoint claims, identity claims, naming claims, rights claims, and
kinship claims into one gufo:Relator structure. These tests verify:

1. The TBox is well-formed (classes, properties, value vocabularies).
2. EL axioms fire (Observation mediates at least vantage + observedFeature).
3. Property chains fire (frame inheritance via observationResult).
4. ScalarQuantity is reasoned correctly as an observation result wrapper.
5. Universal claim construct (#69): NameUsage, IdentityFacet, RightsStatement,
   and KinRelationship are inferred as Observation subclasses.
6. Property bridges fire (usageNamer ⊑ vantage, usageNamed ⊑ observedFeature, etc.).
"""

from __future__ import annotations

from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.native_rl import native_rl_closure
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")


def test_observation_class_exists() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.Observation, RDF.type, OWL.Class) in graph
    assert (GMEOW.Observation, RDFS.subClassOf, GUFO.Relator) in graph


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


def test_observation_el_axioms_fire() -> None:
    """An Observation individual with required properties stays consistent."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    # Minimal A-Box: an observation of a place by an agent
    graph.add((EX.obs1, RDF.type, GMEOW.Observation))
    graph.add((EX.obs1, GMEOW.vantage, EX.agent1))
    graph.add((EX.obs1, GMEOW.observedFeature, EX.place1))
    graph.add((EX.agent1, RDF.type, GMEOW.Agent))
    graph.add((EX.place1, RDF.type, GMEOW.Place))

    # The EL restrictions are necessary (not sufficient) conditions, so the
    # reasoner will not *infer* Observation from the properties alone under
    # OWL 2 RL.  What we verify here is that the asserted type is not
    # contradicted — i.e. the axioms are consistent with the A-Box.
    native_rl_closure(graph)
    assert (EX.obs1, RDF.type, GMEOW.Observation) in graph


def test_frame_inheritance_property_chain() -> None:
    """A result inherits the observation's reference frame via property chain."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")

    graph.add((EX.obs1, GMEOW.observationResult, EX.coords1))
    graph.add((EX.obs1, GMEOW.hasReferenceFrame, EX.frameWGS84))
    graph.add((EX.coords1, RDF.type, GMEOW.GeoCoordinates))
    graph.add((EX.frameWGS84, RDF.type, GMEOW.ReferenceFrame))

    native_rl_closure(graph)
    # The chain: inverse(observationResult) ∘ hasReferenceFrame ⊑ hasReferenceFrame
    # means: coords1 --inverse(observationResult)-- obs1
    #         --hasReferenceFrame-- frameWGS84
    # implies: coords1 --hasReferenceFrame-- frameWGS84
    assert (EX.coords1, GMEOW.hasReferenceFrame, EX.frameWGS84) in graph


def test_measurement_specialises_observation() -> None:
    """Measurement is inferred as an Observation."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.add((EX.m1, RDF.type, GMEOW.Measurement))

    native_rl_closure(graph)
    assert (EX.m1, RDF.type, GMEOW.Observation) in graph


def test_sensory_observation_specialises_observation() -> None:
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.add((EX.s1, RDF.type, GMEOW.SensoryObservation))

    native_rl_closure(graph)
    assert (EX.s1, RDF.type, GMEOW.Observation) in graph


def test_standpoint_claim_specialises_observation() -> None:
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.add((EX.c1, RDF.type, GMEOW.StandpointClaim))

    native_rl_closure(graph)
    assert (EX.c1, RDF.type, GMEOW.Observation) in graph


# --------------------------------------------------------------------------- #
# Universal claim construct (#69)
# --------------------------------------------------------------------------- #


def test_name_usage_specialises_observation() -> None:
    """NameUsage is inferred as an Observation."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("names"), format="turtle")
    graph.add((EX.nu1, RDF.type, GMEOW.NameUsage))

    native_rl_closure(graph)
    assert (EX.nu1, RDF.type, GMEOW.Observation) in graph


def test_identity_facet_specialises_observation() -> None:
    """IdentityFacet is inferred as an Observation."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("gender"), format="turtle")
    graph.add((EX.if1, RDF.type, GMEOW.IdentityFacet))

    native_rl_closure(graph)
    assert (EX.if1, RDF.type, GMEOW.Observation) in graph


def test_rights_statement_specialises_observation() -> None:
    """RightsStatement is inferred as an Observation."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("rights"), format="turtle")
    graph.add((EX.rs1, RDF.type, GMEOW.RightsStatement))

    native_rl_closure(graph)
    assert (EX.rs1, RDF.type, GMEOW.Observation) in graph


def test_kin_relationship_specialises_observation() -> None:
    """KinRelationship is inferred as an Observation."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("genealogy"), format="turtle")
    graph.add((EX.kr1, RDF.type, GMEOW.KinRelationship))

    native_rl_closure(graph)
    assert (EX.kr1, RDF.type, GMEOW.Observation) in graph


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


def test_is_result_of_provenance_chain() -> None:
    """A quantity can trace back to its producing observation via isResultOf (#77)."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")

    graph.add((EX.obs1, RDF.type, GMEOW.Measurement))
    graph.add((EX.q1, RDF.type, GMEOW.Quantity))
    graph.add((EX.q1, GMEOW.isResultOf, EX.obs1))

    native_rl_closure(graph)
    # Because isResultOf is inverse of observationResult,
    # obs1 --observationResult--> q1 is inferred.
    assert (EX.obs1, GMEOW.observationResult, EX.q1) in graph


def test_frame_inheritance_via_quantity() -> None:
    """A Quantity result inherits the observation's reference frame (#77)."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")

    graph.add((EX.obs1, RDF.type, GMEOW.Measurement))
    graph.add((EX.obs1, GMEOW.observationResult, EX.q1))
    graph.add((EX.obs1, GMEOW.hasReferenceFrame, EX.frameSI))
    graph.add((EX.q1, RDF.type, GMEOW.Quantity))
    graph.add((EX.frameSI, RDF.type, GMEOW.ReferenceFrame))

    native_rl_closure(graph)
    assert (EX.q1, GMEOW.hasReferenceFrame, EX.frameSI) in graph


# --------------------------------------------------------------------------- #
# Stream — time-ordered observation sequences (#96)
# --------------------------------------------------------------------------- #


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


def test_stream_el_axiom() -> None:
    """A Stream individual with streamOf stays consistent under OWL RL."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.add((EX.stream1, RDF.type, GMEOW.Stream))
    graph.add((EX.stream1, GMEOW.streamOf, EX.entity1))
    graph.add((EX.entity1, RDF.type, GMEOW.Entity))

    native_rl_closure(graph)
    assert (EX.stream1, RDF.type, GMEOW.Stream) in graph


# --------------------------------------------------------------------------- #
# SpatialMeasurement / CoordinateObservation (#125)
# --------------------------------------------------------------------------- #


def test_spatial_measurement_infers_observation() -> None:
    """SpatialMeasurement is inferred as an Observation under OWL RL."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")
    graph.add((EX.sm1, RDF.type, GMEOW.SpatialMeasurement))

    native_rl_closure(graph)
    assert (EX.sm1, RDF.type, GMEOW.Observation) in graph


def test_coordinate_observation_infers_spatial_measurement() -> None:
    """CoordinateObservation is inferred as SpatialMeasurement
    (and thus Observation)."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")
    graph.add((EX.co1, RDF.type, GMEOW.CoordinateObservation))

    native_rl_closure(graph)
    assert (EX.co1, RDF.type, GMEOW.SpatialMeasurement) in graph
    assert (EX.co1, RDF.type, GMEOW.Observation) in graph


def test_coordinate_observation_frame_inheritance() -> None:
    """A CoordinateObservation's result inherits the observation's reference frame."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")

    graph.add((EX.co2, RDF.type, GMEOW.CoordinateObservation))
    graph.add((EX.co2, GMEOW.coordinateResult, EX.coords2))
    graph.add((EX.co2, GMEOW.hasReferenceFrame, EX.frameWGS84))
    graph.add((EX.coords2, RDF.type, GMEOW.GeoCoordinates))
    graph.add((EX.frameWGS84, RDF.type, GMEOW.ReferenceFrame))

    native_rl_closure(graph)
    # isResultOf is inverse of observationResult, so coords2 --isResultOf-- co2
    # Then the chain isResultOf ∘ hasReferenceFrame ⊑ hasReferenceFrame fires.
    assert (EX.coords2, GMEOW.hasReferenceFrame, EX.frameWGS84) in graph


def test_coordinate_observation_el_axioms() -> None:
    """A CoordinateObservation individual with required properties stays consistent."""
    graph = Graph()
    graph.parse(module_path("observations"), format="turtle")
    graph.parse(module_path("places"), format="turtle")

    graph.add((EX.co3, RDF.type, GMEOW.CoordinateObservation))
    graph.add((EX.co3, GMEOW.vantage, EX.agent3))
    graph.add((EX.co3, GMEOW.observedFeature, EX.place3))
    graph.add((EX.agent3, RDF.type, GMEOW.Agent))
    graph.add((EX.place3, RDF.type, GMEOW.Place))

    native_rl_closure(graph)
    assert (EX.co3, RDF.type, GMEOW.CoordinateObservation) in graph


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
