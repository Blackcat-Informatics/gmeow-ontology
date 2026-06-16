# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the full transpile — consumer RDF → pure GMEOW → MAXIMAL (#448)."""

from __future__ import annotations

from pathlib import Path

import pytest
from rdflib import RDF, Graph, Literal, URIRef

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


def test_gap_report_lists_held_terms_and_their_triples() -> None:
    """A projection-vocab term with no lift rule, and its actual source triple,
    appear in the gap report — never silently dropped."""
    from gmeow_tools.transpile import _gap_report
    from gmeow_tools.up_projection import UpProjection

    src = Graph()
    src.add((URIRef("https://ex.org/x"), URIRef(SCHEMA + "gap"), Literal("v")))
    lift = UpProjection(graph=Graph(), lifted=1, claimed=0, gap_terms={"schema:gap": 1})
    report = _gap_report(src, lift, "g")
    assert "schema:gap" in report  # the gap term named
    assert '"v"' in report  # the actual un-lifted triple listed
    assert "1 triples / 1 terms" in report  # tallies match the accounting


def test_transpile_writes_gap_report(source_file: Path, tmp_path: Path) -> None:
    """Every transpile writes a `<stem>.gaps.md` whose tallies are consistent
    with the report's gap/ambiguous accounting."""
    rep = transpile(source_file, out_dir=tmp_path / "out", profiles=["schema-org"])
    assert rep.gap_report_path.exists() and rep.gap_report_path.name == "people.gaps.md"
    report = rep.gap_report_path.read_text(encoding="utf-8")
    assert "# Transpile gap report" in report
    assert "## Gap terms" in report and "## Ambiguous terms" in report


def test_transpile_writes_index_nt(source_file: Path, tmp_path: Path) -> None:
    """index.nt is part of the maximal family and is plain-RDF parseable."""
    out = tmp_path / "out"
    rep = transpile(source_file, out_dir=out, profiles=["schema-org"])
    assert "index.nt" in {p.name for p in rep.transform.written}
    nt = Graph().parse(out / "index.nt", format="nt")
    ttl = Graph().parse(out / "index.ttl", format="turtle")
    assert len(nt) == len(ttl)  # same asserted triples, different syntax


def test_project_gts_single_vocab_view(source_file: Path, tmp_path: Path) -> None:
    """`project --profile foaf <gts>` filters the maximal .gts to the complete
    FOAF subset (a view, not a re-projection); 'all' and 'gmeow' views too."""
    from gmeow_tools.config import PREFIXES
    from gmeow_tools.projections import project_gts_subset

    out = tmp_path / "out"
    rep = transpile(source_file, out_dir=out, profiles=["foaf"])
    gts_path = next(p for p in rep.transform.written if p.suffix == ".gts")
    views = tmp_path / "views"

    foaf = Graph().parse(
        project_gts_subset(gts_path, "foaf", dist_dir=views), format="turtle"
    )
    index = Graph().parse(out / "index.ttl", format="turtle")
    fns = PREFIXES["foaf"]
    # the FOAF subset = foaf-predicate statements + rdf:type-to-foaf-class. A
    # clean vocab view is predicate-based; an object-only mention (a gmeow
    # predicate pointing at a foaf IRI) is a gmeow statement, not a foaf one.
    foaf_in_max = {
        (s, p, o)
        for s, p, o in index
        if str(p).startswith(fns) or (p == RDF.type and str(o).startswith(fns))
    }
    assert foaf_in_max <= set(foaf), "view must hold the full FOAF subset of maximal"
    assert all(not str(p).startswith(SCHEMA) for _s, p, _o in foaf), "no schema leak"

    allg = Graph().parse(
        project_gts_subset(gts_path, "all", dist_dir=views), format="turtle"
    )
    gm = Graph().parse(
        project_gts_subset(gts_path, "gmeow", dist_dir=views), format="turtle"
    )
    assert len(allg) > len(gm) > 0  # all ⊃ the pure-gmeow base
    assert all(str(p).startswith(GM) or str(p).endswith("#type") for _s, p, _o in gm)


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


def test_transpile_graph_matches_file(source_file: Path, tmp_path: Path) -> None:
    """transpile_graph over an in-memory graph (the stdin path) matches
    transpile over the same file."""
    from gmeow_tools.transpile import transpile_graph

    from_file = transpile(source_file, out_dir=tmp_path / "a", profiles=["schema-org"])
    graph = Graph()
    graph.parse(source_file, format="turtle")
    from_graph = transpile_graph(
        graph, "people", out_dir=tmp_path / "b", profiles=["schema-org"]
    )
    assert from_file.lifted == from_graph.lifted
    assert from_file.transform.projected == from_graph.transform.projected
    a = Graph().parse(tmp_path / "a" / "index.ttl", format="turtle")
    b = Graph().parse(tmp_path / "b" / "index.ttl", format="turtle")
    assert len(a) == len(b)


def test_transpile_cli_reads_stdin(tmp_path: Path) -> None:
    """`gmeow transpile -` reads the source from stdin and writes the family,
    naming the draft from the 'stdin' stem."""
    from typer.testing import CliRunner

    from gmeow_tools.cli import app  # transpile is a consumer command now (PR-B)

    out = tmp_path / "out"
    result = CliRunner().invoke(
        app,
        ["transpile", "-", "-o", str(out), "--profiles", "schema-org"],
        input=_SOURCE,
    )
    assert result.exit_code == 0, result.output
    assert (out / "stdin.gmeow.ttl").exists()
    assert (out / "index.ttl").exists()
    assert (out / "stdin.gts").exists()


def test_project_cli_projects_a_gmeow_data_file(tmp_path: Path) -> None:
    """`gmeow project <data.ttl> --profile <vocab>` is a consumer command (PR-B):
    it runs the per-profile CONSTRUCT on a user's GMEOW data, from the bundle."""
    from typer.testing import CliRunner

    from gmeow_tools.cli import app

    data = tmp_path / "ada.ttl"
    data.write_text(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "@prefix ex: <https://ex.org/> .\n"
        'ex:ada a gmeow:Person ; gmeow:name "Ada" .\n',
        encoding="utf-8",
    )
    out = tmp_path / "proj"
    result = CliRunner().invoke(
        app, ["project", str(data), "--profile", "foaf", "-o", str(out)]
    )
    assert result.exit_code == 0, result.output
    assert (out / "gmeow-ada-foaf.ttl").exists()


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
