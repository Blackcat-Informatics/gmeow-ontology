# Retention: `tests/test_diagnostics_config.py`

**Category:** Python tool algorithm

## What it tests

Tests for the shared diagnostics output config.

Retained dynamic tests:

- `test_defaults_resolve_with_no_flags_or_env` — Retained dynamic test.
- `test_auto_console_resolves_by_tty` — Retained dynamic test.
- `test_flag_beats_env_for_console` — Retained dynamic test.
- `test_env_honored_when_no_flag` — Retained dynamic test.
- `test_stem_precedence` — Retained dynamic test.
- `test_category_precedence` — Retained dynamic test.
- `test_artifacts_parsing` — Retained dynamic test.
- `test_unknown_artifact_token_hard_fails` — Retained dynamic test.
- `test_invalid_console_token_hard_fails` — Retained dynamic test.
- `test_directory_default_is_flat_dist_without_a_category` — Retained dynamic test.
- `test_directory_is_category_scoped_when_category_explicit` — Retained dynamic test.
- `test_directory_is_category_scoped_when_category_from_env` — Retained dynamic test.
- `test_explicit_directory_flag_wins_in_both_modes` — Retained dynamic test.
- `test_env_directory_wins_over_default_off_tty` — Retained dynamic test.
- `test_is_tty_none_falls_back_to_stderr_isatty` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Python-only algorithm or generated-artifact checks with no declarative slice-test equivalent.
