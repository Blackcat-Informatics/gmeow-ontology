# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the fold-view layer (#267 narrow waist, PR 2).

A handcrafted fold exercises every helper precisely; the real committed
snapshot proves the layer works at production scale and that the fold-side
``public_text`` agrees with the rdflib path on real vocabulary.
"""

from __future__ import annotations

import pytest
from gts import Term, TermKind, Writer, read
from rdflib import RDFS, URIRef

from gmeow_tools.config import (
    GTS_GRAPH_ALIGNMENTS,
    GTS_GRAPH_STATEMENTS,
    NAMESPACE,
)
from gmeow_tools.gts_views import ALL, DEFAULT, FoldView, load_fold
from gmeow_tools.language_tags import public_text as rdflib_public_text

EX = "https://example.org/"
RDF_NS = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
XSD = "http://www.w3.org/2001/XMLSchema#"


@pytest.fixture(scope="module")
def view() -> FoldView:
    """A handcrafted fold: typed subjects, scoped quads, list, reifier."""
    w = Writer(profile="dist")
    terms = [
        Term(TermKind.IRI, EX + "cat"),  # 0
        Term(TermKind.IRI, RDF_NS + "type"),  # 1
        Term(TermKind.IRI, EX + "Animal"),  # 2
        Term(TermKind.IRI, str(RDFS.label)),  # 3
        Term(TermKind.LITERAL, "Cat", lang="en"),  # 4
        Term(TermKind.IRI, EX + "age"),  # 5
        Term(TermKind.IRI, XSD + "integer"),  # 6
        Term(TermKind.LITERAL, "7", datatype=6),  # 7
        Term(TermKind.IRI, EX + "graph"),  # 8 (named graph)
        Term(TermKind.IRI, EX + "dog"),  # 9
        # rdf list: (cat dog)
        Term(TermKind.BNODE, "l1"),  # 10
        Term(TermKind.BNODE, "l2"),  # 11
        Term(TermKind.IRI, RDF_NS + "first"),  # 12
        Term(TermKind.IRI, RDF_NS + "rest"),  # 13
        Term(TermKind.IRI, RDF_NS + "nil"),  # 14
        Term(TermKind.IRI, EX + "members"),  # 15
        Term(TermKind.IRI, EX + "r1"),  # 16 (reifier)
        Term(TermKind.IRI, EX + "confidence"),  # 17
        Term(TermKind.LITERAL, "0.9"),  # 18
    ]
    w.add_terms(terms)
    w.add_quads(
        [
            (0, 1, 2, None),  # cat a Animal
            (9, 1, 2, 8),  # dog a Animal (named graph)
            (0, 3, 4, None),  # cat label "Cat"@en
            (0, 5, 7, None),  # cat age 7
            (0, 15, 10, None),  # cat members (list head)
            (10, 12, 0, None),
            (10, 13, 11, None),
            (11, 12, 9, None),
            (11, 13, 14, None),
        ]
    )
    w.add_reifies({16: (0, 1, 2)})
    w.add_annot([(16, 17, 18)])
    return FoldView(read(w.to_bytes()))


def test_term_accessors(view: FoldView) -> None:
    cat = view.tid_of_iri(EX + "cat")
    assert cat is not None
    assert view.is_iri(cat) and not view.is_literal(cat)
    assert view.iri(cat) == EX + "cat"
    assert view.nq_token(cat) == f"<{EX}cat>"

    label = view.objects(cat, str(RDFS.label))[0]
    assert view.is_literal(label)
    assert view.lex(label) == "Cat"
    assert view.lang(label) == "en"
    assert view.datatype(label).endswith("langString")


def test_python_value_conversions(view: FoldView) -> None:
    cat = view.tid_of_iri(EX + "cat")
    assert cat is not None
    age = view.objects(cat, EX + "age")[0]
    assert view.python_value(age) == 7
    label = view.objects(cat, str(RDFS.label))[0]
    assert view.python_value(label) == {"value": "Cat", "lang": "en"}
    assert view.python_value(cat) == EX + "cat"  # no prefix known → IRI


def test_scoped_quad_access(view: FoldView) -> None:
    animals_default = view.subjects_by_type(EX + "Animal", DEFAULT)
    animals_named = view.subjects_by_type(EX + "Animal", EX + "graph")
    animals_all = view.subjects_by_type(EX + "Animal", ALL)
    cat, dog = view.tid_of_iri(EX + "cat"), view.tid_of_iri(EX + "dog")
    assert cat is not None and dog is not None
    assert animals_default == [cat]
    assert animals_named == [dog]
    assert sorted([cat, dog]) == animals_all
    assert len(list(view.quads(ALL))) == 9


def test_value_has_and_predicate_objects(view: FoldView) -> None:
    cat = view.tid_of_iri(EX + "cat")
    animal = view.tid_of_iri(EX + "Animal")
    assert cat is not None and animal is not None
    assert view.value(cat, RDF_NS + "type") == animal
    assert view.has(cat, RDF_NS + "type", animal)
    assert not view.has(cat, RDF_NS + "type", cat)
    pairs = view.predicate_objects(cat)
    assert len(pairs) == 4  # type, label, age, members


def test_rdf_list_walks(view: FoldView) -> None:
    cat = view.tid_of_iri(EX + "cat")
    assert cat is not None
    head = view.objects(cat, EX + "members")[0]
    items = view.rdf_list(head)
    assert [view.iri(t) for t in items] == [EX + "cat", EX + "dog"]


def test_statement_layer_passthrough(view: FoldView) -> None:
    [(rid, (s, _p, _o))] = list(view.reifiers().items())
    assert view.iri(rid) == EX + "r1"
    assert view.iri(s) == EX + "cat"
    [(arid, ap, av)] = view.annotations()
    assert arid == rid
    assert view.iri(ap) == EX + "confidence"
    assert view.lex(av) == "0.9"


def test_curie_uses_longest_prefix() -> None:
    v = FoldView(read(Writer().to_bytes()))
    assert v.curie(NAMESPACE + "Entity") == "gmeow:Entity"
    assert v.curie("https://nowhere.example/x") == "https://nowhere.example/x"


# --------------------------------------------------------------------------- #
# The real snapshot — production scale + rdflib-path agreement
# --------------------------------------------------------------------------- #


@pytest.fixture(scope="module")
def snapshot() -> FoldView:
    return load_fold()


def test_snapshot_scopes_are_populated(snapshot: FoldView) -> None:
    assert next(iter(snapshot.quads(DEFAULT)), None) is not None
    assert next(iter(snapshot.quads(GTS_GRAPH_STATEMENTS)), None) is not None
    assert next(iter(snapshot.quads(GTS_GRAPH_ALIGNMENTS)), None) is not None
    assert snapshot.reifiers() and snapshot.annotations()


def test_snapshot_tag_map_loads(snapshot: FoldView) -> None:
    tag_map = snapshot.tag_map()
    assert tag_map.get("x-gmeow-english") == "en"


def test_public_text_agrees_with_rdflib_path(snapshot: FoldView) -> None:
    """The shared rank_language key makes both paths select identically."""
    from gmeow_tools.graph import load_merged_graph

    g = load_merged_graph(include_imports=False)
    for local in ("Entity", "Agent", "Attestation", "GTSDocument"):
        subject = NAMESPACE + local
        tid = snapshot.tid_of_iri(subject)
        assert tid is not None, subject
        fold_text = snapshot.public_text(tid, str(RDFS.label))
        rdflib_text = rdflib_public_text(g, URIRef(subject), RDFS.label)
        assert fold_text == rdflib_text, subject


def test_snapshot_subjects_by_type_finds_classes(snapshot: FoldView) -> None:
    owl_class = "http://www.w3.org/2002/07/owl#Class"
    classes = snapshot.subjects_by_type(owl_class)
    assert len(classes) > 100
    names = {snapshot.iri(t) for t in classes}
    assert NAMESPACE + "GTSDocument" in names
