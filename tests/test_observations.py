"""Observation module — retained pytest (#66, #69).

The observation-module structural TBox assertions (Observation/Stream class +
property shapes, value-vocabulary + method/type seeds, the module-local
sub-property bridges) were migrated to the slice-resident declarative test-DSL —
``slices/core/observations/tests/structural.ttl``, run by the native Rust
slicetest harness (#867). The OWL-RL ENTAILMENT tests were migrated to
``crates/logic/tests/ontology_entailments.rs`` (#896). See
``dsl/tests/MIGRATION-LEDGER.md``.

What remains here:

* ``test_kin_relationship_bridges_fire`` — the three KinRelationship sub-property
  bridges are asserted in the GENEALOGY module, not observations, so a
  module-scoped observations cell cannot see them; retained over the merged graph
  pending a genealogy-slice structural migration.

The SOSA/AFO ``*_mapped_to_*`` SSSOM-alignment checks were removed: the
generated-mapping projection is now enforced by the native Rust dialect
lowerings and their byte-iso parity oracles (#1092 / F5).
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import RDFS, Namespace

from gmeow_tools.config import NAMESPACE
from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace(NAMESPACE)


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
