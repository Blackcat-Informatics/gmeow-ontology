# Retention: `tests/test_logic_compile_diagnostics.py`

**Category:** Python tool algorithm

## What it tests

The logic-compile diagnostics surface.

Retained dynamic tests:

- `test_compile_logic_returns_a_native_diagnostics_report` — compile_logic exposes a live ``diagnostics_report`` (not a dict list).
- `test_clean_source_compiles_to_an_ok_report` — The committed logic: source compiles without error findings.
- `test_findings_carry_the_logic_compile_namespace` — Any parse finding is tool-tagged and code-prefixed by the Rust core.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
