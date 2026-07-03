# Retention: `tests/test_teleology.py`

**Category:** Merged-graph guard

## What it tests

The teleology core slice.

Retained dynamic tests:

- `test_no_preferred_or_primary_goal_terms` — No preferredGoal / primaryIntention selectors exist.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
