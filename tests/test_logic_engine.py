# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""FFI-contract smoke tests for the PyO3 ``gmeow_logic.materialize`` binding.

The materialize **engine semantics** — round-trip, per-world mapping, world
isolation, real provenance (``source_quad_ids`` / ``rule_iri`` / the
``logic:assert`` sentinel), the empty/whitespace short-circuit, and transitive
derivation — are pinned natively in
``crates/logic/src/materialize.rs`` (the ``materialize_core`` ``#[test]`` module,
issue #786 / T5). They no longer run through Python.

What remains here is intentionally thin:

* one **FFI-marshalling smoke** proving the binding marshals each ``DerivedQuad``
  into a Python ``dict`` with the full seam field set (the binding's actual job),
  and
* the **empty-case oracle parity** check, which exercises the *Python* oracle
  (``gmeow_tools.logic_frontend`` / ``logic_ir``) rather than the FFI, so it is
  not subsumed by the native tests.

The native extension is required. Missing ``gmeow_logic`` is a test-environment
failure, not a skip.
"""

from __future__ import annotations

import pytest

from tests._required_native import require_gmeow_logic

gmeow_logic = require_gmeow_logic()

# ── Fixtures ──────────────────────────────────────────────────────────────────

# N-Quads covering two distinct named-graph worlds.
# Each line is a quad: <subject> <predicate> <object> <graph> .
_S1 = "<http://example.org/s/1>"
_S2 = "<http://example.org/s/2>"
_S3 = "<http://example.org/s/3>"
_P_TYPE = "<http://example.org/p/type>"
_P_NAME = "<http://example.org/p/name>"
_O_THING = "<http://example.org/o/Thing>"
_O_FOO = "<http://example.org/o/Foo>"
_O_BAR = "<http://example.org/o/Bar>"
_W_ALPHA = "<http://world/Alpha>"
_W_BETA = "<http://world/Beta>"

_TWO_WORLD_NQUADS = (
    f"{_S1} {_P_TYPE} {_O_THING} {_W_ALPHA} .\n"
    f"{_S2} {_P_NAME} {_O_FOO} {_W_ALPHA} .\n"
    f"{_S3} {_P_TYPE} {_O_BAR} {_W_BETA} .\n"
)

# The full set of seam fields the binding must marshal into every result dict.
_REQUIRED_META_FIELDS = {
    "graph",
    "subject",
    "predicate",
    "object",
    "graph_component",
    "derivation_id",
    "rule_iri",
    "source_quad_ids",
    "profile",
    "budget_status",
}


# ── FFI-marshalling smoke ─────────────────────────────────────────────────────


def test_materialize_ffi_marshals_quad_dicts() -> None:
    """The binding returns a list of dicts carrying the full seam field set.

    This is a pure FFI-contract check: it proves the ``DerivedQuad → Python
    dict`` marshalling is wired correctly (a list of dicts, every required key
    present, ``source_quad_ids`` list-typed). The *values* and engine behaviour
    are asserted natively in ``materialize_core``'s ``#[test]`` module.
    """
    result = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    assert isinstance(result, list), f"expected list, got {type(result)}"
    assert len(result) == 3, f"expected 3 quads back, got {len(result)}"
    for i, quad_dict in enumerate(result):
        assert isinstance(quad_dict, dict), f"quad[{i}] is not a dict"
        missing = _REQUIRED_META_FIELDS - set(quad_dict.keys())
        assert not missing, f"quad[{i}] missing metadata fields: {missing!r}"
        assert isinstance(quad_dict["source_quad_ids"], list), (
            f"quad[{i}]: source_quad_ids is not a list, "
            f"got {type(quad_dict['source_quad_ids'])}"
        )


# ── AC#4: empty-case oracle parity (Python oracle, not the FFI) ───────────────


def test_empty_case_oracle_parity() -> None:
    """AC#4: empty materialize output == empty LogicProgram canonical output.

    The Python oracle (parse_logic_source) raises LogicParseError on empty
    graphs, so the canonical empty-program state is constructed directly
    (zero axioms, rules, profiles).  Both the Rust materialize and the Python
    oracle must agree: empty in → empty/zero out.  This exercises the Python
    oracle surface, so it is retained alongside the native engine tests.
    """
    from gmeow_tools.logic_frontend import LogicParseError
    from gmeow_tools.logic_ir import LogicProgram

    # Rust side: empty input → empty result list.
    rust_result = gmeow_logic.materialize("", "")
    assert rust_result == [], "Rust materialize: empty input must return empty list"

    # Python oracle side: an empty graph raises LogicParseError; the canonical
    # empty LogicProgram (no axioms/rules/profiles) represents the zero state.
    empty_program = LogicProgram(axioms=(), rules=(), profiles=())
    canonical = empty_program.canonical()

    assert canonical["axioms"] == [], (
        f"oracle: empty LogicProgram should have no axioms, got {canonical['axioms']!r}"
    )
    assert canonical["rules"] == [], (
        f"oracle: empty LogicProgram should have no rules, got {canonical['rules']!r}"
    )
    assert canonical["profiles"] == [], (
        f"oracle: empty LogicProgram should have no profiles, "
        f"got {canonical['profiles']!r}"
    )

    # The parity assertion: both sides agree on the empty/trivial case.
    # Rust: 0 materialized quads.  Python: 0 axioms, 0 rules, 0 profiles.
    assert len(rust_result) == 0
    assert len(canonical["axioms"]) == 0
    assert len(canonical["rules"]) == 0
    assert len(canonical["profiles"]) == 0

    # Confirm the oracle would raise on empty graph input (fail-fast contract).
    from rdflib import Graph

    with pytest.raises(LogicParseError, match="empty"):
        from gmeow_tools.logic_frontend import parse_logic_source

        parse_logic_source(Graph())
