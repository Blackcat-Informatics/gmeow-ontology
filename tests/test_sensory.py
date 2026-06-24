"""Mapping tests for the Sensory module (#126) -- SOSA / AFO alignments only.

The asserted-TBox structural checks (classes, properties, value-vocabulary
seeds, equivalentClass, subPropertyOf, inverseOf) have been migrated to the
declarative slicetest DSL cell file:

    slices/extensions/sensory/tests/structural.ttl

Migrated functions (now cells sa1-sa7):
  test_sensory_classes_exist              -> ex:saSensoryClassesExist
  test_sensory_properties_exist           -> ex:saSensoryPropertiesExist
  test_observable_property_seeds_exist    -> ex:saObservablePropertySeedsExist
  test_sensory_quantity_equivalent_to_scalar_quantity
      -> ex:saSensoryQuantityEquivalentScalarQuantity
  test_sensory_result_subproperty_of_observation_result
      -> ex:saSensoryResultSubPropertyOfObservationResult
  test_sensory_observation_of_subproperty_of_observed_feature
      -> ex:saSensoryObservationOfSubPropertyOfObservedFeature
  test_has_sensory_observation_is_inverse -> ex:saHasSensoryObservationIsInverse

The OWL 2 RL ENTAILMENT tests were migrated to the native Rust RL harness
(crates/logic/tests/ontology_entailments.rs, issue #896).
See dsl/tests/MIGRATION-LEDGER.md.

RETAINED here (load_mappings() / external generated-artifact reads):
  test_sensor_mapped_to_sosa_sensor
  test_sensor_platform_mapped_to_sosa_platform
  test_observable_property_mapped_to_sosa
  test_sensory_quantity_mapped_to_sosa_result
  test_sensory_property_mapped_to_sosa_observed_property
  test_platform_location_mapped_to_geo_location
  test_sensory_afo_mappings_exist
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import (
    RDF,
    Graph,
    Namespace,
    URIRef,
)
from gmeow_rdf.compat.rdflib.namespace import SKOS

from gmeow_tools.config import NAMESPACE
from gmeow_tools.slices import module_path

GMEOW = Namespace(NAMESPACE)

SENSORY_EQ_FILE = module_path("sensory").parent / "mappings" / "equivalences.ttl"


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
