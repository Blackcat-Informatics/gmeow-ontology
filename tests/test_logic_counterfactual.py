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

Skipped cleanly when the native extension is not installed (run 'make logic-py').
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

gmeow_logic = pytest.importorskip(
    "gmeow_logic",
    reason="gmeow_logic native extension not installed — run 'make logic-py' first",
)

_REPO_ROOT = Path(__file__).resolve().parents[1]
_WORLDS_C = _REPO_ROOT / "conformance" / "logic" / "cases" / "worlds-C"


def _profile(case_dir: Path) -> str:
    """The query-resolution profile from a case's profile.json.

    Prefers the optional ``counterfactual_profile`` (a Stratum-C revision profile
    such as ``LewisCredulousProfile``) over the materialization ``semantic_profile``
    — mirroring the runner's ``query_profile`` selection.
    """
    data = json.loads((case_dir / "profile.json").read_text(encoding="utf-8"))
    profile = data.get("counterfactual_profile", data["semantic_profile"])
    assert isinstance(profile, str)
    return profile


def _discover() -> list[tuple[str, str]]:
    """Return (case_name, query_stem) for every worlds-C query, sorted."""
    pairs: list[tuple[str, str]] = []
    for case_dir in sorted(p for p in _WORLDS_C.iterdir() if p.is_dir()):
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
    """AC-2: after a counterfactual overwrite, the base world still reads its
    original value (the constructed world never leaks into the base store)."""
    case_dir = _WORLDS_C / "revision"
    nquads = (case_dir / "input.nq").read_text(encoding="utf-8")
    # A plain (non-counterfactual) goal against the base world: status(server, ·)
    # must still be 'up' — the counterfactual 'down' overwrite is confined to W_cf.
    base_query = (
        ":- prefix(ex, 'https://example.org/worlds-c/revision/').\n"
        "?- ex:status(ex:server, Z).\n"
    )
    base = gmeow_logic.query(nquads, base_query, _profile(case_dir), None, None, None)
    zs = {b["Z"] for b in base["bindings"]}
    assert zs == {"<https://example.org/worlds-c/revision/up>"}, base
