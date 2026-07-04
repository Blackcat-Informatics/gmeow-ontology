# Retention: `tests/test_mereology.py`

**Category:** Merged-graph guard

## What it tests

Universal mereology spine — structural TBox well-formedness.

Retained dynamic tests:

- `test_no_winner_or_cardinality_terms_for_parts` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
