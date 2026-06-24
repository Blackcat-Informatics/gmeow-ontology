"""Reasoning-case orchestration tests for the axiomatized doctrine (#38).

The POSITIVE OWL 2 RL entailment tests that used to live here — derived ancestry,
location-through-containment, sub-organization transitivity, ProximityMeasurement
survival — were migrated to the native Rust reasoning harness
(``crates/logic/tests/ontology_entailments.rs``) under issue #896: the EL/DL/RL
chase now runs once, scoped and Docker-free, in Rust instead of rebuilding a
reasoned rdflib graph per pytest. See ``dsl/tests/MIGRATION-LEDGER.md``.

What remains here is the **reasoning-case orchestration** layer: the live
HermiT/ROBOT inconsistency and fixture-coherence cases run through a repo-local
script (``gmeow_tools.reasoning_cases``) so Make/CI can schedule Docker outside
pytest. These tests exercise that Python orchestration (call order, monkeypatched
reasoner) — an independent live Python impl with no Rust twin, so they are
retained-with-reason rather than migrated.
"""

from __future__ import annotations

import pytest
from gmeow_rdf.compat.rdflib import RDF, Graph

from gmeow_tools import reasoning_cases


def test_two_axis_case_expects_inconsistency(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The Docker lane fails if the two-axis bad individual is coherent."""
    calls: list[tuple[str, str]] = []

    def fake_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
        calls.append((name, reasoner))
        assert (
            reasoning_cases.EX.x,
            RDF.type,
            reasoning_cases.GMEOW.GenderIdentity,
        ) in extra
        assert (
            reasoning_cases.EX.x,
            RDF.type,
            reasoning_cases.GMEOW.GenderExpression,
        ) in extra
        return False

    monkeypatch.setattr(reasoning_cases, "_is_consistent", fake_consistent)
    reasoning_cases.assert_two_axis_individual_is_inconsistent()
    assert calls == [("two-axis", "hermit")]


def test_two_kind_case_expects_inconsistency(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """The Docker lane fails if the two-kind bad individual is coherent."""
    calls: list[tuple[str, str]] = []

    def fake_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
        calls.append((name, reasoner))
        assert (reasoning_cases.EX.y, RDF.type, reasoning_cases.GMEOW.Person) in extra
        assert (
            reasoning_cases.EX.y,
            RDF.type,
            reasoning_cases.GMEOW.Organization,
        ) in extra
        return False

    monkeypatch.setattr(reasoning_cases, "_is_consistent", fake_consistent)
    reasoning_cases.assert_two_kind_individual_is_inconsistent()
    assert calls == [("two-kind", "hermit")]


def test_reasoning_cases_run_all_order(monkeypatch: pytest.MonkeyPatch) -> None:
    """The repo script lane keeps the intended Docker cases in one order."""
    calls: list[str] = []
    monkeypatch.setattr(reasoning_cases, "merge_release", lambda *_a, **_kw: None)
    monkeypatch.setattr(
        reasoning_cases,
        "assert_two_axis_individual_is_inconsistent",
        lambda: calls.append("axis"),
    )
    monkeypatch.setattr(
        reasoning_cases,
        "assert_two_kind_individual_is_inconsistent",
        lambda: calls.append("kind"),
    )
    monkeypatch.setattr(
        reasoning_cases,
        "assert_worked_fixtures_stay_coherent_under_disjointness",
        lambda: calls.append("fixtures"),
    )

    completed = reasoning_cases.run_all()

    assert calls == ["axis", "kind", "fixtures"]
    assert completed == [
        "two-axis inconsistency",
        "two-kind inconsistency",
        "worked-fixture coherence",
    ]
