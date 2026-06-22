# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for Stratum-C counterfactual resolution via gmeow_logic.query (#505).

Exercises the worlds-C conformance corpus end-to-end through the native engine:

  worlds-C/revision     — deterministic AGM revision:
    * consequent  — admit status(server, down) -> alert fires            (AC-1)
    * determined  — over-determined {primary, backup} arbitrated by
                    gmeow:overrides -> the more-entrenched value wins     (AC-3)
    * tie         — incomparable {blue, green} -> status "unknown"        (AC-3)
  worlds-C/lewis        — opt-in LewisCredulousProfile: union of the two
                          closest worlds {blue, green}                    (Lewis)
  worlds-C/nested-budget — depth_budget(0) -> status "incomplete"         (budget)

Each query's engine result is compared against the committed golden
``expected/answers/<q>.json``.  AC-2 (no leakage of the constructed world into
the base store) is structurally guaranteed — construct_world builds a fresh,
isolated graph and never mutates the input — and is pinned by the Rust unit test
``counterfactual::tests::no_leakage_base_store_unchanged``; here we additionally
assert the base store is observable unchanged after a counterfactual query.

The native extension is required. Missing ``gmeow_logic`` is a test-environment
failure, not a skip.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from tests._required_native import require_gmeow_logic

gmeow_logic = require_gmeow_logic()

_REPO_ROOT = Path(__file__).resolve().parents[1]
_WORLDS_C = _REPO_ROOT / "conformance" / "logic" / "cases" / "worlds-C"


def _profile(case_dir: Path) -> str:
    """The query-resolution profile from a case's profile.json.

    Prefers the optional ``counterfactual_profile`` (a Stratum-C revision profile
    such as ``LewisCredulousProfile``) over the materialization ``semantic_profile``
    — mirroring the runner's ``query_profile`` selection.
    """
    data = json.loads((case_dir / "profile.json").read_text(encoding="utf-8"))
    profile = data.get("counterfactual_profile", data["reasoning_contract"]["preset"])
    assert isinstance(profile, str)
    return profile


def _discover() -> list[tuple[str, str]]:
    """Return (case_name, query_stem) for every worlds-C query, sorted.

    A missing corpus directory yields an empty list rather than raising at import
    time, so ``test_worlds_c_corpus_is_present`` can run and report the absence
    with its own assertion instead of crashing collection.
    """
    pairs: list[tuple[str, str]] = []
    try:
        case_dirs = sorted(p for p in _WORLDS_C.iterdir() if p.is_dir())
    except FileNotFoundError:
        return pairs
    for case_dir in case_dirs:
        for qfile in sorted((case_dir / "queries").glob("*.logic")):
            pairs.append((case_dir.name, qfile.stem))
    return pairs


_CASES = _discover()


def test_worlds_c_corpus_is_present() -> None:
    """The worlds-C corpus must be populated (guards against an empty discovery)."""
    assert _CASES, "no worlds-C queries discovered — corpus missing?"
    names = {c for c, _ in _CASES}
    assert {"revision", "lewis", "nested-budget"} <= names, names


@pytest.mark.parametrize(
    ("case_name", "query_stem"),
    _CASES,
    ids=[f"{c}/{q}" for c, q in _CASES],
)
def test_counterfactual_matches_golden(case_name: str, query_stem: str) -> None:
    """Each worlds-C query resolves to its committed golden answer set."""
    case_dir = _WORLDS_C / case_name
    nquads = (case_dir / "input.nq").read_text(encoding="utf-8")
    qtext = (case_dir / "queries" / f"{query_stem}.logic").read_text(encoding="utf-8")
    golden = json.loads(
        (case_dir / "expected" / "answers" / f"{query_stem}.json").read_text(
            encoding="utf-8"
        )
    )

    result = gmeow_logic.query(nquads, qtext, _profile(case_dir), None, None, None)

    assert result["status"] == golden["status"], (
        f"{case_name}/{query_stem}: status {result['status']!r} != {golden['status']!r}"
    )
    # Compare binding sets order-independently (each binding is a flat str->str map).
    got = sorted(tuple(sorted(b.items())) for b in result["bindings"])
    want = sorted(tuple(sorted(b.items())) for b in golden["bindings"])
    assert got == want, f"{case_name}/{query_stem}: bindings {got} != {want}"


def test_ac3_tie_is_unknown_not_a_branch() -> None:
    """A genuine entrenchment tie yields exactly status 'unknown' with no bindings."""
    case_dir = _WORLDS_C / "revision"
    nquads = (case_dir / "input.nq").read_text(encoding="utf-8")
    qtext = (case_dir / "queries" / "tie.logic").read_text(encoding="utf-8")
    result = gmeow_logic.query(nquads, qtext, _profile(case_dir), None, None, None)
    assert result["status"] == "unknown"
    assert result["bindings"] == []


def test_ac2_base_world_is_not_mutated() -> None:
    """AC-2: a counterfactual that overwrites a functional slot must not leak into
    the base store — after running it, the base world still reads its original
    value (the constructed world is a fresh, isolated graph).

    The test first *exercises* the overwrite (the consequent case admits
    status(server, down) and fires alert in W_cf), so the assertion below is a
    genuine non-mutation check rather than a bare baseline read.
    """
    case_dir = _WORLDS_C / "revision"
    nquads = (case_dir / "input.nq").read_text(encoding="utf-8")

    # (1) Run the counterfactual that overwrites status(server, up) -> down in
    #     W_cf and fires the alert rule there. It must succeed inside W_cf.
    cf_query = (case_dir / "queries" / "consequent.logic").read_text(encoding="utf-8")
    cf = gmeow_logic.query(nquads, cf_query, _profile(case_dir), None, None, None)
    assert cf["status"] == "ok", cf
    fired = {b["Z"] for b in cf["bindings"]}
    assert fired == {"<https://example.org/worlds-c/revision/fired>"}, cf

    # (2) Re-query the base world with a plain (non-counterfactual) goal: status
    #     must still be 'up' — the 'down' overwrite was confined to W_cf and never
    #     mutated the base store.
    base_query = (
        ":- prefix(ex, 'https://example.org/worlds-c/revision/').\n"
        "?- ex:status(ex:server, Z).\n"
    )
    base = gmeow_logic.query(nquads, base_query, _profile(case_dir), None, None, None)
    zs = {b["Z"] for b in base["bindings"]}
    assert zs == {"<https://example.org/worlds-c/revision/up>"}, base
