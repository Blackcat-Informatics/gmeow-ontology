# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for gmeow_logic.query backward-goal resolution (issue #504, Task 5).

Covers the four conformance cases under conformance/logic/cases/profiles/:
  1. goal-recursive-ancestor — tabled recursion (AC-1); bindings {b,c,d}, status ok.
  2. goal-pattern-fast       — no rules, SPARQL fast path; bindings {b,c}, status ok.
  3. goal-budget-trip        — max_answers=2 budget cap; 2 bindings, status partial.
  4. goal-procedural-cut     — cut under ProceduralPrologProfile (AC-2); 1 binding, ok.

Also verifies that the same cut program under PositiveHornProfile raises ValueError
(profile gate enforcement — AC-2 negative path).

The native extension is required. Missing ``gmeow_logic`` is a test-environment
failure, not a skip.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from tests._required_native import require_gmeow_logic

gmeow_logic = require_gmeow_logic()

# --------------------------------------------------------------------------- #
# Repository paths
# --------------------------------------------------------------------------- #

_REPO_ROOT = Path(__file__).resolve().parents[1]
_PROFILES_CASES = _REPO_ROOT / "conformance" / "logic" / "cases" / "profiles"

_CASE_ANCESTOR = _PROFILES_CASES / "goal-recursive-ancestor"
_CASE_FAST = _PROFILES_CASES / "goal-pattern-fast"
_CASE_BUDGET = _PROFILES_CASES / "goal-budget-trip"
_CASE_CUT = _PROFILES_CASES / "goal-procedural-cut"


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _load_nq(case_dir: Path) -> str:
    """Read input.nq from a case directory."""
    return (case_dir / "input.nq").read_text(encoding="utf-8")


def _load_query(case_dir: Path, stem: str) -> str:
    """Read a single .logic query file from queries/<stem>.logic."""
    return (case_dir / "queries" / f"{stem}.logic").read_text(encoding="utf-8")


def _load_golden(case_dir: Path, stem: str) -> dict[str, object]:
    """Read the committed golden expected/answers/<stem>.json."""
    data: dict[str, object] = json.loads(
        (case_dir / "expected" / "answers" / f"{stem}.json").read_text(encoding="utf-8")
    )
    return data


def _profile(case_dir: Path) -> str:
    """Read the reasoning-contract preset string from profile.json (#767)."""
    data = json.loads((case_dir / "profile.json").read_text(encoding="utf-8"))
    return str(data["reasoning_contract"]["preset"])


def _max_answers(case_dir: Path) -> int | None:
    """Read budget_params.max_answers from profile.json, or None."""
    data = json.loads((case_dir / "profile.json").read_text(encoding="utf-8"))
    bp = data.get("budget_params")
    if isinstance(bp, dict):
        v = bp.get("max_answers")
        return int(v) if v is not None else None
    return None


def _assert_canonical_equal(
    actual: dict[str, object], expected: dict[str, object], label: str
) -> None:
    """Assert two dicts are equal in canonical JSON form."""
    actual_canon = json.dumps(actual, sort_keys=True, ensure_ascii=False)
    expected_canon = json.dumps(expected, sort_keys=True, ensure_ascii=False)
    assert actual_canon == expected_canon, (
        f"{label}: canonical JSON mismatch\n"
        f"  actual:   {actual_canon}\n"
        f"  expected: {expected_canon}"
    )


# --------------------------------------------------------------------------- #
# Case 1: goal-recursive-ancestor (tabled recursion — AC-1)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(
    not _CASE_ANCESTOR.is_dir(), reason="goal-recursive-ancestor case not present"
)
class TestGoalRecursiveAncestor:
    """AC-1: recursive ancestor query over a parentOf chain a→b→c→d."""

    def test_result_matches_golden(self) -> None:
        """Full result equals committed golden: bindings {b,c,d}, status ok."""
        nq = _load_nq(_CASE_ANCESTOR)
        query = _load_query(_CASE_ANCESTOR, "ancestor")
        profile = _profile(_CASE_ANCESTOR)
        golden = _load_golden(_CASE_ANCESTOR, "ancestor")

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        _assert_canonical_equal(result, golden, "goal-recursive-ancestor/ancestor")

    def test_status_ok(self) -> None:
        """Status must be 'ok' (full resolution, no budget cap)."""
        nq = _load_nq(_CASE_ANCESTOR)
        query = _load_query(_CASE_ANCESTOR, "ancestor")
        profile = _profile(_CASE_ANCESTOR)

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        assert result["status"] == "ok", (
            f"Expected status='ok', got {result['status']!r}"
        )

    def test_three_bindings_returned(self) -> None:
        """Must return exactly 3 bindings: b, c, d."""
        nq = _load_nq(_CASE_ANCESTOR)
        query = _load_query(_CASE_ANCESTOR, "ancestor")
        profile = _profile(_CASE_ANCESTOR)

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        bindings = result["bindings"]
        assert len(bindings) == 3, (
            f"Expected 3 bindings, got {len(bindings)}: {bindings}"
        )

    def test_all_expected_values_present(self) -> None:
        """Bindings must include b, c, and d as values of Y."""
        nq = _load_nq(_CASE_ANCESTOR)
        query = _load_query(_CASE_ANCESTOR, "ancestor")
        profile = _profile(_CASE_ANCESTOR)
        base = "https://example.org/profiles/goal-recursive-ancestor/"

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        y_values = {b["Y"] for b in result["bindings"]}
        for expected_val in [f"<{base}b>", f"<{base}c>", f"<{base}d>"]:
            assert expected_val in y_values, (
                f"Expected binding Y={expected_val!r} not found in {y_values!r}"
            )


