"""Universal identity/coreference guards (#74).

Asserted-TBox MUST/MUST-NOT invariants for authorityLink, counterpartOf,
versionOf, editionOf, and supersedes are now declarative slicetest cells in
slices/core/coreference/tests/structural.ttl (16 cells, migrated from
test_authority_link_is_universal_open_and_not_sameas,
test_counterpart_preserves_link_without_identity_merge, and
test_universal_version_and_edition_lineage_terms).

RETAINED here (not migratable to scopeModule cells):
  test_no_preferred_or_primary_coreference_terms -- whole-graph absence sweep
    over banned IRI names; subjects not home-asserted in module.ttl.
  test_schema_sameas_projection_requires_exact_authority_match -- projection
    result check using project_graph(); not expressible as a module-scoped ASK.

Migrated to crates/validate/tests/ontology_conformance.rs (#867):
  test_authority_link_without_match_strength_warns_only
    (authority_link_without_match_strength_warns_only)
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, URIRef

from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.projections import project_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"
SKOS = "http://www.w3.org/2004/02/skos/core#"
EX = "https://example.org/coref/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_no_preferred_or_primary_coreference_terms() -> None:
    graph = _graph()
    for banned in (
        "primaryAuthority",
        "preferredAuthority",
        "primaryCoreference",
        "preferredCoreference",
        "primaryIdentity",
        "preferredIdentity",
    ):
        node = URIRef(GMEOW + banned)
        assert (node, RDF.type, OWL.Class) not in graph
        assert (node, RDF.type, OWL.ObjectProperty) not in graph
        assert (node, RDF.type, OWL.DatatypeProperty) not in graph


def test_schema_sameas_projection_requires_exact_authority_match() -> None:
    src = load_merged_graph(include_imports=False)
    src.parse(FIXTURES_DIR / "coreference.ttl", format="turtle")
    projected = project_graph("schema-org", src)

    subject = URIRef(EX + "recordedPerson")
    exact = URIRef(EX + "authority/person-123")
    close = URIRef(EX + "authority/person-near")
    assert (subject, URIRef(SCHEMA + "sameAs"), exact) in projected
    assert (subject, URIRef(SCHEMA + "sameAs"), close) not in projected

    # Source keeps the SKOS distinction; projection does not leak GMEOW predicates.
    assert (subject, URIRef(SKOS + "closeMatch"), close) in src
    assert not any(str(p).startswith(GMEOW) for _, p, _ in projected)
