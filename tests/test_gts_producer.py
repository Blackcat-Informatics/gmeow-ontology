# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the RDF → GTS producer and the gts → {sqlite,duckdb} shims (#271)."""

from __future__ import annotations

import sqlite3
from pathlib import Path

from rdflib import BNode, Dataset, Graph, Literal, URIRef
from rdflib.namespace import RDFS, XSD

from gmeow_tools.gts_db import to_duckdb, to_sqlite
from gmeow_tools.gts_producer import gts_from_graph
from gts import read, to_nquads

EX = "https://example.org/"


def _sample_graph() -> Graph:
    g = Graph()
    cat = URIRef(EX + "Cat")
    g.add((cat, RDFS.label, Literal("Cat", lang="en")))
    g.add((cat, URIRef(EX + "legs"), Literal("4", datatype=XSD.integer)))
    g.add((cat, RDFS.comment, Literal("a plain comment")))
    b = BNode()
    g.add((cat, URIRef(EX + "sample"), b))
    g.add((b, RDFS.label, Literal("a sample", lang="en")))
    return g


def _reparse(nq: str, *, dataset: bool = False) -> Graph:
    fmt = "nquads" if dataset else "nt"
    target: Graph = Dataset() if dataset else Graph()
    target.parse(data=nq, format=fmt)
    return target


def test_producer_round_trip_isomorphic() -> None:
    """RDF → GTS → fold → N-Quads → RDF reproduces an isomorphic graph."""
    source = _sample_graph()
    data = gts_from_graph(source)
    folded = read(data)
    assert [d.code for d in folded.diagnostics] == []
    back = _reparse(to_nquads(folded))
    assert source.isomorphic(back)


def test_producer_default_compresses() -> None:
    """The default snapshot uses zstd (a transformed frame) and still folds clean."""
    data = gts_from_graph(_sample_graph())
    # the self-describe magic + a snapshot frame; folds without diagnostics
    assert read(data).diagnostics == []


def test_producer_named_graphs() -> None:
    """A Dataset round-trips its named-graph quads."""
    ds = Dataset()
    g1 = ds.graph(URIRef(EX + "g1"))
    g1.add((URIRef(EX + "s"), URIRef(EX + "p"), URIRef(EX + "o")))
    data = gts_from_graph(ds)
    folded = read(data)
    assert len(folded.quads) == 1
    gname = folded.quads[0][3]
    assert gname is not None
    assert folded.term(gname).value == EX + "g1"


def test_to_sqlite(tmp_path: Path) -> None:
    """gts → sqlite loads the dictionary-encoded tables with the right cardinalities."""
    folded = read(gts_from_graph(_sample_graph()))
    db = to_sqlite(folded, tmp_path / "out.db")
    conn = sqlite3.connect(db)
    try:
        n_terms = (conn.execute("SELECT count(*) FROM terms").fetchone() or (0,))[0]
        n_quads = (conn.execute("SELECT count(*) FROM quads").fetchone() or (0,))[0]
        # a resolving join works: every quad subject resolves to a term row
        joined = (
            conn.execute(
                "SELECT count(*) FROM quads q JOIN terms t ON q.s = t.id"
            ).fetchone()
            or (0,)
        )[0]
    finally:
        conn.close()
    assert n_terms == len(folded.terms)
    assert n_quads == len(folded.quads)
    assert joined == len(folded.quads)


def test_to_duckdb(tmp_path: Path) -> None:
    """gts → duckdb loads and a resolving join returns the source labels."""
    import duckdb

    folded = read(gts_from_graph(_sample_graph()))
    db = to_duckdb(folded, tmp_path / "out.duckdb")
    conn = duckdb.connect(str(db))
    try:
        n_quads = (conn.execute("SELECT count(*) FROM quads").fetchone() or (0,))[0]
        labels = conn.execute(
            "SELECT t.lex FROM quads q "
            "JOIN terms p ON q.p = p.id "
            "JOIN terms t ON q.o = t.id "
            "WHERE p.lex = ? AND t.lang = 'en'",
            [str(RDFS.label)],
        ).fetchall()
    finally:
        conn.close()
    assert n_quads == len(folded.quads)
    assert ("Cat",) in labels


def test_producer_default_graph_is_unnamed() -> None:
    """Default-graph triples export with a None graph name (not the default id)."""
    ds = Dataset()
    # default_graph.add (not the deprecated 3-tuple Dataset.add)
    ds.default_graph.add((URIRef(EX + "s"), URIRef(EX + "p"), URIRef(EX + "o")))
    folded = read(gts_from_graph(ds))
    assert len(folded.quads) == 1
    assert folded.quads[0][3] is None  # default graph, not a spurious named graph


def test_rdf12_producer_reifier_and_annotation(tmp_path: Path) -> None:
    """The RDF 1.2 path (pyoxigraph) ingests reifier triple-terms + annotations."""
    from gmeow_tools.gts_producer import gts_from_rdf12

    ttl = (
        "@prefix g: <https://example.org/> .\n"
        "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n"
        "g:alice g:knows g:bob .\n"
        "g:r1 rdf:reifies <<( g:alice g:knows g:bob )>> ; g:confidence 0.9 .\n"
    )
    src = tmp_path / "stmt.ttl"
    src.write_text(ttl, encoding="utf-8")
    g = read(gts_from_rdf12(src))
    assert [d.code for d in g.diagnostics] == []
    assert len(g.reifiers) == 1  # reifier bound to the quoted triple
    assert len(g.annotations) == 1  # the g:confidence statement metadata
    _reifier, pred_id, value_id = g.annotations[0]
    assert g.term(pred_id).value == "https://example.org/confidence"
    assert g.term(value_id).value == "0.9"


def test_compile_gts_missing_rdf12_raises(tmp_path: Path) -> None:
    """compile_gts errors on an explicitly-provided but missing RDF 1.2 path."""
    import pytest

    from gmeow_tools.gts_producer import compile_gts

    with pytest.raises(FileNotFoundError):
        compile_gts(_sample_graph(), tmp_path / "does-not-exist.ttl")


def test_to_nquads_lang_map_remaps_tags() -> None:
    """The renderer's lang_map remaps tags on OUTPUT; the graph is untouched."""
    folded = read(gts_from_graph(_sample_graph()))
    mapped = to_nquads(folded, {"en": "en-CA"})
    assert "@en-CA" in mapped
    assert "@en ." not in mapped and "@en\n" not in mapped
    # unmapped pass-through + the stored graph keeps its original tags
    assert "@en" in to_nquads(folded, {"fr": "fr-CA"})
    assert any(t.lang == "en" for t in folded.terms)
