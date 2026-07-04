# Retention: `tests/test_engine_crosscheck.py`

**Category:** Oracle / Docker orchestration

## What it tests

The rdflib ↔ purrdf engine-equivalence gate.

Retained dynamic tests:

- `test_every_committed_query_agrees_across_engines` — rdflib and purrdf return identical answers for every committed query.
- `test_skips_are_only_multi_query_demo_files` — Any skipped file is skipped because BOTH engines reject it (not one-sided).
- `test_crosscheck_detects_a_real_divergence` — A query whose answer depends on a deliberately diverged store fails the gate.
- `test_crosscheck_decimal_values_compare_equal` — Value-based comparison: ``645.
- `test_term_keys_accept_native_compat_terms` — The oracle normalizer compares real rdflib and native compat terms by value.
- `test_build_report_maps_each_outcome_to_its_severity` — A diverged/skipped/agree triad maps to error/note/info findings.
- `test_build_report_all_agree_is_ok` — With no divergence the report is clean (info-only).
- `test_run_writes_artifacts_and_passes_on_the_real_surface` — ``run`` cross-checks the committed queries and writes JSON/SARIF/HTML.
- `test_run_uses_the_shared_artifact_writer` — ``run`` still orchestrates cross-checking, reporting, and artifact output.

## Why it cannot be deleted or moved to Rust today

Drives external reasoners or Docker-backed tooling that has no Rust twin by design.