# --------------------------------------------------------------------------- #
# Case 2: goal-pattern-fast (SPARQL fast path)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(
    not _CASE_FAST.is_dir(), reason="goal-pattern-fast case not present"
)
class TestGoalPatternFast:
    """Non-recursive pattern goal: routes through the SPARQL fast path."""

    def test_result_matches_golden(self) -> None:
        """Full result equals committed golden: bindings {b,c}, status ok."""
        nq = _load_nq(_CASE_FAST)
        query = _load_query(_CASE_FAST, "children")
        profile = _profile(_CASE_FAST)
        golden = _load_golden(_CASE_FAST, "children")

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        _assert_canonical_equal(result, golden, "goal-pattern-fast/children")

    def test_status_ok(self) -> None:
        """Status must be 'ok'."""
        nq = _load_nq(_CASE_FAST)
        query = _load_query(_CASE_FAST, "children")
        profile = _profile(_CASE_FAST)

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        assert result["status"] == "ok"

    def test_two_bindings_returned(self) -> None:
        """Must return exactly 2 bindings: b and c."""
        nq = _load_nq(_CASE_FAST)
        query = _load_query(_CASE_FAST, "children")
        profile = _profile(_CASE_FAST)

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        assert len(result["bindings"]) == 2, (
            f"Expected 2 bindings, got {len(result['bindings'])}"
        )


# --------------------------------------------------------------------------- #
# Case 3: goal-budget-trip (max_answers budget cap)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(
    not _CASE_BUDGET.is_dir(), reason="goal-budget-trip case not present"
)
class TestGoalBudgetTrip:
    """Budget governor: max_answers=2 caps the recursive query at 2 results."""

    def test_result_matches_golden(self) -> None:
        """Full result equals committed golden: 2 bindings, status partial."""
        nq = _load_nq(_CASE_BUDGET)
        query = _load_query(_CASE_BUDGET, "ancestor")
        profile = _profile(_CASE_BUDGET)
        max_ans = _max_answers(_CASE_BUDGET)
        golden = _load_golden(_CASE_BUDGET, "ancestor")

        result = gmeow_logic.query(nq, query, profile, None, max_ans, None)
        _assert_canonical_equal(result, golden, "goal-budget-trip/ancestor")

    def test_status_partial(self) -> None:
        """Status must be 'partial' (budget tripped before all answers)."""
        nq = _load_nq(_CASE_BUDGET)
        query = _load_query(_CASE_BUDGET, "ancestor")
        profile = _profile(_CASE_BUDGET)
        max_ans = _max_answers(_CASE_BUDGET)

        result = gmeow_logic.query(nq, query, profile, None, max_ans, None)
        assert result["status"] == "partial", (
            f"Expected status='partial', got {result['status']!r}"
        )

    def test_exactly_two_bindings(self) -> None:
        """Must return exactly 2 bindings (the max_answers ceiling)."""
        nq = _load_nq(_CASE_BUDGET)
        query = _load_query(_CASE_BUDGET, "ancestor")
        profile = _profile(_CASE_BUDGET)
        max_ans = _max_answers(_CASE_BUDGET)

        result = gmeow_logic.query(nq, query, profile, None, max_ans, None)
        n = len(result["bindings"])
        assert n == 2, f"Expected exactly 2 bindings (max_answers=2), got {n}"


# --------------------------------------------------------------------------- #
# Case 4: goal-procedural-cut (cut confinement — AC-2)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(
    not _CASE_CUT.is_dir(), reason="goal-procedural-cut case not present"
)
class TestGoalProceduralCut:
    """AC-2: cut confinement — ProceduralPrologProfile permits `!`."""

    def test_result_matches_golden(self) -> None:
        """Full result equals committed golden: 1 binding (b), status ok."""
        nq = _load_nq(_CASE_CUT)
        query = _load_query(_CASE_CUT, "first")
        profile = _profile(_CASE_CUT)
        golden = _load_golden(_CASE_CUT, "first")

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        _assert_canonical_equal(result, golden, "goal-procedural-cut/first")

    def test_status_ok(self) -> None:
        """Status must be 'ok' (cut committed to first answer)."""
        nq = _load_nq(_CASE_CUT)
        query = _load_query(_CASE_CUT, "first")
        profile = _profile(_CASE_CUT)

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        assert result["status"] == "ok"

    def test_exactly_one_binding_b(self) -> None:
        """Must return exactly 1 binding and it must be b (sorted-first)."""
        nq = _load_nq(_CASE_CUT)
        query = _load_query(_CASE_CUT, "first")
        profile = _profile(_CASE_CUT)
        base = "https://example.org/profiles/goal-procedural-cut/"

        result = gmeow_logic.query(nq, query, profile, None, None, None)
        bindings = result["bindings"]
        assert len(bindings) == 1, (
            f"Cut must yield exactly 1 binding, got {len(bindings)}: {bindings}"
        )
        assert bindings[0]["Y"] == f"<{base}b>", (
            f"Cut must commit to sorted-first value b, got {bindings[0]['Y']!r}"
        )

    def test_cut_under_positive_horn_raises_value_error(self) -> None:
        """AC-2 negative path: cut under PositiveHornProfile must raise ValueError.

        The profile gate must reject `!` when the declared profile is not
        ProceduralPrologProfile.
        """
        nq = _load_nq(_CASE_CUT)
        query = _load_query(_CASE_CUT, "first")

        with pytest.raises(ValueError, match="cut"):
            gmeow_logic.query(nq, query, "PositiveHornProfile", None, None, None)
