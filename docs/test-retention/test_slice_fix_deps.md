# Retention: `tests/test_slice_fix_deps.py`

**Category:** PyO3 seam

## What it tests

End-to-end test for `gmeow-dev slice-fix-deps` over the native binding.

Retained dynamic tests:

- `test_fix_deps_runs_against_native_binding` — The native binding exists, so compute_fix_deps runs (no hard-fail) and proposes the undeclared sliceB → sliceA dependency as a diff.
- `test_fix_deps_dry_run_writes_nothing` — A dry-run (apply=False) emits a diff but never mutates the manifests.
- `test_fix_deps_clean_set_has_no_proposals` — When sliceB already declares its dependency, no proposal is emitted.
- `test_native_binding_catalog_and_analyzer` — Drive the gmeow_slice binding directly: discovery + ownership analysis.
- `test_fix_deps_removes_terminal_dot_stale_dependency` — Removing a STALE dependency that is the LAST predicate (terminal `.
- `test_fix_deps_add_produces_wellformed_parseable_turtle` — An UNDECLARED edge add yields well-formed Turtle parseable by oxigraph that declares the new dependency on the correct (depending) manifest.
- `test_native_binding_detects_cross_slice_conflict` — Two slices each declaring isDefinedBy for the SAME term is a Conflict.

## Why it cannot be deleted or moved to Rust today

Tests Python-to-Rust marshalling and error surfacing for the PyO3 binding, which Rust cannot exercise from the inside.
