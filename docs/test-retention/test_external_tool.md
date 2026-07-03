# Retention: `tests/test_external_tool.py`

**Category:** Python tool algorithm

## What it tests

Tests for wrapping external gate tools as canonical findings.

Retained dynamic tests:

- `test_success_yields_an_empty_report` — Retained dynamic test.
- `test_failure_yields_one_error_finding_with_raw_log` — Retained dynamic test.
- `test_large_log_is_digested_deterministically` — Retained dynamic test.
- `test_multibyte_log_digest_has_no_negative_elision_or_overlap` — Retained dynamic test.
- `test_empty_argv_is_a_finding_not_a_crash` — Retained dynamic test.
- `test_run_external_tool_returns_exact_exit_code` — Retained dynamic test.
- `test_run_external_tool_timeout_is_a_finding_with_rc_124` — Retained dynamic test.
- `test_supplied_env_is_merged_onto_parent_env` — Retained dynamic test.
- `test_argv_with_spaces_is_shell_quoted_in_detail` — Retained dynamic test.
- `test_small_log_is_kept_verbatim` — Retained dynamic test.
- `test_run_external_tool_captures_a_real_failure` — Retained dynamic test.
- `test_run_external_tool_missing_binary_is_a_finding_not_a_crash` — Retained dynamic test.
- `test_cli_external_tool_failure_exit_code_and_sarif` — Retained dynamic test.
- `test_cli_external_tool_category_routes_to_dist_diagnostics_subdir` — Retained dynamic test.
- `test_cli_external_tool_success_exit_zero` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
