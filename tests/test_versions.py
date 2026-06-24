"""Cross-cutting version-set and version-membership guards (#161).

Most TBox invariants have been migrated to declarative slicetest cells in
slices/core/versions/tests/structural.ttl (issue #867).

RETAINED here (cross-slice subject — gmeow:versionLabel is defined in
slices/extensions/languages/module.ttl, not in the versions module, so a
scopeModule cell would silently miss it):
  - test_version_label_domain_is_entity
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# versionLabel domain broadened to Entity
# (cross-slice: versionLabel is defined in slices/extensions/languages/)
# --------------------------------------------------------------------------- #


def test_version_label_domain_is_entity() -> None:
    graph = _graph()
    node = URIRef(GMEOW + "versionLabel")
    assert (node, RDF.type, OWL.DatatypeProperty) in graph
    assert (node, RDFS.domain, URIRef(GMEOW + "Entity")) in graph
