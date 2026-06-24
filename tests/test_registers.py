# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The registers & personas facility (#355, EPIC #348), in the norms slice.

Same agent, same norms, different expression by context. Persona is a RELATOR
(the NameUsage idiom — personas need their own identity for style guides and
tenure; gUFO roles classify, they don't reify), bearing registers from the
names-core gmeow:Register spine (NameRegister ⊑ Register: address and
expression share one vocabulary). The voice payload is byte-perfect: style
guides attach content-digested Documents carrying hasAboutness ENACTS, never
pseudo-quantified style triples. The same-norms invariant is a competency
QUERY, not a shape — divergence is legal (P9); the query makes it visible.
Register-switching is not deception (#212 boundary, documented not
axiomatized).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph
from tests._graph_nt import run_shacl

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)
GUFO = Namespace("http://purl.org/nemo/gufo#")
LOGIC = Namespace("https://blackcatinformatics.ca/logic/")
EX = Namespace("https://example.org/shapes/")

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# Structural invariants
# --------------------------------------------------------------------------- #


def test_register_spine_lives_in_names_core() -> None:
    """gmeow:Register is the names-core umbrella; NameRegister specializes it
    so address and expression draw from one vocabulary (the dependency
    direction requires the umbrella below its consumers)."""
    g = _graph()
    assert (GM.Register, RDF.type, LOGIC.AbstractIndividualType) in g
    assert (GM.Register, RDFS.subClassOf, LOGIC.QualityValue) in g
    assert (GM.NameRegister, RDFS.subClassOf, GM.Register) in g
    # A names-core seed and a persona-facing seed are both Registers.
    assert (GM.registerFormal, RDF.type, GM.NameRegister) in g
    assert (GM.registerPublic, RDF.type, GM.Register) in g


def test_persona_is_a_relator_with_one_bearer() -> None:
    """The grounding decision: relator, not gufo:Role class — personas need
    their own identity (style guides, tenure); roles classify, they don't
    reify."""
    g = _graph()
    assert (GM.Persona, RDF.type, GUFO.Kind) in g
    assert (GM.Persona, RDFS.subClassOf, GUFO.Relator) in g
    assert (GM.personaBearer, RDF.type, OWL.FunctionalProperty) in g
    assert (GM.personaBearer, RDFS.range, GM.Agent) in g


def test_expression_machinery_is_open_and_plural() -> None:
    g = _graph()
    for prop in (GM.personaRegister, GM.activatedIn, GM.expressesNorm):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop
    assert (GM.personaRegister, RDFS.range, GM.Register) in g
    assert (GM.expressesNorm, RDFS.range, GM.Norm) in g
    # Activation context is range-open: a Condition or a situation type.
    assert g.value(GM.activatedIn, RDFS.range) is None


def test_style_guide_voice_doctrine() -> None:
    g = _graph()
    assert (GM.StyleGuide, RDFS.subClassOf, GM.InformationObject) in g
    assert (GM.voiceExemplifiedBy, RDFS.range, GM.Document) in g
    assert g.value(GM.styleGuideFor, RDFS.range) is None
    for prop in (GM.styleGuideFor, GM.voiceExemplifiedBy):
        assert (prop, RDF.type, OWL.FunctionalProperty) not in g, prop


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


# --------------------------------------------------------------------------- #
# The same-norms invariant — a query, not a shape
# --------------------------------------------------------------------------- #


def test_same_norms_invariant_holds_on_wellformed_fixture() -> None:
    """Both personas express the tier-1 norm → the divergence query returns
    no rows. Divergence would be LEGAL — the query makes it visible."""
    query_path = COMPETENCY_DIR / "registers-norm-divergence.rq"
    query = query_path.read_text(encoding="utf-8")
    rows = list(_fixture("registers-wellformed").query(query))
    assert rows == []


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
