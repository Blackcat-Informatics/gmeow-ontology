# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for gmeow_logic.query probabilistic inference (issue #506, v6).

Drives the four conformance cases under conformance/logic/cases/profiles/ through
the Rust probabilistic evaluator (weighted model counting) and checks each against
its committed golden, plus explicit red-if-violated guard assertions:

  1. probabilistic-marginal-independent — AC-1: noisy-OR marginal 0.75 under a
     declared logic:FullIndependence model.
  2. probabilistic-dependency-joint     — declared logic:DependencyModel joint;
     marginal 0.5, differing from the 0.25 independence reading.
  3. probabilistic-confidence-guard     — AC-2: a logic:confidence annotation is
     NEVER read as a probability (marginal 1.0, NOT 0.9).
  4. probabilistic-no-model             — probabilistic facts with no declared
     model REFUSE with status "unknown".

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

_CASE_INDEP = _PROFILES_CASES / "probabilistic-marginal-independent"
_CASE_JOINT = _PROFILES_CASES / "probabilistic-dependency-joint"
_CASE_CONFIDENCE = _PROFILES_CASES / "probabilistic-confidence-guard"
_CASE_NO_MODEL = _PROFILES_CASES / "probabilistic-no-model"

_PROBABILISTIC_PROFILE = "ProbabilisticProfile"


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _load_nq(case_dir: Path) -> str:
    return (case_dir / "input.nq").read_text(encoding="utf-8")


def _load_query(case_dir: Path, stem: str) -> str:
    return (case_dir / "queries" / f"{stem}.logic").read_text(encoding="utf-8")


def _load_golden(case_dir: Path, stem: str) -> dict[str, object]:
    data: dict[str, object] = json.loads(
        (case_dir / "expected" / "answers" / f"{stem}.json").read_text(encoding="utf-8")
    )
    return data


def _profile(case_dir: Path) -> str:
    data = json.loads((case_dir / "profile.json").read_text(encoding="utf-8"))
    return str(data["semantic_profile"])


def _run(case_dir: Path, stem: str) -> dict[str, object]:
    """Resolve queries/<stem>.logic for a case through the probabilistic engine."""
    result: dict[str, object] = gmeow_logic.query(
        _load_nq(case_dir),
        _load_query(case_dir, stem),
        _profile(case_dir),
        None,
        None,
        None,
    )
    return result


def _assert_canonical_equal(
    actual: dict[str, object], expected: dict[str, object], label: str
) -> None:
    actual_canon = json.dumps(actual, sort_keys=True, ensure_ascii=False)
    expected_canon = json.dumps(expected, sort_keys=True, ensure_ascii=False)
    assert actual_canon == expected_canon, (
        f"{label}: canonical JSON mismatch\n"
        f"  actual:   {actual_canon}\n"
        f"  expected: {expected_canon}"
    )


def _only_binding(result: dict[str, object]) -> dict[str, object]:
    bindings = result["bindings"]
    assert isinstance(bindings, list) and len(bindings) == 1, (
        f"expected exactly one binding, got {bindings!r}"
    )
    b: dict[str, object] = bindings[0]
    return b


# --------------------------------------------------------------------------- #
# All four cases declare ProbabilisticProfile (sanity: the profile is the v6 one)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CASE_INDEP.is_dir(), reason="case not present")
def test_all_cases_declare_probabilistic_profile() -> None:
    for case in (_CASE_INDEP, _CASE_JOINT, _CASE_CONFIDENCE, _CASE_NO_MODEL):
        assert _profile(case) == _PROBABILISTIC_PROFILE, (
            f"{case.name} must declare {_PROBABILISTIC_PROFILE}"
        )


# --------------------------------------------------------------------------- #
# Case 1: AC-1 — independent marginal (noisy-OR → 0.75)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CASE_INDEP.is_dir(), reason="case not present")
class TestMarginalIndependent:
    """Marginals computed correctly under a declared independence model."""

    def test_result_matches_golden(self) -> None:
        _assert_canonical_equal(
            _run(_CASE_INDEP, "wet"),
            _load_golden(_CASE_INDEP, "wet"),
            "probabilistic-marginal-independent/wet",
        )

    def test_noisy_or_marginal_is_three_quarters(self) -> None:
        """P(wet) = 1 - (1-0.5)(1-0.5) = 0.75 — the exact θ-enumeration sum."""
        result = _run(_CASE_INDEP, "wet")
        assert result["status"] == "ok"
        assert _only_binding(result)["probability"] == 0.75


# --------------------------------------------------------------------------- #
# Case 2: dependency joint — 0.5, NOT the 0.25 independence reading
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CASE_JOINT.is_dir(), reason="case not present")
class TestDependencyJoint:
    """Marginals under a declared dependency joint differ from independence."""

    def test_result_matches_golden(self) -> None:
        _assert_canonical_equal(
            _run(_CASE_JOINT, "both"),
            _load_golden(_CASE_JOINT, "both"),
            "probabilistic-dependency-joint/both",
        )

    def test_joint_marginal_is_half_not_quarter(self) -> None:
        """Perfectly-correlated joint gives P(both)=0.5, not the independent 0.25."""
        result = _run(_CASE_JOINT, "both")
        assert result["status"] == "ok"
        prob = _only_binding(result)["probability"]
        assert prob == 0.5, f"expected 0.5 (joint), got {prob!r}"
        assert prob != 0.25, "joint was ignored — fell back to independence (0.25)"


# --------------------------------------------------------------------------- #
# Case 3: AC-2 — confidence is NOT a probability (red if violated)
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CASE_CONFIDENCE.is_dir(), reason="case not present")
class TestConfidenceGuard:
    """A logic:confidence annotation must never be read as a probability."""

    def test_result_matches_golden(self) -> None:
        _assert_canonical_equal(
            _run(_CASE_CONFIDENCE, "diagnosis"),
            _load_golden(_CASE_CONFIDENCE, "diagnosis"),
            "probabilistic-confidence-guard/diagnosis",
        )

    def test_confidence_not_promoted_to_probability(self) -> None:
        """The asserted fact's marginal is 1.0; the 0.9 confidence never leaks."""
        result = _run(_CASE_CONFIDENCE, "diagnosis")
        assert result["status"] == "ok"
        prob = _only_binding(result)["probability"]
        assert prob == 1.0, f"asserted fact must be deterministic (1.0), got {prob!r}"
        assert prob != 0.9, (
            "confidence 0.9 leaked into the probability — guard violated"
        )


# --------------------------------------------------------------------------- #
# Case 4: no declared model → refuse with status "unknown"
# --------------------------------------------------------------------------- #


@pytest.mark.skipif(not _CASE_NO_MODEL.is_dir(), reason="case not present")
class TestNoModelRefusal:
    """Probabilistic facts with no declared model must refuse, not assume
    independence."""

    def test_result_matches_golden(self) -> None:
        _assert_canonical_equal(
            _run(_CASE_NO_MODEL, "wet"),
            _load_golden(_CASE_NO_MODEL, "wet"),
            "probabilistic-no-model/wet",
        )

    def test_refuses_with_unknown_and_no_bindings(self) -> None:
        result = _run(_CASE_NO_MODEL, "wet")
        assert result["status"] == "unknown", (
            f"no declared model must refuse with 'unknown', got {result['status']!r}"
        )
        assert result["bindings"] == [], (
            "refusal must produce no marginals (no silent independence assumption)"
        )
