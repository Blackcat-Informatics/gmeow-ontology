# Retention: `tests/test_okf_import.py`

**Category:** Oracle / Docker orchestration

## What it tests

Acceptance tests for the OKF (Open Knowledge Format) import lane.

Retained dynamic tests:

- `test_gts_from_okf_folds_our_bundle` — The bundle we emit is conformant: ``gts from-okf`` folds it without error.
- `test_lift_roundtrips_recognized_subset_and_retains_unknown` — Lift maps the recognized okf: subset and retains unknown keys verbatim.

## Why it cannot be deleted or moved to Rust today

Drives external reasoners or Docker-backed tooling that has no Rust twin by design.
