# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Correctness tests for the foundation-lowering conformance cases (issue #503, Task 5).

These tests are independent of the conformance gate's diff-against-golden machinery.
The gate (``gmeow-dev conformance``) generates goldens from ``logic_runner.run`` and
then diffs ``run`` output against them, so a golden trivially diff-passes regardless
of whether it is *correct*.  This module instead PARSES each committed
``expected/materialized.nq`` and asserts the discipline-fact set it contains is
EXACTLY the lint-faithful set hand-computed per :mod:`gmeow_tools.reasoning_lint`.

For each foundation case we collect the three discipline-fact families the
foundation lowering emits:

* ``logic:violation``           — the four in-world OntoUML disciplines (Task 2):
  StereotypeCardinality / MixIden / FreeRole / MixRig / RelComp.
* ``logic:rigidityViolation``   — the cross-world rigidity closure (Task 3).
* ``logic:dischargeObligation`` /
  ``logic:witnessRequiredViolation`` — the anti-rigidity witness policy (Task 4).

The expected sets below are the verdicts :mod:`reasoning_lint` produces over the
equivalent gUFO schema (cross-checked against the lint in development); the
``mixrig-kind-under-role`` case in particular asserts the MixRig catch (AC#3).

A second test re-runs ``diff_case(run(case_dir))`` per case and asserts zero diffs,
pinning the goldens to the runner oracle (the locally authoritative path; the native
engine is not built in this environment).
"""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.logic_runner import diff_case
from gmeow_tools.logic_runner import run as logic_run

# --------------------------------------------------------------------------- #
# Constants
# --------------------------------------------------------------------------- #

_FOUNDATION_ROOT = (
    Path(__file__).resolve().parents[1]
    / "conformance"
    / "logic"
    / "cases"
    / "foundation"
)

_LOGIC = "https://blackcatinformatics.ca/logic/"
_P_VIOLATION = _LOGIC + "violation"
_P_RIGIDITY = _LOGIC + "rigidityViolation"
_P_DISCHARGE = _LOGIC + "dischargeObligation"
_P_WITNESS_REQ = _LOGIC + "witnessRequiredViolation"

#: The discipline predicates whose facts this test extracts and compares.
_DISCIPLINE_PREDICATES = frozenset(
    {_P_VIOLATION, _P_RIGIDITY, _P_DISCHARGE, _P_WITNESS_REQ}
)


def _b(case: str) -> str:
    """Return the example IRI base for a foundation case."""
    return f"https://example.org/foundation/{case}/"


# --------------------------------------------------------------------------- #
# Hand-computed lint-faithful expected discipline-fact sets, per case.
#
# Each entry is a set of (subject, predicate, object, graph) tuples — every
# discipline fact the case's materialized.nq must contain, and NO others.
# --------------------------------------------------------------------------- #


def _expected_facts() -> dict[str, set[tuple[str, str, str, str]]]:
    f: dict[str, set[tuple[str, str, str, str]]] = {}

    # 1. exactly-one-stereotype:
    #    NoStereo  → StereotypeCardinality (0-stereotype branch)
    #    TwoStereo → StereotypeCardinality (>1 branch) + FreeRole (Role w/ no rigid
    #                ancestor — anti_rigidity_discipline fires independently)
    b = _b("exactly-one-stereotype")
    g = b + "schema"
    f["exactly-one-stereotype"] = {
        (b + "NoStereo", _P_VIOLATION, _LOGIC + "StereotypeCardinality", g),
        (b + "TwoStereo", _P_VIOLATION, _LOGIC + "StereotypeCardinality", g),
        (b + "TwoStereo", _P_VIOLATION, _LOGIC + "FreeRole", g),
    }

    # 2. identity-overlap-mixiden:
    #    Dog (Kind) ⊑ Animal (Kind) → MixIden.  Animal is a clean top Kind.
    b = _b("identity-overlap-mixiden")
    g = b + "schema"
    f["identity-overlap-mixiden"] = {
        (b + "Dog", _P_VIOLATION, _LOGIC + "MixIden", g),
    }

    # 3. free-role:
    #    Wanderer (Role, no rigid ancestor) → FreeRole + MixIden (non-Kind sortal
    #    tracing to zero Kinds).
    b = _b("free-role")
    g = b + "schema"
    f["free-role"] = {
        (b + "Wanderer", _P_VIOLATION, _LOGIC + "FreeRole", g),
        (b + "Wanderer", _P_VIOLATION, _LOGIC + "MixIden", g),
    }

    # 4. mixrig-kind-under-role (AC#3):
    #    HonorsStudent (SubKind) ⊑ Student (Role) → MixRig + MixIden.
    #    Student (Role, no rigid ancestor)        → FreeRole + MixIden.
    b = _b("mixrig-kind-under-role")
    g = b + "schema"
    f["mixrig-kind-under-role"] = {
        (b + "HonorsStudent", _P_VIOLATION, _LOGIC + "MixRig", g),
        (b + "HonorsStudent", _P_VIOLATION, _LOGIC + "MixIden", g),
        (b + "Student", _P_VIOLATION, _LOGIC + "FreeRole", g),
        (b + "Student", _P_VIOLATION, _LOGIC + "MixIden", g),
    }

    # 5. relcomp-under-mediated:
    #    Marriage (concrete Relator, 1 distinct mediated relatum) → RelComp.
    #    Employment (2 distinct relata) is well-formed and trips nothing.
    b = _b("relcomp-under-mediated")
    g = b + "schema"
    f["relcomp-under-mediated"] = {
        (b + "Marriage", _P_VIOLATION, _LOGIC + "RelComp", g),
    }

    # 6. cross-world-rigidity:
    #    alice rigidityViolation Person  IN worldB (rigid Kind fails to persist).
    #    bob   — typed Person in both worlds → no rigidity violation.
    #    carol dischargeObligation Employee IN worldA (Task-4 witness-obligation).
    #    Schema type-level lint over worldA: the Role Employee → FreeRole + MixIden.
    b = _b("cross-world-rigidity")
    wa = b + "worldA"
    wb = b + "worldB"
    f["cross-world-rigidity"] = {
        (b + "alice", _P_RIGIDITY, b + "Person", wb),
        (b + "carol", _P_DISCHARGE, b + "Employee", wa),
        (b + "Employee", _P_VIOLATION, _LOGIC + "FreeRole", wa),
        (b + "Employee", _P_VIOLATION, _LOGIC + "MixIden", wa),
    }

    return f


_EXPECTED = _expected_facts()
_CASES = sorted(_EXPECTED)


# --------------------------------------------------------------------------- #
# N-Quads parsing (discipline facts only)
# --------------------------------------------------------------------------- #


def _parse_discipline_facts(nq_text: str) -> set[tuple[str, str, str, str]]:
    """Extract the discipline facts (S, P, O, G) from a materialized.nq golden.

    Only quads whose predicate is one of the four discipline predicates are
    collected, and only those whose object is an IRI (every discipline object is
    an IRI — a label individual, a rigid type, or an anti-rigid type).  Parsing is
    deliberately a simple ``<s> <p> <o> <g> .`` split so the assertion does not
    depend on the same serializer that produced the golden.
    """
    facts: set[tuple[str, str, str, str]] = set()
    for raw in nq_text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or not line.endswith("."):
            continue
        body = line[:-1].strip()  # drop trailing '.'
        if not (body.startswith("<") and body.endswith(">")):
            # object is a literal (no discipline fact has a literal object); skip.
            continue
        # Split on '> <' boundaries — every term here is an IRI in <...> form.
        terms = body.split("> <")
        if len(terms) != 4:
            continue
        s = terms[0].lstrip("<")
        p = terms[1]
        o = terms[2]
        gg = terms[3].rstrip(">")
        if p in _DISCIPLINE_PREDICATES:
            facts.add((s, p, o, gg))
    return facts


# --------------------------------------------------------------------------- #
# Tests
# --------------------------------------------------------------------------- #


@pytest.mark.parametrize("case", _CASES)
def test_materialized_discipline_facts_are_lint_faithful(case: str) -> None:
    """The committed materialized.nq contains EXACTLY the lint-faithful fact set."""
    mat_path = _FOUNDATION_ROOT / case / "expected" / "materialized.nq"
    assert mat_path.exists(), f"missing golden materialized.nq for {case}"
    actual = _parse_discipline_facts(mat_path.read_text(encoding="utf-8"))
    expected = _EXPECTED[case]
    assert actual == expected, (
        f"{case}: discipline-fact set mismatch\n"
        f"  unexpected (in golden, not expected): {sorted(actual - expected)}\n"
        f"  missing    (expected, not in golden): {sorted(expected - actual)}"
    )


def test_mixrig_ac3_is_caught() -> None:
    """AC#3: the SubKind-under-Role case MUST carry the MixRig verdict."""
    case = "mixrig-kind-under-role"
    b = _b(case)
    mat_path = _FOUNDATION_ROOT / case / "expected" / "materialized.nq"
    actual = _parse_discipline_facts(mat_path.read_text(encoding="utf-8"))
    mixrig = (b + "HonorsStudent", _P_VIOLATION, _LOGIC + "MixRig", b + "schema")
    assert mixrig in actual


@pytest.mark.parametrize("case", _CASES)
def test_runner_diff_against_golden_is_clean(case: str) -> None:
    """``diff_case(run(case))`` reports zero diffs — goldens pinned to the oracle."""
    case_dir = _FOUNDATION_ROOT / case
    result = diff_case(logic_run(case_dir))
    assert result.passed, f"{case}: diff_case reported:\n" + "\n".join(result.diffs)
