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
* the **empty-case contract**: ``materialize`` on empty input yields zero quads,
  and ``compile_logic`` rejects an empty/whitespace source (fail-fast). The Python
  logic oracle (``logic_frontend`` / ``logic_ir``) was deleted in #664/#727 — the
  compiler is Rust-authoritative — so the empty case is a native FFI contract, not
  a Python-oracle parity check.

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
    """The binding returns a ``{quads, preservation}`` disclosure dict.

    This is a pure FFI-contract check: it proves the ``DerivedQuad → Python
    dict`` marshalling is wired correctly (a list of dicts under ``quads``, every
    required key present, ``source_quad_ids`` list-typed) and that the preservation
    disclosure surface is present. The *values* and engine behaviour are asserted
    natively in ``materialize``'s ``#[test]`` module.
    """
    out = gmeow_logic.materialize("", _TWO_WORLD_NQUADS)
    assert isinstance(out, dict), f"expected disclosure dict, got {type(out)}"
    assert "preservation" in out, "materialize must disclose a preservation claim"
    result = out["quads"]
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


# ── AC#4: empty-case contract (native; the Python oracle was deleted) ──────────


def test_empty_case_trivial() -> None:
    """AC#4: empty materialize input → empty result (the trivial zero case).

    The logic compiler is Rust-authoritative since #664/#727 (the Python oracle
    was deleted); the empty/trivial contract is simply: empty materialize input
    yields zero materialized quads.
    """
    rust_result = gmeow_logic.materialize("", "")
    assert rust_result["quads"] == [], (
        "Rust materialize: empty input must return no quads"
    )
    assert len(rust_result["quads"]) == 0


def test_compile_logic_empty_source_rejected() -> None:
    """The Rust compiler rejects an empty/whitespace-only source (fail-fast).

    Mirrors the historical oracle fail-fast contract: there is no silent
    empty-program fallback — a source with no logic: vocabulary is a hard error.
    """
    with pytest.raises(ValueError):
        gmeow_logic.compile_logic("")
