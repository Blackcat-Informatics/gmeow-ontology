# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the context-aware graph-descent up-projection (#451)."""

from __future__ import annotations

from rdflib import RDF, Graph, Literal, URIRef
from rdflib.term import Node

from gmeow_tools.config import FIXTURES_DIR
from gmeow_tools.up_projection import up_project
from gmeow_tools.up_projection_descend import build_context, up_project_descend

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"


def _src(*triples: tuple[Node, Node, Node]) -> Graph:
    g = Graph()
    for triple in triples:
        g.add(triple)
    return g


def test_context_resolves_same_predicate_by_subject_type() -> None:
    """schema:about resolves to different gmeow terms by the subject's type —
    gmeow:depicts on a MediaObject, gmeow:isAbout on a document — the heart of
    'where in the graph you are'."""
    ctx = build_context()
    from gmeow_tools.up_projection_descend import _resolve

    media = _resolve(SCHEMA + "about", {GM + "MediaObject"}, ctx)
    doc = _resolve(SCHEMA + "about", {GM + "CreativeWork"}, ctx)
    assert media is not None and media.gmeow == GM + "depicts"
    assert doc is not None and doc.gmeow == GM + "isAbout"


def test_descent_corrects_the_context_free_misfire() -> None:
    """The floor claims schema:about → depicts for everything; on a CreativeWork
    that is wrong, and the descent corrects it to isAbout."""
    src = _src(
        (URIRef("https://ex.org/doc"), RDF.type, URIRef(SCHEMA + "CreativeWork")),
        (
            URIRef("https://ex.org/doc"),
            URIRef(SCHEMA + "about"),
            URIRef("https://ex.org/x"),
        ),
    )
    flat = up_project(src)
    desc = up_project_descend(src)
    qpred = URIRef(GM + "qPredicate")
    # floor misfires to depicts; descent claims isAbout
    assert (None, qpred, URIRef(GM + "depicts")) in flat.graph
    assert (None, qpred, URIRef(GM + "isAbout")) in desc.graph
    assert (None, qpred, URIRef(GM + "depicts")) not in desc.graph
    assert desc.context_resolved >= 1
    assert "schema:about" in desc.context_terms


def test_descent_rescues_a_floor_ambiguous_term() -> None:
    """A term the floor holds out as ambiguous (several candidates, no flat
    winner) is resolved by the subject's type — schema:alternateName on a Person
    → gmeow:hasName, not held out."""
    ada = URIRef("https://ex.org/ada")
    src = _src(
        (ada, RDF.type, URIRef(SCHEMA + "Person")),
        (ada, URIRef(SCHEMA + "alternateName"), Literal("Ada")),
    )
    flat = up_project(src)
    desc = up_project_descend(src)
    assert "schema:alternateName" in flat.ambiguous_terms  # floor holds it out
    assert "schema:alternateName" not in desc.ambiguous_terms  # descent resolves it
    assert (None, URIRef(GM + "qPredicate"), URIRef(GM + "hasName")) in desc.graph


def test_descent_defers_when_type_adds_no_signal() -> None:
    """With no subject type (or an incompatible one), the descent adds nothing and
    the edge falls through to the floor — identical output, never guessed."""
    x = URIRef("https://ex.org/x")
    src = _src((x, URIRef(SCHEMA + "alternateName"), Literal("anon")))  # untyped
    desc = up_project_descend(src)
    flat = up_project(src)
    assert desc.context_resolved == 0
    # same disposition as the floor: held ambiguous, nothing emitted
    assert "schema:alternateName" in desc.ambiguous_terms
    assert len(desc.graph) == len(flat.graph) == 0


def test_descent_never_regresses_facts_on_the_real_corpus() -> None:
    """On the vendored real snapshots the descent strictly improves the floor:
    facts never decrease and the ambiguous count drops (context rescues)."""
    exercised = 0
    for name in ("bii", "paudley"):
        path = FIXTURES_DIR / "external" / f"{name}.ttl"
        if not path.exists():
            continue
        exercised += 1
        src = Graph().parse(path, format="turtle")
        flat = up_project(src)
        desc = up_project_descend(src)
        assert desc.lifted >= flat.lifted, f"{name}: facts regressed"
        assert sum(desc.ambiguous_terms.values()) < sum(flat.ambiguous_terms.values())
        assert desc.context_resolved > 0
    assert exercised, "no corpus fixtures exercised — vacuous pass"


def test_descent_output_is_pure_gmeow() -> None:
    """The descent output, like the floor, is pure GMEOW — every predicate is a
    gmeow term or rdf:type, every rdf:type object a gmeow class."""
    import pytest

    path = FIXTURES_DIR / "external" / "paudley.ttl"
    if not path.exists():
        pytest.skip("corpus fixture absent")
    src = Graph().parse(path, format="turtle")
    desc = up_project_descend(src)
    for _s, p, o in desc.graph:
        assert str(p).startswith(GM) or p == RDF.type, f"non-gmeow predicate {p}"
        if p == RDF.type and isinstance(o, URIRef):
            assert str(o).startswith(GM), f"non-gmeow type {o}"


def test_descent_empty_graph_raises() -> None:
    """up_project_descend rejects an empty source graph with ValueError."""
    import pytest

    with pytest.raises(ValueError, match="empty"):
        up_project_descend(Graph())
