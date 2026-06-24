"""Observation module — retained pytest (#66, #69).

The observation-module structural TBox assertions (Observation/Stream class +
property shapes, value-vocabulary + method/type seeds, the module-local
sub-property bridges) were migrated to the slice-resident declarative test-DSL —
``slices/core/observations/tests/structural.ttl``, run by the native Rust
slicetest harness (#867). The OWL-RL ENTAILMENT tests were migrated to
``crates/logic/tests/ontology_entailments.rs`` (#896). See
``dsl/tests/MIGRATION-LEDGER.md``.

What remains here:

* the SOSA/AFO ``*_mapped_to_*`` alignment tests — they read GENERATED mapping
  artifacts (``load_mappings``), an independent live Python surface with no module
  graph and no Rust twin (doctrine-guard) → Keep.
* ``test_kin_relationship_bridges_fire`` — the three KinRelationship sub-property
  bridges are asserted in the GENEALOGY module, not observations, so a
  module-scoped observations cell cannot see them; retained over the merged graph
  pending a genealogy-slice structural migration.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDFS, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)


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


def test_kin_relationship_bridges_fire() -> None:
    """The KinRelationship sub-property bridges expose kinship roles as observation
    roles (#69). These bridges are asserted in the GENEALOGY module (not
    observations), so they are checked over the merged graph rather than a
    module-scoped observations cell — retained pending a genealogy-slice migration.
    """
    graph = load_merged_graph(include_imports=False)
    of = GMEOW.observedFeature
    assert (GMEOW.relationshipParent, RDFS.subPropertyOf, of) in graph
    assert (GMEOW.relationshipChild, RDFS.subPropertyOf, of) in graph
    assert (GMEOW.hasPartner, RDFS.subPropertyOf, of) in graph
