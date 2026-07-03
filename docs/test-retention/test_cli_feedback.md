# Retention: `tests/test_cli_feedback.py`

**Category:** Python CLI surface

## What it tests

CLI wiring for the ``gmeow-dev feedback`` diagnostics-output knobs.

Retained dynamic tests:

- `test_feedback_writes_all_artifacts_by_default` — Retained dynamic test.
- `test_feedback_artifacts_none_writes_only_the_bundle` — Retained dynamic test.
- `test_feedback_artifacts_none_preserves_exit_code_on_failure` — Retained dynamic test.
- `test_feedback_category_lands_in_sarif_automation_details` — Retained dynamic test.
- `test_feedback_env_category_is_honored` — Retained dynamic test.
- `test_feedback_silent_console_suppresses_finding_lines` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.
