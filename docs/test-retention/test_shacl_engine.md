# Retention: `tests/test_shacl_engine.py`

**Category:** Python tool algorithm

## What it tests

Unit tests for the N-Triples→gmeow_shacl validation seam.

Retained dynamic tests:

- `test_version_is_reported` — Retained dynamic test.
- `test_conforming_graph_has_no_results` — Retained dynamic test.
- `test_violation_partitions_to_errors_with_stable_line` — Retained dynamic test.
- `test_warning_severity_buckets_to_warnings` — Retained dynamic test.
- `test_partition_results_prefixes_box_roles_when_present` — Retained dynamic test.
- `test_partition_results_uses_hash_iri_local_name_for_unknown_roles` — Retained dynamic test.
- `test_parse_error_hard_fails` — Retained dynamic test.
- `test_term_normalization` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
