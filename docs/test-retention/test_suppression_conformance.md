# Retention: `tests/test_suppression_conformance.py`

**Category:** Merged-graph guard

## What it tests

Generated leak-conformance suite.

Retained dynamic tests:

- `test_suppressed_canary_never_leaks` — displayable false never surfaces — in ANY profile, present or future.
- `test_precise_coarsened_values_never_leak` — A coarsenTo-marked place's precise coordinates appear in no profile.
- `test_control_canary_proves_coverage` — The displayable twin DOES project — the leak tests are not vacuous.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
