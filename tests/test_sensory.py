"""Structural TBox + mapping tests for the Sensory module (#126).

The sensory module deepens the SensoryObservation stub with sensor-specific
properties (sensoryProperty, sensoryResult), introduces Sensor, SensorPlatform,
ObservableProperty (value vocabulary), and SensoryQuantity (equivalent alias for
ScalarQuantity). These tests verify, over the ASSERTED graph / loaded mappings:

1. The TBox is well-formed (classes, properties, value vocabularies).
2. The bridge properties / equivalentClass / inverseOf axioms are asserted
   (sensoryResult ⊑ observationResult, sensoryObservationOf ⊑ observedFeature,
   SensoryQuantity ≡ ScalarQuantity, hasSensoryObservation inverseOf …).
3. SOSA / AFO alignments are present in the loaded mappings.

The OWL 2 RL ENTAILMENT tests — SensoryObservation ⊑ Observation, Sensor ⊑ Agent,
SensoryQuantity inheritance, the frame-inheritance and hasSensoryQuantity property
chains, EL consistency, and contested-reading coexistence — were migrated to the
native Rust RL harness (``crates/logic/tests/ontology_entailments.rs``, issue #896).
See ``dsl/tests/MIGRATION-LEDGER.md``.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import (
    OWL,
    RDF,
    RDFS,
    Graph,
    Namespace,
    URIRef,
)
from gmeow_rdf.compat.rdflib.namespace import SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://example.org/test/")

SENSORY_EQ_FILE = module_path("sensory").parent / "mappings" / "equivalences.ttl"


# --------------------------------------------------------------------------- #
# TBox well-formedness
# --------------------------------------------------------------------------- #


def test_sensory_classes_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for cls in (
        "Sensor",
        "SensorPlatform",
        "ObservableProperty",
        "SensoryQuantity",
        "SensoryObservation",
    ):
        assert (GMEOW[cls], RDF.type, OWL.Class) in graph


def test_sensory_properties_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for prop in (
        "sensoryProperty",
        "sensoryResult",
        "sensoryObservationOf",
        "hasSensoryObservation",
        "platformLocation",
        "hasSensoryQuantity",
    ):
        assert (GMEOW[prop], RDF.type, OWL.ObjectProperty) in graph


def test_observable_property_seeds_exist() -> None:
    graph = load_merged_graph(include_imports=False)
    for term in (
        "observablePropertyTemperature",
        "observablePropertyHumidity",
        "observablePropertyLightIntensity",
        "observablePropertySoundPressureLevel",
        "observablePropertyAtmosphericPressure",
        "observablePropertyAirQualityIndex",
        "observablePropertyRadiationLevel",
        "observablePropertyTimbre",
        "observablePropertyLoudness",
        "observablePropertyRoughness",
        "observablePropertyTimingDeviation",
    ):
        assert (GMEOW[term], RDF.type, GMEOW.ObservableProperty) in graph


# --------------------------------------------------------------------------- #
# Class hierarchy and equivalence (#126, #77)
# --------------------------------------------------------------------------- #


def test_sensory_quantity_equivalent_to_scalar_quantity() -> None:
    """SensoryQuantity is equivalent to ScalarQuantity (#77, #126)."""
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.SensoryQuantity,
        OWL.equivalentClass,
        GMEOW.ScalarQuantity,
    ) in graph


def test_sensory_result_subproperty_of_observation_result() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (GMEOW.sensoryResult, RDFS.subPropertyOf, GMEOW.observationResult) in graph


def test_sensory_observation_of_subproperty_of_observed_feature() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.sensoryObservationOf,
        RDFS.subPropertyOf,
        GMEOW.observedFeature,
    ) in graph


def test_has_sensory_observation_is_inverse() -> None:
    graph = load_merged_graph(include_imports=False)
    assert (
        GMEOW.hasSensoryObservation,
        OWL.inverseOf,
        GMEOW.sensoryObservationOf,
    ) in graph


# --------------------------------------------------------------------------- #
# EL axioms — consistency
# --------------------------------------------------------------------------- #


