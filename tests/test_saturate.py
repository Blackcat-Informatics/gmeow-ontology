# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Equivalence saturation E(G) (#34): strong-only, lint-gated, suppression-safe.

The acceptance keystones: a ``skos:closeMatch``-only term NEVER materializes
(it is a hint, not a fact); a lint-denied cell emits nothing; a suppressed
node contributes no derived triple while its control twin does (non-vacuous);
every derived triple carries its ``gmeow:mappedFrom`` audit annotation.
"""

from __future__ import annotations

import pytest
from gmeow_rdf.compat.rdflib import OWL, RDF, Graph, Literal, URIRef

from gmeow_tools.config import FIXTURES_DIR, NAMESPACE
from gmeow_tools.graph import load_merged_graph
from gmeow_tools.mapping_compile import _default_suppression_vocab
from gmeow_tools.saturate import (
    SAME_AS_MIRROR_RULE,
    Cell,
    DerivedTriple,
    load_cells,
    saturate,
)

pytestmark = pytest.mark.maintainer

_GM = NAMESPACE
_SCHEMA_PERSON = URIRef("https://schema.org/Person")
_SCHEMA_DATASET = URIRef("https://schema.org/Dataset")
_SCHEMA_SAME_AS = URIRef("https://schema.org/sameAs")
_MAPPED_FROM = URIRef(_GM + "mappedFrom")
_CONFIDENCE = URIRef(_GM + "confidence")
_EX = "https://example.org/sat/"


@pytest.fixture(scope="module")
def onto() -> Graph:
    return load_merged_graph(include_imports=False)


@pytest.fixture(scope="module")
def cells() -> list[Cell]:
    return load_cells()


def _saturate(
    abox: Graph, onto: Graph, cells: list[Cell], **kw: object
) -> list[DerivedTriple]:
    return saturate(
        abox,
        onto=onto,
        cells=cells,
        denied=kw.get("denied", set()),  # type: ignore[arg-type]
        vocab=_default_suppression_vocab(),
    )


def _person_abox() -> Graph:
    g = Graph()
    g.add((URIRef(_EX + "me"), RDF.type, URIRef(_GM + "Person")))
    return g


def test_strong_class_edges_materialize(onto: Graph, cells: list[Cell]) -> None:
    """gmeow:Person saturates to every strong external equivalent at once."""
    derived = _saturate(_person_abox(), onto, cells)
    types = {row.triple[2] for row in derived if row.triple[1] == RDF.type}
    assert _SCHEMA_PERSON in types
    assert URIRef("http://xmlns.com/foaf/0.1/Person") in types


def test_close_match_never_materializes(onto: Graph, cells: list[Cell]) -> None:
    """gmeow:Corpus has ONLY closeMatch cells — a hint must not become a fact."""
    g = Graph()
    g.add((URIRef(_EX + "corpus"), RDF.type, URIRef(_GM + "Corpus")))
    derived = _saturate(g, onto, cells)
    assert derived == []
    # Non-vacuous: the closeMatch cells genuinely exist in the authoring DSL.
    corpus_cells = [c for c in cells if c.subject == URIRef(_GM + "Corpus")]
    assert any(c.predicate_curie == "skos:closeMatch" for c in corpus_cells)
    assert _SCHEMA_DATASET in {c.obj for c in corpus_cells}


def test_denied_cell_is_refused(onto: Graph, cells: list[Cell]) -> None:
    """A lint-ERROR row emits nothing; sibling edges are untouched."""
    denied = {("gmeow:Person", "owl:equivalentClass", "schema:Person")}
    derived = _saturate(_person_abox(), onto, cells, denied=denied)
    types = {row.triple[2] for row in derived if row.triple[1] == RDF.type}
    assert _SCHEMA_PERSON not in types
    assert URIRef("http://xmlns.com/foaf/0.1/Person") in types  # control


def test_suppressed_nodes_contribute_nothing(onto: Graph, cells: list[Cell]) -> None:
    """The #282 canary: a displayable-false node never saturates; its twin does."""
    g = Graph()
    g.parse(FIXTURES_DIR / "suppression-canary.ttl", format="turtle")
    derived = _saturate(g, onto, cells)
    subjects = {row.triple[0] for row in derived}
    assert URIRef("https://example.org/canary/suppressedPerson") not in subjects
    assert URIRef("https://example.org/canary/canaryPerson") in subjects  # control


def test_same_as_mirror(onto: Graph, cells: list[Cell]) -> None:
    """owl:sameAs external links mirror to schema:sameAs, rule-attributed."""
    g = _person_abox()
    qid = URIRef("http://www.wikidata.org/entity/Q42")
    g.add((URIRef(_EX + "me"), OWL.sameAs, qid))
    derived = _saturate(g, onto, cells)
    mirrors = [row for row in derived if row.triple[1] == _SCHEMA_SAME_AS]
    assert len(mirrors) == 1
    assert mirrors[0].triple == (URIRef(_EX + "me"), _SCHEMA_SAME_AS, qid)
    assert (_MAPPED_FROM, SAME_AS_MIRROR_RULE) in mirrors[0].annotations


def test_provenance_annotations_carry_cell_and_confidence(
    onto: Graph, cells: list[Cell]
) -> None:
    """Every derived triple is mappedFrom-attributed to its authored cell."""
    derived = _saturate(_person_abox(), onto, cells)
    schema_person = next(row for row in derived if row.triple[2] == _SCHEMA_PERSON)
    mapped = [v for p, v in schema_person.annotations if p == _MAPPED_FROM]
    assert mapped, "no mappedFrom annotation"
    assert str(mapped[0]).startswith(_GM)  # the authored TermEquivalence IRI
    confidences = [v for p, v in schema_person.annotations if p == _CONFIDENCE]
    assert confidences and isinstance(confidences[0], Literal)
    assert 0.0 <= float(confidences[0]) <= 1.0


def test_already_asserted_triples_are_skipped(onto: Graph, cells: list[Cell]) -> None:
    """G is canonical — a triple already in the A-Box gets no reifier."""
    g = _person_abox()
    g.add((URIRef(_EX + "me"), RDF.type, _SCHEMA_PERSON))
    derived = _saturate(g, onto, cells)
    assert (URIRef(_EX + "me"), RDF.type, _SCHEMA_PERSON) not in {
        row.triple for row in derived
    }


def test_determinism(onto: Graph, cells: list[Cell]) -> None:
    """Two runs over the same A-Box derive identical rows (incl. reifiers)."""
    g = Graph()
    g.parse(FIXTURES_DIR / "rights.ttl", format="turtle")
    assert _saturate(g, onto, cells) == _saturate(g, onto, cells)
