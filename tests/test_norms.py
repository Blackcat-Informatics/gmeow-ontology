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
  - test_competency_deontic_modalities_query: external .rq file + result-set check.
  - test_competency_authority_order_query: external .rq file + result-set check.

The two run_shacl fixture tests (wellformed/malformed) have been migrated to
crates/validate/tests/conformance_norms.rs (#867).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, Graph, Namespace
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR
from gmeow_tools.graph import load_merged_graph

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


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
