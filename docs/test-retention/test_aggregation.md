# Retention: `tests/test_aggregation.py`

**Category:** Merged-graph guard

## What it tests

Cross-slice guards for the spatial aggregation module.

Retained dynamic tests:

- `test_contains_place_exists_and_is_inverse` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
