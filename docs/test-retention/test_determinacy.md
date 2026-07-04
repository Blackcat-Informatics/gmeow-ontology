# Retention: `tests/test_determinacy.py`

**Category:** Merged-graph guard

## What it tests

The universal determinacy vocabulary.

Retained dynamic tests:

- `test_no_preferred_or_primary_term_is_declared` — No GMEOW vocabulary term is a preferred/primary selector.

## Why it cannot be deleted or moved to Rust today

Whole-merged-graph sweeps over terms declared across many slice modules; cannot be faithfully scoped to a single slice module.
