"""Structural + DL-safety guards for the language varieties, states, and
change events layer (#170).

Pins the contested-classification pattern: varietyKind is non-functional and
standpoint-scoped, LanguageState follows the VersionMembership relator pattern,
and LanguageChangeEvent follows the Activity event pattern.
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import XSD
from gmeow_rdf.compat.rdflib.query import ResultRow
from gmeow_rdf.compat.rdflib.term import Identifier

from gmeow_tools.config import TEMPORAL_QUERY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
COVERAGE_FIXTURES = Path(__file__).parent / "fixtures" / "coverage"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Variety hierarchy and non-functionality
# --------------------------------------------------------------------------- #


def test_variety_hierarchy() -> None:
    g = _graph()
    assert (GM.LanguageVariety, RDFS.subClassOf, GM.Language) in g


def test_variety_kind_is_non_functional() -> None:
    g = _graph()
    assert (GM.varietyKind, RDF.type, OWL.ObjectProperty) in g
    assert (GM.varietyKind, RDF.type, OWL.FunctionalProperty) not in g


def test_variety_of_is_non_functional() -> None:
    g = _graph()
    assert (GM.varietyOf, RDF.type, OWL.ObjectProperty) in g
    assert (GM.varietyOf, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# LanguageState relator pattern
# --------------------------------------------------------------------------- #


def test_language_state_functional_role() -> None:
    g = _graph()
    assert (GM.stateLanguage, RDF.type, OWL.FunctionalProperty) in g


def test_language_state_non_functional_roles() -> None:
    g = _graph()
    for prop in (GM.stateStatusValue, GM.stateAuthority, GM.stateInterval):
        assert (prop, RDF.type, OWL.ObjectProperty) in g
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g


# --------------------------------------------------------------------------- #
# LanguageChangeEvent event pattern
# --------------------------------------------------------------------------- #


def test_change_event_is_activity() -> None:
    g = _graph()
    assert (GM.LanguageChangeEvent, RDF.type, OWL.Class) in g
    assert (GM.LanguageChangeEvent, RDFS.subClassOf, GM.Activity) in g


# --------------------------------------------------------------------------- #
# Bitemporal query competency - LanguageState
# --------------------------------------------------------------------------- #


def test_language_state_bitemporal_query() -> None:
    """LanguageState inherits Observation machinery, so the existing bitemporal
    query works out of the box."""
    data = Graph().parse(COVERAGE_FIXTURES / "language-varieties.ttl", format="turtle")
    query_text = (TEMPORAL_QUERY_DIR / "bitemporal.rq").read_text(encoding="utf-8")
    # Middle English spans 1150-1500; 1200 CE falls inside it.
    result = data.query(
        query_text,
        initBindings={
            "validAt": Literal("1200-01-01T00:00:00Z", datatype=XSD.dateTime),
            "asOf": Literal("2100-01-01T00:00:00Z", datatype=XSD.dateTime),
        },
    )
    states: set[Identifier] = set()
    for row in result:
        assert isinstance(row, ResultRow)
        states.add(row[0])
    assert URIRef("https://example.org/lang/middleEnglishState") in states, (
        f"Expected middleEnglishState in bitemporal results, got: {states}"
    )
