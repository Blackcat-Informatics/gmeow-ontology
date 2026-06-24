"""Structural + DL-safety guards for the cross-cutting attestation module.

TBox invariants that are home-asserted in slices/core/attestation/module.ttl
have been migrated to slices/core/attestation/tests/structural.ttl as
declarative slicetest cells (#867). Only the test below was RETAINED because
it is not expressible as a module-scoped SPARQL ASK cell:

- test_certification_still_exists_as_relator: gmeow:Certification is not
  defined in the attestation module; it is a cross-slice subject that
  scopeModule cannot reach.

Migrated to crates/validate/tests/ontology_conformance.rs (#867):
- test_contested_attestation_coexists (contested_attestation_coexists)
- test_all_fixture_files_load (attestation_all_fixture_files_load)
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import Graph, URIRef
from gmeow_rdf.compat.rdflib.namespace import OWL, RDF, RDFS

from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GUFO = "http://purl.org/nemo/gufo#"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Certification preserved and documented (cross-slice -- Certification is NOT
# in slices/core/attestation/module.ttl so scopeModule cannot cover it)
# --------------------------------------------------------------------------- #


def test_certification_still_exists_as_relator() -> None:
    graph = _graph()
    cert = URIRef(GMEOW + "Certification")
    assert (cert, RDF.type, OWL.Class) in graph
    assert (cert, RDFS.subClassOf, URIRef(GUFO + "Relator")) in graph
    # Documentation scopeNote should mention attestation.
    assert (cert, RDFS.comment, None) in graph or (cert, RDFS.label, None) in graph
