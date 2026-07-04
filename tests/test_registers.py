# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The registers & personas facility, in the norms slice.

All structural invariants and the same-norms competency question have been
migrated to the slice-resident declarative test-DSL:

  - slices/extensions/norms/tests/structural.ttl
  - slices/extensions/norms/tests/competency.ttl
  - slices/core/names/tests/structural.ttl (Register / NameRegister classhood)

What remains here are tests that cannot be expressed as module-scoped SPARQL
ASK/SELECT cells:

  - test_no_primary_persona_machinery: dynamic sweep over the merged graph
    checking that no primary/preferred persona/register term exists.
  - test_divergence_query_surfaces_legal_divergence: hybrid test that mutates
    a fixture, runs SHACL, and checks the divergence query reports the injected
    private-only norm.
"""

from __future__ import annotations

from pathlib import Path

from purrdf.compat.rdflib import Graph, Namespace
from purrdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
EX = Namespace("https://example.org/shapes/")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Dynamic / fixture residue
# --------------------------------------------------------------------------- #


def test_no_primary_persona_machinery() -> None:
    """No primaryPersona / preferredRegister selectors exist (Principle 9)."""
    g = _graph()
    banned = (
        "primarypersona",
        "preferredpersona",
        "primaryregister",
        "preferredregister",
    )
    offenders = [
        str(s)
        for s in set(g.subjects())
        if str(s).startswith(GMEOW)
        and "/" not in str(s)[len(GMEOW) :]
        and str(s)[len(GMEOW) :].lower().startswith(banned)
    ]
    assert offenders == []


def test_divergence_query_surfaces_legal_divergence() -> None:
    """Add a private-only norm: the query reports it (and SHACL still
    conforms — divergence is not a violation)."""
    g = _fixture("registers-wellformed")
    g.parse(
        data="""
        @prefix ex:    <https://example.org/shapes/> .
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        ex:playNorm a gmeow:Norm ;
            gmeow:deonticModality gmeow:deonticRecommendation ;
            gmeow:normIssuer ex:issuer .
        ex:privatePersona gmeow:expressesNorm ex:playNorm .
        """,
        format="turtle",
    )
    assert run_shacl(g).ok
    query_path = COMPETENCY_DIR / "registers-norm-divergence.rq"
    query = query_path.read_text(encoding="utf-8")
    rows = list(g.query(query))
    diverged = set()
    for row in rows:
        assert isinstance(row, ResultRow)
        diverged.add((row[1], row[2]))
    assert (EX.publicPersona, EX.playNorm) not in diverged
    assert (EX.privatePersona, EX.playNorm) in diverged
