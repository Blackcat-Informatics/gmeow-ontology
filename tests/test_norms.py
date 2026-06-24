# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: MIT
"""The norms extension + rights graft (#351 / #352, EPIC #348) — RETAINED tests.

The asserted-TBox invariants have been migrated to the declarative slicetest
DSL in slices/extensions/norms/tests/structural.ttl (#867). What remains here
are tests that cannot be expressed as module-scoped SPARQL ASK cells:

  - test_graft_axioms_live_extension_side_only: loads slices/core/rights/module.ttl
    as a separate graph and checks no norms-extension IRIs appear there
    (cross-slice file-load check).
  - test_graft_preserves_core_trio_classhood: checks gmeow:Permission,
    gmeow:Prohibition, gmeow:Duty which are subjects in slices/core/rights/module.ttl,
    not the norms module; a scopeModule cell would silently miss them.
  - test_wellformed_norms_fixture_conforms: run_shacl() ExampleConformance check.
  - test_malformed_norms_fixture_is_flagged: run_shacl() with error-text assertions.
  - test_competency_deontic_modalities_query: external .rq file + result-set check.
  - test_competency_authority_order_query: external .rq file + result-set check.
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

FIXTURES = Path(__file__).parent / "fixtures" / "shapes"


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def _fixture(name: str) -> Graph:
    g = Graph()
    g.parse(FIXTURES / f"{name}.ttl", format="turtle")
    return g


# --------------------------------------------------------------------------- #
# The rights graft (#352) — cross-slice checks (not migratable to DSL)
# --------------------------------------------------------------------------- #


def test_graft_axioms_live_extension_side_only() -> None:
    """Zero core churn: the core rights module contains no reference to any
    norms-extension IRI — the graft is asserted in the norms module."""
    core_rights = Graph()
    core_rights.parse(
        Path(__file__).parent.parent / "slices" / "core" / "rights" / "module.ttl",
        format="turtle",
    )
    norms_terms = [GM.Norm, GM.deonticModality, GM.normIssuer, GM.normBearer]
    for term in norms_terms:
        assert not list(core_rights.triples((term, None, None))), f"{term} as subject"
        assert not list(core_rights.triples((None, term, None))), f"{term} as predicate"
        assert not list(core_rights.triples((None, None, term))), f"{term} as object"


def test_graft_preserves_core_trio_classhood() -> None:
    """The survey's do-not-touch list holds: the trio remain disjoint OWL
    classes with their gUFO grounding (the 5 class-dependent conflicts never
    fire)."""
    g = _graph()
    for cls in (GM.Permission, GM.Prohibition, GM.Duty):
        assert (cls, RDF.type, OWL.Class) in g
        assert (cls, RDFS.subClassOf, GM.Rule) in g


# --------------------------------------------------------------------------- #
# Closed-world SHACL shapes
# --------------------------------------------------------------------------- #


def test_wellformed_norms_fixture_conforms() -> None:
    result = run_shacl(_fixture("norms-wellformed"))
    assert result.ok, "\n".join(result.errors)


def test_malformed_norms_fixture_is_flagged() -> None:
    result = run_shacl(_fixture("norms-malformed"))
    assert not result.ok
    errors = "\n".join(result.errors)
    assert "no ought, only ought-according-to" in errors
    assert "never overrides itself" in errors
    assert "at least two gmeow:groupMember" in errors
    assert "exactly one gmeow:groupOperator" in errors
    # ex:mutexParam binds both a value and an entity — the XOR must fire.
    assert "binds exactly one of gmeow:parameterValue" in errors
    assert "higher and lower norms must be distinct" in errors
    # ex:flatTenure also omits its scope — both tenure violations must fire.
    assert "must be scoped to exactly one gmeow:precedenceScope" in errors
    assert "must be gmeow:deonticPermission" in errors
    assert "exactly one gmeow:evaluationVerdict" in errors
    assert "at most one gmeow:deonticModality" in errors


# --------------------------------------------------------------------------- #
# Competency
# --------------------------------------------------------------------------- #


def test_competency_deontic_modalities_query() -> None:
    query = (COMPETENCY_DIR / "norms-deontic-modalities.rq").read_text(encoding="utf-8")
    modalities: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        modalities.add(row[0])
    assert {
        GM.deonticObligation,
        GM.deonticProhibition,
        GM.deonticPermission,
        GM.deonticRecommendation,
    } <= modalities


def test_competency_authority_order_query() -> None:
    """absolute reaches conditional through strongerThan+ and nothing
    outranks absolute."""
    query = (COMPETENCY_DIR / "norms-authority-order.rq").read_text(encoding="utf-8")
    tops: set[object] = set()
    for row in _graph().query(query):
        assert isinstance(row, ResultRow)
        tops.add(row[0])
    assert tops == {GM.authorityAbsolute}
