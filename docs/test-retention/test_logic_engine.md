# Retention: `tests/test_logic_engine.py`

**Category:** PyO3 seam

## What it tests

The `gmeow_logic.materialize` / `gmeow_logic.compile_logic` FFI boundary — the
marshalling and error-surfacing the PyO3 binding is responsible for, and nothing
below it. The materialize **engine semantics** (round-trip, per-world mapping,
world isolation, real provenance — `source_quad_ids` / `rule_iri` / the
`logic:assert` sentinel / `derivation_id`, the empty/whitespace short-circuit,
transitive derivation, routing/budget/disclosure) are pinned natively in the
`materialize_core` `#[test]` module of `crates/logic/src/materialize.rs` and no
longer run through Python. What remains here is the thinnest possible residue: a
proof that the binding marshals a `DerivedQuad` into a Python `dict` with the full
seam field set, plus the empty/fail-fast FFI contract.

## Retained dynamic tests

- `test_materialize_ffi_marshals_quad_dicts` — the binding returns a
  `{quads, preservation}` disclosure dict, each quad a `dict` carrying every seam
  field (`graph`, `subject`, `predicate`, `object`, `graph_component`,
  `derivation_id`, `rule_iri`, `source_quad_ids`, `profile`, `budget_status`),
  with `source_quad_ids` list-typed. Pure `DerivedQuad → dict` marshalling.
- `test_empty_case_trivial` — `materialize("", "")` returns zero quads (the trivial
  zero case, surfaced across the FFI).
- `test_compile_logic_empty_source_rejected` — `compile_logic("")` raises
  `ValueError` (the compiler's fail-fast contract, surfaced as a Python exception).

## Why it cannot be deleted or moved to Rust today

This is the PyO3 marshalling/error-surfacing seam, which Rust cannot test from the
inside: the extension is built with pyo3's `extension-module` feature, so it links
against the host interpreter and cannot start its own GIL under `cargo test` (the
same constraint documented in `crates/docs/tests/extract_roundtrip.rs`). The
engine logic below the seam is already Rust-authoritative and natively tested; only
the dict-marshalling and exception-surfacing at the boundary itself remain, and
those are only observable from the Python side.

**Retirement condition:** delete when the Python wheel surface that owns this seam
is removed — at that point there is no consumer of the marshalling and the seam
ceases to exist. Until then the check has no Rust home and is retained as the
minimal FFI-contract residue. `tests/_required_native.py` (the `require_gmeow_logic`
hard-import shim) is retained with it as its sole consumer and retires on the same
condition.
