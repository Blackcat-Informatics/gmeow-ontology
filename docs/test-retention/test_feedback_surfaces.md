# Retention: `tests/test_feedback_surfaces.py`

**Category:** Oracle / Docker orchestration

## What it tests

The `gmeow-dev feedback` surface fold loop.

Retained dynamic tests:

- `test_surface_reports_covers_every_migrated_surface` — The fold table must list exactly the migrated surfaces — no drift.
- `test_feedback_folds_all_surface_findings` — `_fold_surfaces` merges every surface's findings into the report.
- `test_feedback_surface_failure_is_isolated` — One surface raising leaves the others intact, surfaces the error loudly, and the bundle still self-attests.

## Why it cannot be deleted or moved to Rust today

Drives external reasoners or Docker-backed tooling that has no Rust twin by design.