def test_sensor_mapped_to_sosa_sensor() -> None:
    """Sensor is aligned to sosa:Sensor (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    sensor_mappings = [m for m in mappings if m.subject_id == "gmeow:Sensor"]
    assert sensor_mappings, "Sensor must have at least one mapping"
    sosa_matches = [m for m in sensor_mappings if m.object_id == "sosa:Sensor"]
    assert sosa_matches, "Sensor must map to sosa:Sensor"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_sensor_platform_mapped_to_sosa_platform() -> None:
    """SensorPlatform is aligned to sosa:Platform (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    platform_mappings = [m for m in mappings if m.subject_id == "gmeow:SensorPlatform"]
    assert platform_mappings, "SensorPlatform must have at least one mapping"
    sosa_matches = [m for m in platform_mappings if m.object_id == "sosa:Platform"]
    assert sosa_matches, "SensorPlatform must map to sosa:Platform"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_observable_property_mapped_to_sosa() -> None:
    """ObservableProperty is aligned to sosa:ObservableProperty (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    prop_mappings = [m for m in mappings if m.subject_id == "gmeow:ObservableProperty"]
    assert prop_mappings, "ObservableProperty must have at least one mapping"
    sosa_matches = [
        m for m in prop_mappings if m.object_id == "sosa:ObservableProperty"
    ]
    assert sosa_matches, "ObservableProperty must map to sosa:ObservableProperty"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_sensory_quantity_mapped_to_sosa_result() -> None:
    """SensoryQuantity is aligned to sosa:Result (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    sq_mappings = [m for m in mappings if m.subject_id == "gmeow:SensoryQuantity"]
    assert sq_mappings, "SensoryQuantity must have at least one mapping"
    sosa_matches = [m for m in sq_mappings if m.object_id == "sosa:Result"]
    assert sosa_matches, "SensoryQuantity must map to sosa:Result"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_sensory_property_mapped_to_sosa_observed_property() -> None:
    """sensoryProperty is aligned to sosa:observedProperty (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    sp_mappings = [m for m in mappings if m.subject_id == "gmeow:sensoryProperty"]
    assert sp_mappings, "sensoryProperty must have at least one mapping"
    sosa_matches = [m for m in sp_mappings if m.object_id == "sosa:observedProperty"]
    assert sosa_matches, "sensoryProperty must map to sosa:observedProperty"
    assert sosa_matches[0].predicate_id == "skos:closeMatch"


def test_platform_location_mapped_to_geo_location() -> None:
    """platformLocation is aligned to geo:location (#126)."""
    from gmeow_tools.mappings import load_mappings

    mappings = load_mappings()
    pl_mappings = [m for m in mappings if m.subject_id == "gmeow:platformLocation"]
    assert pl_mappings, "platformLocation must have at least one mapping"
    geo_matches = [m for m in pl_mappings if m.object_id == "geo:location"]
    assert geo_matches, "platformLocation must map to geo:location"
    assert geo_matches[0].predicate_id == "skos:closeMatch"


def test_sensory_afo_mappings_exist() -> None:
    graph = Graph()
    graph.parse(SENSORY_EQ_FILE, format="turtle")
    expected = {
        "eqSens001": (
            "observablePropertyTimbre",
            "https://w3id.org/afo/onto/1.1#AudioFeature",
            SKOS.closeMatch,
        ),
        "eqSens002": (
            "observablePropertyTimbre",
            "https://w3id.org/afo/vocab/1.1#TimbreDistribution",
            SKOS.relatedMatch,
        ),
        "eqSens003": (
            "observablePropertyLoudness",
            "https://w3id.org/afo/vocab/1.1#Loudness",
            SKOS.closeMatch,
        ),
        "eqSens004": (
            "observablePropertyRoughness",
            "https://w3id.org/afo/vocab/1.1#Roughness",
            SKOS.closeMatch,
        ),
        "eqSens005": (
            "observablePropertyTimingDeviation",
            "https://w3id.org/afo/vocab/1.1#Onset",
            SKOS.relatedMatch,
        ),
    }
    for eq, (subject_local, object_iri, predicate) in expected.items():
        eq_iri = GMEOW[eq]
        assert (eq_iri, RDF.type, GMEOW.TermEquivalence) in graph, f"{eq} missing"
        assert (eq_iri, GMEOW.alignPredicate, predicate) in graph
        subject_iri = GMEOW[subject_local]
        assert (eq_iri, GMEOW.alignSubject, subject_iri) in graph
        assert (eq_iri, GMEOW.alignObject, URIRef(object_iri)) in graph
