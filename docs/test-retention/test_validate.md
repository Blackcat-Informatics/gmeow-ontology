# Retention: `tests/test_validate.py`

**Category:** PyO3 seam

## What it tests

Tests for syntax checking and structural lint.

Retained dynamic tests:

- `test_check_syntax_on_sources` — Retained dynamic test.
- `test_validate_all_delegates_to_rust_native` — validate_all wraps gmeow_validate.
- `test_validate_all_skips_guide_anchor_on_rust_errors` — When Rust reports errors, Python skips the guide-anchor lint.
- `test_cached_validation_result_write_replaces_cleanly` — Retained dynamic test.
- `test_cached_validation_result_ignores_non_object_payload` — Retained dynamic test.
- `test_structural_lint_flags_missing_annotations` — Retained dynamic test.
- `test_check_sameas_ban_rejects_external_sameas` — Retained dynamic test.
- `test_check_sameas_ban_allows_internal_sameas` — Retained dynamic test.
- `test_check_sameas_ban_respects_allowlist` — Retained dynamic test.
- `test_check_sameas_ban_rejects_empty_paths` — An explicitly empty paths list is a caller bug — fail fast, not silently.

## Why it cannot be deleted or moved to Rust today

Tests Python-to-Rust marshalling and error surfacing for the PyO3 binding, which Rust cannot exercise from the inside.
