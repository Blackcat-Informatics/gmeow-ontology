# Retention: `tests/test_logic_cli.py`

**Category:** Python CLI surface

## What it tests

Tests for ``gmeow logic compile`` CLI and ``logic_compile`` library.

Retained dynamic tests:

- `test_logic_compile_check_no_drift` — --check exits 0 when committed artifacts match the source.
- `test_logic_compile_mode_owl_el` — --mode owl-el writes only the EL back-end (does not raise).
- `test_logic_compile_unknown_mode_fails` — An unknown --mode exits non-zero with an error message.
- `test_logic_query_recursive_ancestor` — `logic query` resolves a tabled recursive goal to the transitive closure.
- `test_logic_query_cut_rejected_outside_procedural` — Cut under a non-ProceduralPrologProfile profile hard-fails (AC-2 gate).
- `test_logic_compile_help` — ``gmeow logic compile --help`` exits 0 and describes the command.
- `test_reason_mode_native_exits_clean` — ``reason --mode native`` reasons the bundle Docker-free and exits 0.
- `test_reason_unknown_mode_fails` — An unknown ``--mode`` exits non-zero (only native/docker are valid).

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.
