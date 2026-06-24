# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Correctness tests for the foundation-lowering conformance cases (issue #503, Task 5).

These tests are independent of the conformance gate's diff-against-golden machinery.
The native gate (the ``gmeow-conformance`` datatest harness) runs the engine and
diffs its output against the committed goldens, so a golden trivially diff-passes
regardless of whether it is *correct*.  This module instead PARSES each committed
``expected/materialized.nq`` and asserts the discipline-fact set it contains is
EXACTLY the lint-faithful set hand-computed per the native
``gmeow_validate.reasoning_*_nt`` anti-pattern checks.

For each foundation case we collect the three discipline-fact families the
foundation lowering emits:

* ``logic:violation``           — the four in-world OntoUML disciplines (Task 2):
  StereotypeCardinality / MixIden / FreeRole / MixRig / RelComp.
* ``logic:rigidityViolation``   — the cross-world rigidity closure (Task 3).
* ``logic:dischargeObligation`` /
  ``logic:witnessRequiredViolation`` — the anti-rigidity witness policy (Task 4).

The expected sets below are the verdicts the native ``gmeow_validate`` checks produce over the
equivalent gUFO schema (cross-checked against the lint in development); the
``mixrig-kind-under-role`` case in particular asserts the MixRig catch (AC#3).

The runner-vs-golden diff that formerly lived here is now enforced natively by the
``gmeow-conformance`` datatest harness (crates/conformance) under cargo-nextest.
"""

from __future__ import annotations

from pathlib import Path

import pytest

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
    """Return hand-computed discipline-fact sets for each foundation case."""
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


# NOTE: the runner-vs-golden diff (formerly
# ``test_runner_diff_against_golden_is_clean``, which called
# ``logic_runner.diff_case(run(case))``) is now enforced natively by the
# `gmeow-conformance` datatest harness (crates/conformance), which runs + diffs
# every foundation case under cargo-nextest. The hand-computed lint-faithful
# assertions above remain here because they verify the goldens are *correct* (not
# merely self-consistent with the runner), a property the diff gate cannot give.
