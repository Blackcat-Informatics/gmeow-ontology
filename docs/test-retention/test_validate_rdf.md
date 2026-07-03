# Retention: `tests/test_validate_rdf.py`

**Category:** Python CLI surface

## What it tests

Acceptance tests for the repo-free ``gmeow validate <data>`` RDF path.

Retained dynamic tests:

- `test_validate_rdf_reports_two_errors_one_warning_with_locations` — Retained dynamic test.
- `test_validate_rdf_human_format_exits_nonzero` — Retained dynamic test.
- `test_validate_rdf_sarif_is_well_formed` — Retained dynamic test.
- `test_validate_rdf_clean_file_passes` — Retained dynamic test.
- `test_validate_unknown_extension_hard_fails` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.
