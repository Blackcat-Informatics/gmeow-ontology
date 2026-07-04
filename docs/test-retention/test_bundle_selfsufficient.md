# Retention: `tests/test_bundle_selfsufficient.py`

**Category:** Python CLI surface

## What it tests

The bundle is self-sufficient: transpile runs from the wheel, no repo (#bundle).

Retained dynamic tests:

- `test_transpile_runs_purely_from_the_bundle` — Wheel mode (every repo path blinded) transpiles non-trivially from the bundle.
- `test_wheel_mode_matches_repo_mode_exactly` — The bundle is a faithful stand-in: blinded run == repo run, metric for metric.

## Why it cannot be deleted or moved to Rust today

The CLIs under test are Typer applications; their behavior is exercised through CliRunner and subprocess integration, which is inherently Python-only surface.
