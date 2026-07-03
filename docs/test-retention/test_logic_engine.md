# Retention: `tests/test_logic_engine.py`

**Category:** PyO3 seam

## What it tests

FFI-contract smoke tests for the PyO3 ``gmeow_logic.materialize`` binding.

Retained dynamic tests:

- `test_materialize_ffi_marshals_quad_dicts` — The binding returns a ``{quads, preservation}`` disclosure dict.
- `test_empty_case_trivial` — AC: empty materialize input → empty result (the trivial zero case).
- `test_compile_logic_empty_source_rejected` — The Rust compiler rejects an empty/whitespace-only source (fail-fast).

## Why it cannot be deleted or moved to Rust today

Tests Python-to-Rust marshalling and error surfacing for the PyO3 binding, which Rust cannot exercise from the inside.
