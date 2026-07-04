# Retention: `tests/test_coreference.py`

**Category:** Merged-graph guard

## What it tests

Universal identity/coreference guards.

Retained dynamic tests:

- `test_no_preferred_or_primary_coreference_terms` — Retained dynamic test.

## Why it cannot be deleted or moved to Rust today

Whole-graph absence sweep over banned IRI names; subjects not home-asserted in module.ttl.
