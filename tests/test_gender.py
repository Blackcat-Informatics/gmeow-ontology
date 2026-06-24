"""Structural guards for the gender building block — RETAINED pytest tests only.

The asserted-TBox MUST / MUST-NOT invariants (IdentityFacet relator hierarchy,
value-vocab subclasses, functional-per-facet properties, flat-literal bans, and
the sexAssignedAtBirth recorded-not-a-facet guards) have been migrated to
declarative slicetest cells in slices/core/gender/tests/structural.ttl (#867).

RETAINED here (module-scoped cells cannot cover these):
  test_displayable_generalised_to_cover_identity — gmeow:displayable is defined
    in a cross-slice module, not gender/module.ttl; a scopeModule cell would
    silently narrow the guard.
  test_competency_gender_values_query — reads COMPETENCY_DIR external file and
    runs a dynamic count query (len >= 11); cannot be expressed as a static ASK.

Cross-axis independence lives in test_identity_orthogonality.py.
"""

from __future__ import annotations

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, URIRef
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_displayable_generalised_to_cover_identity() -> None:
    """displayable is now domain-free — it covers both Appellation and IdentityFacet."""
    graph = _graph()
    displayable = URIRef(GMEOW + "displayable")
    assert (displayable, RDF.type, OWL.DatatypeProperty) in graph
    # No narrow domain pinning it to Appellation only.
    assert (displayable, RDFS.domain, URIRef(GMEOW + "Appellation")) not in graph


def test_competency_gender_values_query() -> None:
    graph = _graph()
    query = (COMPETENCY_DIR / "gender-values.rq").read_text(encoding="utf-8")
    values: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        values.add(str(row[0]))
    for ind in ("genderWoman", "genderNonBinary", "genderAgender", "genderTwoSpirit"):
        assert GMEOW + ind in values
    assert len(values) >= 11
