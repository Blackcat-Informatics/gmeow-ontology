# Retention: `tests/test_shacl_engine.py`

**Status:** Migrated to Rust (`crates/validate/tests/shacl_engine.rs`) by issue 1314 Task 8.

The Python N-Triples→SHACL seam tests have been retired; the native Rust twin
covers the same adapter contract:

- `test_version_is_reported`
- `test_conforming_graph_has_no_results`
- `test_violation_partitions_to_errors_with_stable_line`
- `test_warning_severity_buckets_to_warnings`
- `test_partition_results_prefixes_box_roles_when_present`
- `test_partition_results_uses_hash_iri_local_name_for_unknown_roles`
- `test_parse_error_hard_fails`
- `test_term_normalization`

The remaining Python `run_shacl` helper in `src/gmeow_tools/validate.py` now
calls `purrdf.purrdf_native.shacl.validate` directly and applies the same
legacy partitioning logic inline.
