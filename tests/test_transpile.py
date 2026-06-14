# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""Tests for the full transpile — consumer RDF → pure GMEOW → MAXIMAL (#448)."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import Graph, URIRef

from gmeow_tools.transpile import transpile

GM = "https://blackcatinformatics.ca/gmeow/"
SCHEMA = "https://schema.org/"

_SOURCE = f"""
@prefix schema: <{SCHEMA}> .
@prefix ex: <https://ex.org/> .

ex:ada a schema:Person ;
    schema:name "Ada Lovelace" ;
    schema:alternateName "Countess of Lovelace" ;
    schema:url <https://ex.org/ada-home> .
"""


@pytest.fixture
def source_file(tmp_path: Path) -> Path:
    path = tmp_path / "people.ttl"
    path.write_text(_SOURCE, encoding="utf-8")
    return path


def test_transpile_produces_maximal_family(source_file: Path, tmp_path: Path) -> None:
    """A consumer source is lifted to GMEOW and re-expressed maximally: the draft
    plus the .gts/.nq/.ttl/.jsonld family are written, and the maximal index.ttl
    carries both the GMEOW base and the re-projected consumer vocabulary."""
    out = tmp_path / "out"
    rep = transpile(source_file, out_dir=out, profiles=["schema-org"])

    assert rep.lifted > 0  # schema:name → gmeow:name etc.
    # the full file family + the pure-GMEOW draft are written
    written = {p.name for p in rep.transform.written}
    assert {"people.gts", "index.nq", "index.ttl", "index.jsonld"} <= written
    assert rep.draft_path.exists() and rep.draft_path.name == "people.gmeow.ttl"

    # the draft is pure GMEOW
    draft = Graph().parse(rep.draft_path, format="turtle")
    assert any(str(p).startswith(GM) for _s, p, _o in draft)
    assert not any(str(p).startswith(SCHEMA) for _s, p, _o in draft)

    # the maximal output has BOTH the GMEOW base and the schema.org re-projection
    index = Graph().parse(out / "index.ttl", format="turtle")
    preds = {str(p) for p in index.predicates()}
    assert any(p.startswith(GM) for p in preds), "gmeow base missing"
    assert any(p.startswith(SCHEMA) for p in preds), "schema.org re-projection missing"


def test_transpile_round_trips_a_clean_term(source_file: Path, tmp_path: Path) -> None:
    """schema:name round-trips through the whole pipeline: lifted to gmeow:name,
    then re-projected back to schema:name in the maximal output."""
    out = tmp_path / "out"
    transpile(source_file, out_dir=out, profiles=["schema-org"])
    index = Graph().parse(out / "index.ttl", format="turtle")
    # gmeow:name in the base (the lift), schema:name in the projection (round trip)
    assert (None, URIRef(GM + "name"), None) in index
    assert (None, URIRef(SCHEMA + "name"), None) in index


def test_transpile_descent_resolves_ambiguous_in_the_draft(
    source_file: Path, tmp_path: Path
) -> None:
    """The descent resolves schema:alternateName (floor-ambiguous) to gmeow:hasName
    by the Person type, so it reaches the draft as a provenance-stamped claim
    instead of being lost."""
    rep = transpile(source_file, out_dir=tmp_path / "out", profiles=["schema-org"])
    assert rep.context_resolved > 0
    draft = Graph().parse(rep.draft_path, format="turtle")
    # hasName is the claimed predicate (a StatementMetadata qPredicate), not a
    # bare edge — alternateName lifts as a claim, not a fact
    assert (None, URIRef(GM + "qPredicate"), URIRef(GM + "hasName")) in draft


def test_transpile_empty_lift_raises(tmp_path: Path) -> None:
    """A source whose terms don't lift to GMEOW yields an empty draft — surfaced
    as an error, not a silent empty publication."""
    src = tmp_path / "alien.ttl"
    src.write_text(
        "@prefix x: <https://nope.example/> . x:a x:unmapped x:b .", encoding="utf-8"
    )
    with pytest.raises(ValueError, match=r"nothing lifted|empty"):
        transpile(src, out_dir=tmp_path / "out", profiles=["schema-org"])


def test_transform_graph_matches_transform_file(tmp_path: Path) -> None:
    """The refactor is behaviour-preserving: transform_graph over a parsed graph
    writes the same base+derived triple count as transform over the file."""
    from gmeow_tools.transform import transform, transform_graph

    abox = tmp_path / "g.ttl"
    abox.write_text(
        f"@prefix gmeow: <{GM}> . @prefix ex: <https://ex.org/> .\n"
        f'ex:x a gmeow:Person ; gmeow:fullName "Ada" .',
        encoding="utf-8",
    )
    from_file = transform(abox, out_dir=tmp_path / "a", profiles=["schema-org"])
    graph = Graph()
    graph.parse(abox, format="turtle")
    from_graph = transform_graph(
        graph, "g", out_dir=tmp_path / "b", profiles=["schema-org"]
    )
    a = Graph().parse(tmp_path / "a" / "index.ttl", format="turtle")
    b = Graph().parse(tmp_path / "b" / "index.ttl", format="turtle")
    assert len(a) == len(b)
    assert from_file.asserted == from_graph.asserted
    assert from_file.projected == from_graph.projected
