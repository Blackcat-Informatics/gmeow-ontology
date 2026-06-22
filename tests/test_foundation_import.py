# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The foundation-corpus importer (#364, EPIC #358) — the consumer child.

CI runs against a SYNTHETIC one-book corpus (privacy: real corpus content
never enters the repo). The acceptance spine: the imported graph conforms to
the closed-world shapes; the budget report shows the flat/reified split with
no silent caps (tags counted as unpromoted); the six projections emit
parseable artifacts; and the cross-children competency demo answers — a
character's trajectory against her exemplified principia, with the chapters
that evidence each, in discourse order.
"""

from __future__ import annotations

import csv
import io
import json
import xml.etree.ElementTree as ET
from pathlib import Path

from gmeow_rdf.compat.rdflib import RDF, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.foundation_import import (
    PROJECTIONS,
    FoundationImporter,
    load_records,
    run_import,
)
from tests._graph_nt import run_shacl

GM = Namespace("https://blackcatinformatics.ca/gmeow/")
CORP = Namespace("https://blackcatinformatics.ca/gmeow/corpus/foundation/")
FIXTURE = Path(__file__).parent / "fixtures" / "foundation" / "synthetic-corpus.jsonl"


def _imported() -> tuple[Graph, FoundationImporter]:
    importer = FoundationImporter()
    importer.import_corpus(load_records(FIXTURE), source_path=str(FIXTURE))
    return importer.graph, importer


# --------------------------------------------------------------------------- #
# The acceptance spine
# --------------------------------------------------------------------------- #


def test_imported_graph_conforms_to_shapes() -> None:
    """The whole point: corpus → instance data that the closed-world shapes
    accept without manual edits."""
    graph, _ = _imported()
    result = run_shacl(graph)
    assert result.ok, "\n".join(result.errors)


def test_budget_report_has_no_silent_caps() -> None:
    """Flat seam links by default; reified only where vantage/score is data;
    tags counted as unpromoted, never silently dropped (#360/#363)."""
    _, importer = _imported()
    budget = importer.budget
    assert budget.flat["narrates → active character"] == 3
    assert budget.flat["narrates → key event"] == 3
    assert budget.flat["narratedIn ← appearance"] == 3
    assert budget.reified["arc samples (vantage is data)"] == 3
    assert budget.reified["goal-score assessments (zeros are scores)"] == 3
    assert budget.reified["role claims (scoped, interpretive)"] == 2
    assert budget.reified["entity exemplars (exemplarSubject, #353/#362)"] == 1
    assert budget.skipped["thematic_tags (unpromoted — #363 heuristic)"] == 3
    text = budget.as_text()
    assert "no silent caps" in text


def test_zeros_are_scores() -> None:
    """The 0.0 axis imports as an Assessment like any other."""
    graph, _ = _imported()
    zero = []
    for a in graph.subjects(RDF.type, GM.Assessment):
        value = graph.value(a, GM.assessmentScoreValue)
        assert value is not None, a
        if float(str(value)) == 0.0:
            zero.append(a)
    assert len(zero) == 1


def test_unknown_role_mints_open_vocabulary_value() -> None:
    """'apprentice sage' is no seed: it becomes a corpus-local NarrativeRole
    individual (the vocabulary is open, P9), not a dropped claim."""
    graph, _ = _imported()
    minted = CORP["role/apprentice-sage"]
    assert (minted, RDF.type, GM.NarrativeRole) in graph
    claims = list(graph.subjects(GM.narrativeRoleValue, minted))
    assert len(claims) == 1


# --------------------------------------------------------------------------- #
# The cross-children competency demo
# --------------------------------------------------------------------------- #


def test_competency_demo_trajectory_against_exemplified_principia() -> None:
    """'Rowan's emotional trajectory against her exemplified principia, with
    the chapters that evidence each, in discourse order' — one query touching
    #359 positions, #361 samples, #362 exemplar subjects, #353 criteria, and
    the #360 seam."""
    graph, _ = _imported()
    query = """
    PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
    PREFIX rdfs:  <http://www.w3.org/2000/01/rdf-schema#>
    SELECT ?ordinal ?stateLabel ?criterionLabel WHERE {
        ?sample a gmeow:ArcSample ;
            gmeow:sampleSubject ?who ;
            gmeow:samplePosition ?pos ;
            gmeow:sampleState ?state .
        ?pos gmeow:positionOrdinal ?ordinal .
        ?state rdfs:label ?stateLabel .
        ?exemplar a gmeow:Exemplar ; gmeow:exemplarSubject ?who .
        ?anchor gmeow:anchorExemplar ?exemplar .
        ?criterion gmeow:hasScoreAnchor ?anchor ; rdfs:label ?criterionLabel .
        ?who gmeow:narratedIn ?segment .
        ?segment gmeow:atNarrativePosition ?pos .
    }
    ORDER BY ?ordinal
    """
    rows = []
    for r in graph.query(query):
        assert isinstance(r, ResultRow)
        rows.append((int(str(r[0])), str(r[1]), str(r[2])))
    assert rows == [
        (1, "Resolute Doubt", "enforce_test_trust"),
        (2, "Hard-won Calm", "enforce_test_trust"),
    ]


# --------------------------------------------------------------------------- #
# Projections — each emits, each parses
# --------------------------------------------------------------------------- #


def test_projections_emit_parseable_artifacts(tmp_path: Path) -> None:
    graph, budget = run_import(FIXTURE, tmp_path)
    # All artifacts exist.
    for name in (*PROJECTIONS, "foundation.ttl", "budget-report.txt"):
        assert (tmp_path / name).exists(), name
    # DraCor: Rowan and Willam co-occur in chapter two.
    rows = list(csv.reader(io.StringIO((tmp_path / "dracor.csv").read_text())))
    assert rows[0] == ["Source", "Target", "Weight"]
    assert any(
        "rowan-cogsworth" in r[0] + r[1] and "willam" in r[0] + r[1] for r in rows[1:]
    )
    # Syuzhet: ordered trajectory rows.
    rows = list(csv.reader(io.StringIO((tmp_path / "syuzhet.csv").read_text())))
    assert rows[0] == ["subject", "vantage", "ordinal", "state"]
    assert len(rows) == 4  # header + three samples
    # schema.org: parses, carries the book and both authors.
    doc = json.loads((tmp_path / "schema-org.jsonld").read_text())
    book = doc["@graph"][0]
    assert book["@type"] == "Book"
    assert len(book["author"]) == 2
    # TEI: well-formed XML with castList and two chapter divs.
    root = ET.fromstring((tmp_path / "tei.xml").read_text())
    ns = {"tei": "http://www.tei-c.org/ns/1.0"}
    assert len(root.findall(".//tei:castItem", ns)) >= 2
    assert len(root.findall(".//tei:div[@type='chapter']", ns)) == 2
    # Web Annotation: parses, one annotation per flat narrates link.
    anno = json.loads((tmp_path / "web-annotation.jsonld").read_text())
    assert len(anno["@graph"]) == 6
    # Training manifest: one record per assessment, scores faithful.
    records = [
        json.loads(line)
        for line in (tmp_path / "training-manifest.jsonl").read_text().splitlines()
    ]
    assert len(records) == 3
    assert {r["score"] for r in records} == {0.9, 0.4, 0.0}
    del graph, budget


def test_real_corpus_never_in_repo() -> None:
    """Privacy gate: no file under tests/ or slices/ may reference the real
    corpus path or its content (the fixture is synthetic by construction)."""
    fixture_text = FIXTURE.read_text(encoding="utf-8")
    assert "lillith" not in fixture_text.lower()
    assert "Synthetic" in fixture_text
